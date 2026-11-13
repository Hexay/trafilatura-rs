//! `DateDataParser.get_date_data(s)["date_obj"]` (dateparser.date) with htmldate's parser: no date formats, so
//! only the absolute-time parser runs, once per applicable locale and translation. None defers to Python.
use super::locales::{global, Locale, DATA};
use super::parser::{parse, Stop, Today};
use super::sanitize::sanitize;
use super::tables::TABLES;
use super::translate::{is_applicable, translate};
use super::tz::pop_tz;
use super::unicode::{is, py_strip, Class};
use super::{excluded, Words};
use crate::py::datetime::DateTime;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const VARIANT_LETTERS: [char; 4] = ['ی', 'ک', 'ي', 'ك'];
const DEFAULT_DATE_ORDER: &str = "MDY";

/// Per entry of `DATA.order`: the tables.txt locales proving its applicability check (and its twin's) false.
static TABLE_INDEX: LazyLock<Vec<(usize, Option<usize>)>> = LazyLock::new(|| {
    let mut out: Vec<(usize, Option<usize>)> = Vec::new();
    for (i, l) in TABLES.locales.iter().enumerate() {
        match out.last_mut() {
            Some(last) if l.variant => last.1 = Some(i),
            _ => out.push((i, None)),
        }
    }
    out
});

/// A locale to try: its index, the locale whose applicability decides, and the tables.txt locale for the proof.
struct Candidate {
    locale: usize,
    applicable: usize,
    table: Option<usize>,
}

fn candidates(s: &str) -> Vec<Candidate> {
    let index = &*TABLE_INDEX;
    let consistent = index.len() == DATA.order.len();
    let table = |k: usize, twin: bool| consistent.then(|| if twin { index[k].1 } else { Some(index[k].0) }).flatten();
    let mut out: Vec<Candidate> = DATA
        .order
        .iter()
        .enumerate()
        .map(|(k, &(l, a))| Candidate { locale: l, applicable: a, table: table(k, false) })
        .collect();
    if s.chars().any(|c| VARIANT_LETTERS.contains(&c)) {
        let twins = DATA.order.iter().enumerate().filter(|(_, &(l, a))| l != a);
        out.extend(
            twins.map(|(k, &(l, _))| Candidate { locale: l, applicable: l, table: table(k, true) }).collect::<Vec<_>>(),
        );
    }
    out
}

fn date_order(locale: &Locale) -> &'static str {
    locale.date_order.unwrap_or(DEFAULT_DATE_ORDER)
}

fn meridiem(part: &str, hour: i64) -> &'static str {
    match part {
        "early_morning" | "morning" => "am",
        "night" if !(6..=11).contains(&hour) => "am",
        _ => "pm",
    }
}

/// `_replace_part_of_day(translated, PartsOfDay())[0]`.
fn replace_part_of_day(translated: &str) -> Option<String> {
    let Some(caps) = global("part_of_day").search(translated) else { return Some(translated.to_owned()) };
    let (m, part) = (caps.get(0).expect("group 0"), caps.get(1).expect("the part").as_str());
    let (before, after) = (&translated[..m.start()], &translated[m.end()..]);
    if let Some(n) = global("trailing_number").search(before).filter(|n| n.get(1).is_none()) {
        let hour: i64 = n.get(2).expect("the hour").as_str().parse().ok()?;
        if hour > 12 {
            return Some(translated.to_owned());
        }
        let minutes = n.get(3).map_or(":00", |g| g.as_str());
        let start = n.get(0).expect("group 0").start();
        return Some(format!("{}{hour}{minutes} {} {after}", &before[..start], meridiem(part, hour)));
    }
    let rest = format!("{before} {after}");
    if py_strip(&rest).is_empty() {
        return Some(translated.to_owned());
    }
    if global("time").search(&rest).is_some() {
        return Some(rest);
    }
    let time = match part {
        "early_morning" => "03:00:00",
        "morning" => "09:00:00",
        "afternoon" => "15:00:00",
        "evening" => "18:00:00",
        _ => "20:00:00",
    };
    Some(format!("{rest} {time}"))
}

/// One `get_date_data` call: the outcome keys that failed, shared by all locales, and the timezone-stripped strings.
struct Run {
    today: Today,
    failed: HashSet<(String, &'static str)>,
    popped: HashMap<String, String>,
}

impl Run {
    /// `date_parser._parse_with_parts(translated, _parse_absolute, date_order=...)`, as a naive datetime.
    fn absolute(&mut self, translated: &str, date_order: &str) -> Result<DateTime, Stop> {
        if py_strip(translated).is_empty() {
            return Err(Stop::Fail);
        }
        let unbraced: String = translated.chars().filter(|c| !"{}()<>[]".contains(*c)).collect();
        let popped = self.popped.entry(unbraced).or_insert_with_key(|s| pop_tz(s).0);
        parse(popped, date_order, self.today)
    }

    /// `_DateLocaleParser._parse_unless_failed` of one translation: Some(None) when it gives no date.
    fn parse_translation(&mut self, translated: &str, order: &'static str) -> Option<Option<DateTime>> {
        let key = (replace_part_of_day(translated)?, order);
        if self.failed.contains(&key) {
            return Some(None);
        }
        match self.absolute(&key.0, order) {
            Ok(date) => Some(Some(date)),
            Err(Stop::Defer) => None,
            Err(Stop::Fail) => {
                self.failed.insert(key);
                Some(None)
            }
        }
    }

    /// `_parse_using_locale(locale, s, alternatives=...)`.
    fn parse_using_locale(&mut self, locale: &Locale, s: &str, alternatives: bool) -> Option<Option<DateTime>> {
        let order = date_order(locale);
        if !alternatives {
            let translated = translate(locale, s, 0)?.expect("a default translation");
            return self.parse_translation(&translated, order);
        }
        for alternative in 1.. {
            let Some(translated) = translate(locale, s, alternative)? else { return Some(None) };
            if let Some(date) = self.parse_translation(&translated, order)? {
                return Some(Some(date));
            }
        }
        unreachable!("alternatives run out")
    }
}

/// `get_date_data(s)["date_obj"]` with htmldate's parser; `today` is the local date's ordinal.
pub(super) fn date_obj(s: &str, today: Today) -> Option<Option<DateTime>> {
    let s = sanitize(s)?;
    let words = Words::of(&s);
    let variant_words = words.as_ref().map(Words::translated);
    let stripped: OnceCell<Option<String>> = OnceCell::new();
    let letter_free = !s.chars().any(|c| is(Class::Alpha, c));
    let (mut orders, mut seen) = (HashSet::new(), HashSet::new());
    let mut run = Run { today, failed: HashSet::new(), popped: HashMap::new() };
    let mut tried: Vec<usize> = Vec::new();
    for c in candidates(&s) {
        let locale = &DATA.locales[c.locale];
        if seen.contains(&c.locale) || (letter_free && orders.contains(&locale.date_order)) {
            continue;
        }
        let proof = c.table.filter(|_| !TABLES.any).and_then(|t| {
            let variant = TABLES.locales[t].variant;
            Some((t, if variant { variant_words.as_ref()? } else { words.as_ref()? }))
        });
        if proof.is_some_and(|(t, w)| excluded(t, &TABLES.locales[t], w)) {
            continue;
        }
        let applicable = &DATA.locales[c.applicable];
        // the timezone-stripped string only when the string itself does not apply, as Python's generator does
        if !is_applicable(applicable, &s)? {
            let stripped = stripped.get_or_init(|| Some(pop_tz(&s).0).filter(|t| *t != s));
            if !stripped.as_ref().map_or(Some(false), |t| is_applicable(applicable, t))? {
                continue;
            }
        }
        orders.insert(locale.date_order);
        seen.insert(c.locale);
        tried.push(c.locale);
        if let Some(date) = run.parse_using_locale(locale, &s, false)? {
            return Some(Some(date));
        }
    }
    for l in tried {
        if let Some(date) = run.parse_using_locale(&DATA.locales[l], &s, true)? {
            return Some(Some(date));
        }
    }
    Some(None)
}
