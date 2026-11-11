//! htmldate/extractors.py: `try_date_expr` and its custom parsers.
use super::regexes::{
    DAY_TOKEN, DISCARD_PATTERNS, LONG_TEXT_PATTERN, TEXT_DATE_PATTERN, YMD_NO_SEP_PATTERN, YMD_PATTERN, YM_PATTERN,
};
use super::validators::{correct_year, validate_and_convert};
use super::{caught, cslice, isdigit, Env, MAX_SEGMENT_LEN};
use crate::dom::{Error, Result};
use crate::py::datetime::DateTime;
use crate::py::int::py_int;
use crate::py::pyre::{group, named};
use crate::py::{is_digit, Collapse};
use std::collections::HashMap;
use std::sync::LazyLock;

pub(super) const FREE_TEXT_EXPRESSIONS: &str =
    ".//*[self::div or self::h2 or self::h3 or self::h4 or self::li or self::p or self::span or self::time or self::ul]/text()";

/// English, French, German, Indonesian and Turkish month names.
const MONTHS: [&[&str]; 12] = [
    &["jan", "januar", "jänner", "january", "januari", "janvier", "ocak", "oca"],
    &["feb", "februar", "feber", "february", "februari", "février", "şubat", "şub"],
    &["mar", "mär", "märz", "march", "maret", "mart", "mars"],
    &["apr", "april", "avril", "nisan", "nis"],
    &["may", "mai", "mei", "mayıs"],
    &["jun", "juni", "june", "juin", "haziran", "haz"],
    &["jul", "juli", "july", "juillet", "temmuz", "tem"],
    &["aug", "august", "agustus", "ağustos", "ağu", "aout"],
    &["sep", "september", "septembre", "eylül", "eyl"],
    &["oct", "oktober", "october", "octobre", "okt", "ekim", "eki"],
    &["nov", "november", "kasım", "kas", "novembre"],
    &["dec", "dez", "dezember", "december", "desember", "décembre", "aralık", "ara"],
];

/// `_fold_month_name`: str.lower() alone mishandles the Turkish dotless i.
fn fold_month_name(token: &str) -> String {
    token.replace(['ı', 'İ'], "i").to_lowercase()
}

static MONTH_NUMBERS: LazyLock<HashMap<String, i128>> = LazyLock::new(|| {
    (1..).zip(MONTHS).flat_map(|(i, names)| names.iter().map(move |n| (fold_month_name(n), i))).collect()
});

fn int(s: Option<&str>) -> Result<i128> {
    py_int(s.unwrap_or_default())
}

/// `try_swap_values`.
fn try_swap_values(day: i128, month: i128) -> (i128, i128) {
    if month > 12 && day <= 12 {
        (month, day)
    } else {
        (day, month)
    }
}

/// `_build_dmy`.
pub(super) fn build_dmy(day: i128, month: i128, year: i128) -> Result<DateTime> {
    let year = correct_year(year);
    let (day, month) = try_swap_values(day, month);
    DateTime::ymd(year, month, day)
}

/// `regex_parse`.
pub(super) fn regex_parse(string: &str) -> Result<Option<DateTime>> {
    let mut pos = 0;
    while let Some(m) = LONG_TEXT_PATTERN.search_from(string, pos) {
        let first = named(&m, "month").filter(|s| !s.is_empty());
        let mut word = fold_month_name(first.or(named(&m, "month2")).unwrap_or_default());
        if first.is_some() {
            let chars: Vec<char> = word.chars().collect();
            let tail = &chars[chars.len().saturating_sub(12)..];
            word = (0..tail.len().saturating_sub(2))
                .map(|i| tail[i..].iter().collect::<String>())
                .find(|w| MONTH_NUMBERS.contains_key(w))
                .unwrap_or_else(|| tail.iter().collect());
        }
        let whole = m.get(0).expect("group 0");
        let Some(&month) = MONTH_NUMBERS.get(&word) else {
            pos = whole.start() + string[whole.start()..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let built = (|| {
            let day = int(named(&m, "day").filter(|s| !s.is_empty()).or(named(&m, "day2")))?;
            let year = int(named(&m, "year").filter(|s| !s.is_empty()).or(named(&m, "year2")))?;
            build_dmy(day, month, year)
        })();
        match caught(built)? {
            Some(d) => return Ok(Some(d)),
            None => pos = whole.end(),
        }
    }
    Ok(None)
}

/// `_parse_yyyymmdd`.
fn parse_yyyymmdd(digits: &str) -> Result<DateTime> {
    DateTime::ymd(py_int(cslice(digits, 0, 4))?, py_int(cslice(digits, 4, 6))?, py_int(cslice(digits, 6, 8))?)
}

/// `custom_parse`: the `_date_candidates` generator, in priority order, until one validates.
pub(super) fn custom_parse(env: &Env, string: &str, outputformat: &str) -> Result<Option<String>> {
    let accept = |c: Option<DateTime>| validate_and_convert(c, outputformat, env);
    // 1. shortcut
    if isdigit(cslice(string, 0, 4)) {
        let candidate = if isdigit(cslice(string, 4, 8)) {
            caught(parse_yyyymmdd(string))?
        } else {
            match DateTime::fromisoformat(string) {
                Ok(d) => Some(d),
                Err(Error::Value(_)) => env.hooks.dateutil_parse(string)?,
                Err(e) => return Err(e),
            }
        };
        if let Some(r) = accept(candidate) {
            return Ok(Some(r));
        }
    }
    // 2. YYYYMMDD
    if let Some(m) = YMD_NO_SEP_PATTERN.search(string) {
        if let Some(r) = accept(caught(parse_yyyymmdd(group(&m, 1).unwrap_or_default()))?) {
            return Ok(Some(r));
        }
    }
    // 3. Y-M-D and D-M-Y
    if let Some(m) = YMD_PATTERN.search(string) {
        let candidate = (|| {
            if named(&m, "day").is_some() {
                DateTime::ymd(int(named(&m, "year"))?, int(named(&m, "month"))?, int(named(&m, "day"))?)
            } else {
                build_dmy(int(named(&m, "day2"))?, int(named(&m, "month2"))?, int(named(&m, "year2"))?)
            }
        })();
        if let Some(r) = accept(caught(candidate)?) {
            return Ok(Some(r));
        }
    }
    // 4. Y-M and M-Y
    if let Some(m) = YM_PATTERN.search(string) {
        let year = named(&m, "year").filter(|s| !s.is_empty()).or(named(&m, "year2"));
        let month = named(&m, "month").filter(|s| !s.is_empty()).or(named(&m, "month2"));
        if let Some(r) = accept(caught((|| DateTime::ymd(int(year)?, int(month)?, 1))())?) {
            return Ok(Some(r));
        }
    }
    // 5. text patterns
    Ok(accept(regex_parse(string)?))
}

/// `try_date_expr(string, outputformat, options.extensive, options.min, options.max)`.
pub(super) fn try_date_expr(env: &Env, string: Option<&str>, outputformat: &str) -> Result<Option<String>> {
    let Some(string) = string.filter(|s| !s.is_empty()) else { return Ok(None) };
    let trimmed = Collapse::of(string);
    let string = cslice(&trimmed, 0, MAX_SEGMENT_LEN);
    let digits = string.chars().filter(|&c| is_digit(c)).count();
    if string.is_empty() || !(4..=18).contains(&digits) {
        return Ok(None);
    }
    if DISCARD_PATTERNS.is_match(string) {
        return Ok(None);
    }
    if let Some(r) = custom_parse(env, string, outputformat)? {
        return Ok(Some(r));
    }
    // dateparser only for plausible dates; the hook checks has_date_cue itself
    if env.extensive && TEXT_DATE_PATTERN.is_match(string) && DAY_TOKEN.is_match(string) {
        let external = env.hooks.external_date(string, env.min.year, env.max.year)?;
        return Ok(validate_and_convert(external, outputformat, env));
    }
    Ok(None)
}

/// `try_date_expr_opts`.
pub(super) fn try_date_expr_opts(env: &Env, string: Option<&str>) -> Result<Option<String>> {
    try_date_expr(env, string, env.format)
}
