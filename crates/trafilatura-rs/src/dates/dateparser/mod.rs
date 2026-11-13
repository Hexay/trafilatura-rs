//! `DateDataParser.get_date_data(s)` with htmldate's settings (dateparser 1.5.0): a port of the whole call
//! (date.rs, from locales.txt and unicode.txt), and in front of it, the proof below.
//!
//! The proof: when `get_date_data(s)` returns no date because no locale accepts the string. A locale accepts a string only if every token it splits off is a dictionary word, a
//! relative-time expression or a number. Words are maximal runs of `[^\W\d_]` letters of what the locale sees
//! (chars.rs). With word spacing, a word ends up whole inside one token or one simplification match, so a word
//! outside the locale's word list (tables.txt, from harness/gen_dateparser_tables.py) rules the locale out; without
//! word spacing (ja, zh, ...), a word that is no concatenation of the locale's pieces does. Rewrites that may splice
//! words or copy any letters (ja, zh, ko, he) only apply to strings with their letters; for those, a letter the
//! locale never uses rules it out instead. Timezone names, which `pop_tz_offset_from_string` may strip before the
//! second applicability check, never count. This proves a None result for strings in no language at all, which
//! cost dateparser a pass over all ~200 locales; anything else is left to Python.
mod chars;
mod date;
mod dictionary;
mod locales;
mod parser;
mod quick;
mod sanitize;
mod strptime;
mod tables;
mod tokens;
mod translate;
mod tz;
mod unicode;

use crate::py::datetime::DateTime;
pub(crate) use chars::versions;
use std::collections::HashSet;
use tables::{Locale, Vocabulary, TABLES};

/// `get_date_data(s)["date_obj"]` (see date.rs), `today` being the ordinal of the local date; None when unknown.
/// Needs the `unicodedata` and `regex` of `versions()`, like the non-ASCII shortcut.
pub(super) fn date_obj(s: &str, today: i64) -> Option<Option<DateTime>> {
    date::date_obj(s, today)
}

/// Whether locales.txt and unicode.txt come from the same dateparser data, `unicodedata` and `regex` as chars.txt.
pub(super) fn data_consistent() -> bool {
    let (unidata, regex) = versions();
    let l = locales::DATA.versions;
    unicode::versions() == (unidata, regex) && l == (regex, unidata)
}

/// dateparser's month names of all locales, folded (htmldate's `month_words()`).
pub(super) fn month_words() -> &'static HashSet<&'static str> {
    &TABLES.months
}

/// The words of a sanitized date string that both applicability checks see, and every letter it has.
struct Words {
    evidence: Vec<String>,
    letters: HashSet<char>,
}

impl Words {
    fn of(s: &str) -> Option<Words> {
        let chars: Vec<char> = s.chars().collect();
        let tz: Vec<Option<char>> = chars.iter().map(|&c| chars::tz_fold(c)).collect();
        // `(?:UTC|GMT)[+-]hh:?mm.*$` offsets swallow everything after them
        let at = |i: usize, z: &str| z.chars().enumerate().all(|(k, c)| tz.get(i + k) == Some(&Some(c)));
        let cut = (0..chars.len()).find(|&i| at(i, "utc") || at(i, "gmt")).unwrap_or(chars.len());
        let mut words = Words { evidence: Vec::new(), letters: HashSet::new() };
        let (mut run, mut start) = (String::new(), None);
        for i in 0..=chars.len() {
            if i < chars.len() && chars::push_letters(chars[i], &mut run)? {
                start.get_or_insert(i);
                continue;
            }
            let Some(st) = start.take() else { continue };
            let raw: Option<String> = tz[st..i].iter().copied().collect();
            words.letters.extend(run.chars());
            if i <= cut && !run.is_empty() && !raw.is_some_and(|w| TABLES.tz.contains(w.as_str())) {
                words.evidence.push(std::mem::take(&mut run));
            }
            run.clear();
        }
        Some(words)
    }

    fn translated(&self) -> Words {
        let tr = |c: char| *TABLES.variants.get(&c).unwrap_or(&c);
        Words {
            evidence: self.evidence.iter().map(|w| w.chars().map(tr).collect()).collect(),
            letters: self.letters.iter().map(|&c| tr(c)).collect(),
        }
    }
}

fn concatenation_of(word: &str, pieces: &HashSet<&str>) -> bool {
    let bounds: Vec<usize> = word.char_indices().map(|(i, _)| i).chain(std::iter::once(word.len())).collect();
    let mut reach = vec![false; bounds.len()];
    reach[0] = true;
    for end in 1..bounds.len() {
        reach[end] = (0..end).any(|start| reach[start] && pieces.contains(&word[bounds[start]..bounds[end]]));
    }
    reach[bounds.len() - 1]
}

fn excluded(index: usize, locale: &Locale, words: &Words) -> bool {
    let rewritten = match &locale.guard {
        None => false,
        Some(None) => true,
        Some(Some(guard)) => words.letters.iter().any(|c| guard.contains(c)),
    };
    if rewritten {
        let Some(ok) = &locale.letters else { return false };
        return words.evidence.iter().any(|w| w.chars().any(|c| !ok.contains(&c)));
    }
    match &locale.vocabulary {
        Vocabulary::Pieces(pieces) => words.evidence.iter().any(|w| !concatenation_of(w, pieces)),
        Vocabulary::Words => words
            .evidence
            .iter()
            .any(|w| TABLES.words.get(w.as_str()).is_none_or(|bits| bits[index / 64] >> (index % 64) & 1 == 0)),
    }
}

/// True when `get_date_data(s)["date_obj"]` is certainly None; false when unknown. Without `unicode` (the
/// installed Python or `regex` is not the one chars.txt was generated with), only ASCII strings are tried.
pub(super) fn no_locale_applies(s: &str, unicode: bool) -> bool {
    let t = &*TABLES;
    if t.any || !(unicode || s.is_ascii()) {
        return false;
    }
    let Some(words) = sanitize::sanitize(s).and_then(|s| Words::of(&s)) else { return false };
    if words.evidence.is_empty() {
        return false;
    }
    let variant = t.locales.iter().any(|l| l.variant).then(|| words.translated());
    t.locales.iter().enumerate().all(|(i, locale)| match (&variant, locale.variant) {
        (Some(v), true) => excluded(i, locale, v),
        _ => excluded(i, locale, &words),
    })
}
