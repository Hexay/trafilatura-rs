//! `datetime.strptime(s, "%Y-%m-%d")` (Lib/_strptime.py): the format compiles to
//! `(?P<Y>\d\d\d\d)-(?P<m>1[0-2]|0[1-9]|[1-9])-(?P<d>3[0-1]|[1-2]\d|0[1-9]|[1-9]| [1-9])`, matched at the start with
//! nothing left over; `\d` is any Unicode decimal digit, which `int()` accepts.
use super::DateTime;
use crate::dom::{Error, Result};
use crate::py::int::{decimal_value, py_int};

fn is_decimal(c: Option<&char>) -> bool {
    c.is_some_and(|&c| decimal_value(c).is_some())
}

fn in_range(c: Option<&char>, lo: char, hi: char) -> bool {
    c.is_some_and(|c| (lo..=hi).contains(c))
}

/// Length in chars of the first alternative of `%m` matching at `s[i..]`.
fn month_len(s: &[char], i: usize) -> Option<usize> {
    let (a, b) = (s.get(i), s.get(i + 1));
    if a == Some(&'1') && in_range(b, '0', '2') {
        Some(2)
    } else if a == Some(&'0') && in_range(b, '1', '9') {
        Some(2)
    } else {
        in_range(a, '1', '9').then_some(1)
    }
}

/// Length in chars of the first alternative of `%d` matching at `s[i..]`.
fn day_len(s: &[char], i: usize) -> Option<usize> {
    let (a, b) = (s.get(i), s.get(i + 1));
    if a == Some(&'3') && in_range(b, '0', '1') {
        Some(2)
    } else if in_range(a, '1', '2') && is_decimal(b) {
        Some(2)
    } else if a == Some(&'0') && in_range(b, '1', '9') {
        Some(2)
    } else if in_range(a, '1', '9') {
        Some(1)
    } else if a == Some(&' ') && in_range(b, '1', '9') {
        Some(2)
    } else {
        None
    }
}

impl DateTime {
    /// `datetime.strptime(s, "%Y-%m-%d")`.
    pub fn strptime_ymd(s: &str) -> Result<DateTime> {
        let no_match = || Error::Value(format!("time data {s:?} does not match format '%Y-%m-%d'"));
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 4 || !chars[..4].iter().all(|c| decimal_value(*c).is_some()) || chars.get(4) != Some(&'-') {
            return Err(no_match());
        }
        let m = month_len(&chars, 5).ok_or_else(no_match)?;
        if chars.get(5 + m) != Some(&'-') {
            return Err(no_match());
        }
        let d = day_len(&chars, 6 + m).ok_or_else(no_match)?;
        let end = 6 + m + d;
        if end != chars.len() {
            return Err(Error::Value(format!("unconverted data remains: {}", chars[end..].iter().collect::<String>())));
        }
        let part = |a: usize, b: usize| py_int(&chars[a..b].iter().collect::<String>());
        DateTime::ymd(part(0, 4)?, part(5, 5 + m)?, part(6 + m, end)?)
    }
}
