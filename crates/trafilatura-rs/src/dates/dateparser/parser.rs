//! `dateparser.parser._parser._parse_with_parts` with htmldate's settings (STRICT_PARSING, PREFER_DATES_FROM past,
//! no DATE_ORDER setting, so STRICT_DATE_ORDER is "none"). Err(Fail) is a ValueError, Err(Defer) runs Python.
use super::strptime::time;
use super::tokens::{tokenize, Fields, Tok};
use super::unicode::py_strip;
use crate::py::datetime::{ymd_to_ord, DateTime};

#[derive(Debug, PartialEq)]
pub(super) enum Stop {
    Fail,
    Defer,
}

pub(super) type R<T> = Result<T, Stop>;

const SKIP_TOKENS: [&str; 4] = ["t", "year", "hour", "minute"];
const MODIFIERS: [&str; 3] = ["last", "this", "next"];

/// `^([0-9]|0[0-9]|1[0-9]|2[0-3]):[0-5][0-9]$`
fn hour_minute(s: &str) -> bool {
    let Some((h, m)) = s.split_once(':') else { return false };
    let m_ok =
        m.len() == 2 && m.as_bytes()[0].is_ascii_digit() && m.as_bytes()[0] <= b'5' && m.as_bytes()[1].is_ascii_digit();
    let h_ok = h.bytes().all(|b| b.is_ascii_digit())
        && (h.len() == 1
            || (h.len() == 2 && (h.as_bytes()[0] <= b'1' || (h.as_bytes()[0] == b'2' && h.as_bytes()[1] <= b'3'))));
    m_ok && h_ok
}

/// MICROSECOND `\d{1,6}` search.
fn microsecond(s: &str) -> Option<String> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    Some(s[start..].chars().take_while(char::is_ascii_digit).take(6).collect())
}

/// MERIDIAN `am|pm` search.
fn meridian(s: &str) -> Option<&'static str> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(1)).find_map(|i| match &b[i..i + 2] {
        b"am" => Some("am"),
        b"pm" => Some("pm"),
        _ => None,
    })
}

/// `_parser.__init__` up to the attributes `_results` reads.
fn read(datestring: &str, date_order: &str) -> R<Fields> {
    let tokens: Vec<(String, u8)> =
        tokenize(datestring)?.into_iter().map(|(t, k)| (py_strip(&t).to_owned(), k)).collect();
    let mut p = Fields::new(tokens, date_order);
    // a meridian written before a time goes after it
    for i in 0..p.filtered.len().saturating_sub(1) {
        let preceding_colon = i > 0 && p.filtered[i - 1].0.contains(':');
        if (p.filtered[i].0 == "am" || p.filtered[i].0 == "pm") && p.filtered[i + 1].0.contains(':') && !preceding_colon
        {
            p.filtered.swap(i, i + 1);
        }
    }
    let mut skip_index: Vec<usize> = Vec::new();
    let mut skip_component_year = false;
    let mut nothing_to_parse = true;
    for index in 0..p.filtered.len() {
        if skip_index.contains(&index) {
            continue;
        }
        let (mut token, kind, original) = p.filtered[index].clone();
        if SKIP_TOKENS.contains(&token.as_str()) {
            continue;
        }
        nothing_to_parse = false;
        if MODIFIERS.contains(&token.as_str()) {
            if p.weekday_modifier.is_some() {
                return Err(Stop::Fail);
            }
            p.weekday_modifier = Some(token);
            continue;
        }
        if p.time.is_none() {
            let mut meridian_index = index + 1;
            let tok = |i: usize| p.tokens.get(i).map(|t| t.0.as_str());
            let before = tok(original + 1).map(|t| t == ".");
            let after = original != 0 && tok(original - 1) == Some(".");
            if before == Some(true) && !after {
                if let Some(next) = p.filtered.get(index + 1) {
                    let last = index + 1 == p.filtered.len() - 1;
                    if last || tok(next.2 + 1).is_some_and(|t| t != ".") {
                        let joined = format!("{token}:{}", next.0);
                        if hour_minute(&joined) {
                            token = joined;
                            skip_index.push(index + 1);
                            meridian_index += 1;
                        }
                    }
                }
            }
            let micro = (|| {
                let m = microsecond(&p.filtered.get(index + 1)?.0)?;
                token.contains(':').then_some(())?;
                let at = p.tokens.iter().position(|t| t.0 == token && t.1 == 0)?;
                p.tokens.get(at + 1)?.0.contains('.').then_some(m)
            })();
            if micro.is_some() {
                meridian_index += 1;
            }
            let mer = p.filtered.get(meridian_index).and_then(|t| meridian(&t.0));
            if token.contains(':') || mer.is_some() || micro.is_some() {
                p.time = Some(match (mer, &micro) {
                    (Some(m), None) => {
                        skip_index.push(meridian_index);
                        format!("{token} {m}")
                    }
                    (None, Some(us)) => {
                        skip_index.push(index + 1);
                        format!("{token}.{us}")
                    }
                    (Some(m), Some(us)) => {
                        skip_index.extend([index + 1, meridian_index]);
                        format!("{token}.{us} {m}")
                    }
                    (None, None) => token,
                });
                continue;
            }
        }
        let year_set = if kind == 0 { p.parse_number(&token, skip_component_year)? } else { p.parse_alpha(&token)? };
        if token.chars().count() == 4 && year_set {
            skip_component_year = true;
        }
    }
    if nothing_to_parse {
        return Err(Stop::Fail);
    }
    p.assign_unset()?;
    Ok(p)
}

/// Ordinal of the local date, for the two-digit-year correction against `now`.
pub(super) type Today = i64;

/// `_parser._parse_with_parts(datestring, settings, tz, date_order)`: the naive datetime.
pub(super) fn parse(datestring: &str, date_order: &str, today: Today) -> R<DateTime> {
    let p = read(datestring, date_order)?;
    let (Some(year), Some(month), Some(day)) =
        (p.year.filter(|&v| v != 0), p.month.filter(|&v| v != 0), p.day.filter(|&v| v != 0))
    else {
        return Err(Stop::Fail);
    };
    let (hour, minute, second, micro) = match &p.time {
        Some(t) => time(t).ok_or(Stop::Fail)?,
        None => (0, 0, 0, 0),
    };
    let mut date = DateTime::new(
        year.into(),
        month.into(),
        day.into(),
        hour.into(),
        minute.into(),
        second.into(),
        micro.into(),
        None,
    )
    .map_err(|_| Stop::Fail)?;
    if p.weekday_modifier.is_some() && !(p.weekday && p.token_day.is_some()) {
        return Err(Stop::Fail);
    }
    if matches!(&p.token_year, Some(Tok::Pair(t, _)) if t.chars().count() == 2) {
        // datetime.now(tz) vs the date: decided only away from today, whatever tz is
        let day = ymd_to_ord(date.year.into(), date.month.into(), date.day.into());
        if (day - today).abs() <= 2 {
            return Err(Stop::Defer);
        }
        if day > today {
            date = DateTime::new(
                i128::from(date.year) - 100,
                date.month.into(),
                date.day.into(),
                date.hour.into(),
                date.minute.into(),
                date.second.into(),
                date.micro.into(),
                None,
            )
            .map_err(|_| Stop::Fail)?;
        }
    }
    Ok(date)
}
