//! `datetime.fromisoformat` (`datetime_fromisoformat` in CPython 3.13's C module). It works on the UTF-8 bytes
//! of the string and reads NUL past the end, which makes it accept more than the documented grammar
//! (any separator character, a fraction without a decimal mark after `HHMMSS`, ...).
use super::{ord_to_ymd, ymd_to_ord, DateTime};
use crate::dom::{Error, Result};

/// The string's bytes, NUL-terminated as the C parser sees them.
struct Buf<'a>(&'a [u8]);

impl Buf<'_> {
    fn at(&self, i: usize) -> u8 {
        self.0.get(i).copied().unwrap_or(0)
    }

    /// `parse_digits`: accumulates onto `var`.
    fn digits(&self, p: &mut usize, var: &mut i64, n: usize) -> bool {
        for _ in 0..n {
            let d = self.at(*p).wrapping_sub(b'0');
            *p += 1;
            if d > 9 {
                return false;
            }
            *var = *var * 10 + i64::from(d);
        }
        true
    }
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

/// `_find_isoformat_datetime_separator`; None for its -1.
fn find_separator(b: &Buf) -> Option<usize> {
    let len = b.0.len();
    if len == 7 {
        return Some(7);
    }
    if b.at(4) == b'-' {
        if b.at(5) == b'W' {
            if len < 8 {
                return None;
            }
            if len > 8 && b.at(8) == b'-' {
                if len == 9 {
                    return None;
                }
                if len > 10 && is_digit(b.at(10)) {
                    return Some(8);
                }
                return Some(10);
            }
            return Some(8);
        }
        return Some(10);
    }
    if b.at(4) == b'W' {
        let mut idx = 7;
        while idx < len && is_digit(b.at(idx)) {
            idx += 1;
        }
        if idx < 9 {
            return Some(idx);
        }
        return Some(if idx % 2 == 0 { 7 } else { 8 });
    }
    Some(8)
}

/// `iso_to_ymd`.
fn iso_to_ymd(year: i64, week: i64, day: i64) -> Option<(i64, i64, i64)> {
    if !(1..=9999).contains(&year) {
        return None;
    }
    if week <= 0 || week >= 53 {
        let first_weekday = (ymd_to_ord(year, 1, 1) + 6) % 7;
        if !(week == 53 && (first_weekday == 3 || (first_weekday == 2 && super::is_leap(year)))) {
            return None;
        }
    }
    if day <= 0 || day >= 8 {
        return None;
    }
    let first_day = ymd_to_ord(year, 1, 1);
    let first_weekday = (first_day + 6) % 7;
    let mut week1_monday = first_day - first_weekday;
    if first_weekday > 3 {
        week1_monday += 7;
    }
    Some(ord_to_ymd(week1_monday + (week - 1) * 7 + day - 1))
}

/// `parse_isoformat_date`.
fn parse_date(b: &Buf, len: usize) -> Option<(i64, i64, i64)> {
    let mut p = 0;
    let (mut year, mut month, mut day) = (0, 0, 0);
    if !b.digits(&mut p, &mut year, 4) {
        return None;
    }
    let uses_separator = b.at(p) == b'-';
    p += usize::from(uses_separator);
    if b.at(p) == b'W' {
        p += 1;
        let (mut week, mut iso_day) = (0, 0);
        if !b.digits(&mut p, &mut week, 2) {
            return None;
        }
        if p < len {
            if uses_separator {
                let c = b.at(p);
                p += 1;
                if c != b'-' {
                    return None;
                }
            }
            if !b.digits(&mut p, &mut iso_day, 1) {
                return None;
            }
        } else {
            iso_day = 1;
        }
        return iso_to_ymd(year, week, iso_day);
    }
    if !b.digits(&mut p, &mut month, 2) {
        return None;
    }
    if uses_separator {
        let c = b.at(p);
        p += 1;
        if c != b'-' {
            return None;
        }
    }
    b.digits(&mut p, &mut day, 2).then_some((year, month, day))
}

/// `parse_hh_mm_ss_ff` over `[start, end)`: (return code, [hour, minute, second, microsecond]).
fn parse_hh_mm_ss_ff(b: &Buf, start: usize, end: usize) -> (i32, [i64; 4]) {
    let mut vals = [0i64; 4];
    let mut p = start;
    let mut has_separator = true;
    for (i, val) in vals.iter_mut().take(3).enumerate() {
        if !b.digits(&mut p, val, 2) {
            return (-3, vals);
        }
        let c = b.at(p);
        p += 1;
        if i == 0 {
            has_separator = c == b':';
        }
        if c == b'.' || c == b',' {
            if p >= end {
                return (-3, vals);
            }
            break;
        } else if p >= end {
            return (i32::from(c != 0), vals);
        } else if has_separator && c == b':' {
            continue;
        } else if !has_separator {
            p -= 1;
        } else {
            return (-4, vals);
        }
    }
    let to_parse = (end - p).min(6);
    let mut micro = 0;
    if !b.digits(&mut p, &mut micro, to_parse) {
        return (-3, vals);
    }
    if (1..6).contains(&to_parse) {
        micro *= [100_000, 10_000, 1000, 100, 10][to_parse - 1];
    }
    vals[3] = micro;
    while is_digit(b.at(p)) {
        p += 1;
    }
    (i32::from(b.at(p) != 0), vals)
}

/// `parse_isoformat_time`: (return code, [h, m, s, us], tz offset seconds, tz microseconds).
fn parse_time(b: &Buf) -> (i32, [i64; 4], i64, i64) {
    let p_end = b.0.len();
    let mut tz = 0;
    loop {
        if matches!(b.at(tz), b'Z' | b'+' | b'-') {
            break;
        }
        tz += 1;
        if tz >= p_end {
            break;
        }
    }
    let (rv, time) = parse_hh_mm_ss_ff(b, 0, tz);
    if rv < 0 {
        return (rv, time, 0, 0);
    }
    if tz == p_end {
        return (if rv == 1 { -5 } else { 0 }, time, 0, 0);
    }
    if b.at(tz) == b'Z' {
        return (if b.at(tz + 1) != 0 { -5 } else { 1 }, time, 0, 0);
    }
    let sign = if b.at(tz) == b'-' { -1 } else { 1 };
    let (rv, [h, m, s, us]) = parse_hh_mm_ss_ff(b, tz + 1, p_end);
    (if rv != 0 { -5 } else { 1 }, time, sign * (h * 3600 + m * 60 + s), sign * us)
}

impl DateTime {
    /// `datetime.fromisoformat(s)`. A sub-second UTC offset is `Unsupported` (`DateTime` keeps whole seconds).
    pub fn fromisoformat(s: &str) -> Result<DateTime> {
        let invalid = || Error::Value(format!("Invalid isoformat string: {s:?}"));
        if s.chars().count() < 7 {
            return Err(invalid());
        }
        let b = Buf(s.as_bytes());
        let len = s.len();
        let sep = find_separator(&b).ok_or_else(invalid)?;
        let (year, month, day) = parse_date(&b, sep).ok_or_else(invalid)?;
        let (mut rv, mut time, mut tzoffset, mut tzusec) = (0, [0; 4], 0, 0);
        if len > sep {
            let lead = b.at(sep);
            let width = if lead & 0x80 == 0 {
                1
            } else {
                match lead & 0xf0 {
                    0xe0 => 3,
                    0xf0 => 4,
                    _ => 2,
                }
            };
            (rv, time, tzoffset, tzusec) = parse_time(&Buf(s.as_bytes().get(sep + width..).unwrap_or_default()));
        }
        if rv < 0 {
            return Err(invalid());
        }
        let offset = if rv == 1 {
            let total = tzoffset * 1_000_000 + tzusec;
            if total.abs() >= 86_400_000_000 {
                return Err(Error::Value("offset must be a timedelta strictly between -24h and 24h".into()));
            }
            Some(total)
        } else {
            None
        };
        let [hour, minute, second, micro] = time.map(i128::from);
        let dt = DateTime::new(year.into(), month.into(), day.into(), hour, minute, second, micro, None)?;
        match offset {
            Some(us) if us % 1_000_000 != 0 => Err(Error::Unsupported("sub-second UTC offset".into())),
            Some(us) => Ok(DateTime { offset: Some((us / 1_000_000) as i32), ..dt }),
            None => Ok(dt),
        }
    }
}
