//! Python `datetime.datetime` semantics htmldate depends on, as CPython 3.13's C module implements them
//! (Modules/_datetimemodule.c): constructor checks, `fromisoformat`, `strptime("%Y-%m-%d")`, `strftime`,
//! `timestamp()` (naive = local time) and `fromtimestamp(t, tz=timezone.utc)`.
use crate::dom::{Error, Result};

mod iso;
mod local;
mod strptime;
#[cfg(test)]
mod tests;

pub use local::today_end;

/// A `datetime.datetime`: naive (`offset` None) or with a fixed UTC offset in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub micro: u32,
    pub offset: Option<i32>,
}

const DAYS_IN_MONTH: [i64; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const DAYS_BEFORE_MONTH: [i64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

pub(crate) fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    if month == 2 && is_leap(year) {
        29
    } else {
        DAYS_IN_MONTH[month as usize]
    }
}

/// `ymd_to_ord`: proleptic Gregorian ordinal, 0001-01-01 is day 1.
pub(crate) fn ymd_to_ord(year: i64, month: i64, day: i64) -> i64 {
    let y = year - 1;
    let before_month = DAYS_BEFORE_MONTH[month as usize] + i64::from(month > 2 && is_leap(year));
    y * 365 + y / 4 - y / 100 + y / 400 + before_month + day
}

/// `ord_to_ymd`.
pub(crate) fn ord_to_ymd(ordinal: i64) -> (i64, i64, i64) {
    let mut n = ordinal - 1;
    let n400 = n.div_euclid(146_097);
    n = n.rem_euclid(146_097);
    let n100 = n / 36_524;
    n %= 36_524;
    let n4 = n / 1461;
    n %= 1461;
    let n1 = n / 365;
    n %= 365;
    let year = n400 * 400 + 1 + n100 * 100 + n4 * 4 + n1;
    if n1 == 4 || n100 == 4 {
        return (year - 1, 12, 31);
    }
    let leap = n1 == 3 && (n4 != 24 || n100 == 3);
    let mut month = (n + 50) >> 5;
    let mut preceding = DAYS_BEFORE_MONTH[month as usize] + i64::from(month > 2 && leap);
    if preceding > n {
        month -= 1;
        preceding -= DAYS_IN_MONTH[month as usize] + i64::from(month == 2 && leap);
    }
    (year, month, n - preceding + 1)
}

fn value_error<T>(msg: &str) -> Result<T> {
    Err(Error::Value(msg.to_owned()))
}

/// C-int argument conversion of the constructor (OverflowError beyond `int`).
fn c_int(v: i128) -> Result<i64> {
    i32::try_from(v)
        .map(i64::from)
        .map_err(|_| Error::Runtime("OverflowError: signed integer is greater than maximum".into()))
}

impl DateTime {
    /// `datetime(year, month, day, hour, minute, second, microsecond)` with its range checks (ValueError).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        year: i128,
        month: i128,
        day: i128,
        hour: i128,
        minute: i128,
        second: i128,
        micro: i128,
        offset: Option<i32>,
    ) -> Result<DateTime> {
        let [year, month, day, hour, minute, second, micro] =
            [c_int(year)?, c_int(month)?, c_int(day)?, c_int(hour)?, c_int(minute)?, c_int(second)?, c_int(micro)?];
        if !(1..=9999).contains(&year) {
            return value_error(&format!("year {year} is out of range"));
        }
        if !(1..=12).contains(&month) {
            return value_error("month must be in 1..12");
        }
        if day < 1 || day > days_in_month(year, month) {
            return value_error("day is out of range for month");
        }
        if !(0..=23).contains(&hour) {
            return value_error("hour must be in 0..23");
        }
        if !(0..=59).contains(&minute) {
            return value_error("minute must be in 0..59");
        }
        if !(0..=59).contains(&second) {
            return value_error("second must be in 0..59");
        }
        if !(0..=999_999).contains(&micro) {
            return value_error("microsecond must be in 0..999999");
        }
        Ok(DateTime {
            year: year as i32,
            month: month as u32,
            day: day as u32,
            hour: hour as u32,
            minute: minute as u32,
            second: second as u32,
            micro: micro as u32,
            offset,
        })
    }

    /// `datetime(year, month, day)`.
    pub fn ymd(year: i128, month: i128, day: i128) -> Result<DateTime> {
        DateTime::new(year, month, day, 0, 0, 0, 0, None)
    }

    /// `self.replace(tzinfo=None)` as an ordering key (wall-clock comparison).
    pub fn naive_key(&self) -> (i32, u32, u32, u32, u32, u32, u32) {
        (self.year, self.month, self.day, self.hour, self.minute, self.second, self.micro)
    }

    /// `strftime` for the directives htmldate's formats use: `%Y %m %d %H %M %S %f %z %%` (glibc: `%Y` unpadded).
    pub fn strftime(&self, fmt: &str) -> String {
        let mut out = String::new();
        let mut chars = fmt.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('Y') => out.push_str(&self.year.to_string()),
                Some('m') => out.push_str(&format!("{:02}", self.month)),
                Some('d') => out.push_str(&format!("{:02}", self.day)),
                Some('H') => out.push_str(&format!("{:02}", self.hour)),
                Some('M') => out.push_str(&format!("{:02}", self.minute)),
                Some('S') => out.push_str(&format!("{:02}", self.second)),
                Some('f') => out.push_str(&format!("{:06}", self.micro)),
                Some('z') => {
                    if let Some(off) = self.offset {
                        let sign = if off < 0 { '-' } else { '+' };
                        let a = off.unsigned_abs();
                        out.push_str(&format!("{sign}{:02}{:02}", a / 3600, a % 3600 / 60));
                        if a % 60 != 0 {
                            out.push_str(&format!("{:02}", a % 60));
                        }
                    }
                }
                Some('%') => out.push('%'),
                other => panic!("strftime directive %{other:?} not ported"),
            }
        }
        out
    }
}
