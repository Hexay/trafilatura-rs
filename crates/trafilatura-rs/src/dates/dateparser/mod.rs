//! When `DateDataParser.get_date_data(s)` (htmldate's settings, dateparser 1.5.0) returns no date because no locale
//! accepts the string. A locale accepts a string only if every token it splits off is a dictionary word, a
//! relative-time expression or a number. With word spacing, a maximal run of letters can only end up whole inside
//! one token or one simplification match, so a word outside the locale's word list (tables.txt, from
//! harness/gen_dateparser_tables.py) rules the locale out; without word spacing (ja, zh, ...), a word that is no
//! concatenation of the locale's pieces does. Timezone names, which `pop_tz_offset_from_string` may strip before the
//! second applicability check, never count. This proves a None result for strings in no language at all, which cost
//! dateparser a pass over all ~200 locales; anything else is left to Python.
mod sanitize;

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const TABLES: &str = include_str!("tables.txt");

struct Tables {
    /// Spacing locales accepting each word, as a bitset.
    words: HashMap<&'static str, Vec<u64>>,
    spacing: usize,
    pieces: Vec<HashSet<&'static str>>,
    /// Some locale's words could not be bounded: nothing can be proved.
    any: bool,
    tz: HashSet<&'static str>,
    months: HashSet<&'static str>,
}

static TABLES_DATA: LazyLock<Tables> = LazyLock::new(|| {
    let mut t = Tables {
        words: HashMap::new(),
        spacing: 0,
        pieces: Vec::new(),
        any: false,
        tz: HashSet::new(),
        months: HashSet::new(),
    };
    let mut spacing_words = Vec::new();
    for line in TABLES.lines().filter(|l| !l.starts_with('#')) {
        let mut fields = line.split(' ');
        match fields.next() {
            Some("L") => match fields.nth(1) {
                Some("words") => spacing_words.push(fields.collect::<Vec<_>>()),
                Some("pieces") => t.pieces.push(fields.collect()),
                _ => t.any = true,
            },
            Some("T") => t.tz.extend(fields),
            Some("M") => t.months.extend(fields),
            _ => panic!("malformed dateparser table line {line:?}"),
        }
    }
    t.spacing = spacing_words.len();
    let blocks = t.spacing.div_ceil(64);
    for (i, words) in spacing_words.iter().enumerate() {
        for w in words {
            t.words.entry(w).or_insert_with(|| vec![0; blocks])[i / 64] |= 1 << (i % 64);
        }
    }
    t
});

/// dateparser's month names of all locales, folded (htmldate's `month_words()`).
pub(super) fn month_words() -> &'static HashSet<&'static str> {
    &TABLES_DATA.months
}

fn concatenation_of(word: &str, pieces: &HashSet<&str>) -> bool {
    let mut reach = vec![false; word.len() + 1];
    reach[0] = true;
    for end in 1..=word.len() {
        reach[end] = (0..end).any(|start| reach[start] && pieces.contains(&word[start..end]));
    }
    reach[word.len()]
}

/// True when `get_date_data(s)["date_obj"]` is certainly None; false when unknown.
pub(super) fn no_locale_applies(s: &str) -> bool {
    let t = &*TABLES_DATA;
    if t.any || !s.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return false;
    }
    let Some(sanitized) = sanitize::sanitize(s) else { return false };
    let lower = sanitized.to_ascii_lowercase();
    // `(?:UTC|GMT)[+-]hh:?mm.*$` offsets swallow everything after them
    let cut = ["utc", "gmt"].iter().filter_map(|z| lower.find(z)).min().unwrap_or(lower.len());
    let mut candidates = Vec::new();
    let mut start = None;
    for (i, c) in lower.char_indices().chain(std::iter::once((lower.len(), ' '))) {
        match (c.is_ascii_alphabetic(), start) {
            (true, None) => start = Some(i),
            (false, Some(st)) => {
                let w = &lower[st..i];
                if i <= cut && !t.tz.contains(w) {
                    candidates.push(w);
                }
                start = None;
            }
            _ => {}
        }
    }
    if candidates.is_empty() {
        return false;
    }
    let blocks = t.spacing.div_ceil(64);
    let mut alive = vec![u64::MAX; blocks];
    if !t.spacing.is_multiple_of(64) {
        alive[blocks - 1] = (1 << (t.spacing % 64)) - 1;
    }
    for w in &candidates {
        match t.words.get(w) {
            Some(ok) => alive.iter_mut().zip(ok).for_each(|(a, o)| *a &= o),
            None => alive.fill(0),
        }
    }
    alive.iter().all(|&a| a == 0) && t.pieces.iter().all(|p| candidates.iter().any(|w| !concatenation_of(w, p)))
}
