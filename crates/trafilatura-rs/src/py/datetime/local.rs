//! Local-time conversions of the C datetime module: `timestamp()` of naive datetimes (`local_to_seconds`),
//! `fromtimestamp(t, tz=timezone.utc)` and the end of today's local date.
use super::{ord_to_ymd, ymd_to_ord, DateTime};

/// `date(1970, 1, 1).toordinal()` in seconds.
const EPOCH_SECONDS: i64 = 719_163 * 86_400;
const MAX_FOLD_SECONDS: i64 = 24 * 3600;

/// `_PyTime_localtime`; None where Python raises OSError (or OverflowError for a too-wide time_t).
#[allow(clippy::useless_conversion)]
fn localtime(t: i64) -> Option<libc::tm> {
    let t = libc::time_t::try_from(t).ok()?;
    // SAFETY: tm is plain old data; localtime_r/_s only write into it.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        #[cfg(not(windows))]
        let ok = !libc::localtime_r(&t, &mut tm).is_null();
        #[cfg(windows)]
        let ok = libc::localtime_s(&mut tm, &t) == 0;
        ok.then_some(tm)
    }
}

fn utc_to_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> Option<i64> {
    (1..=9999).contains(&year).then(|| ((ymd_to_ord(year, month, day) * 24 + hour) * 60 + minute) * 60 + second)
}

/// `local(u)`: seconds since 0001-01-01 of the local wall clock at `u` (same scale).
fn local(u: i64) -> Option<i64> {
    let tm = localtime(u - EPOCH_SECONDS)?;
    utc_to_seconds(
        i64::from(tm.tm_year) + 1900,
        i64::from(tm.tm_mon) + 1,
        i64::from(tm.tm_mday),
        i64::from(tm.tm_hour),
        i64::from(tm.tm_min),
        i64::from(tm.tm_sec),
    )
}

/// `local_to_seconds` with fold=0.
fn local_to_seconds(t: i64) -> Option<i64> {
    let a = local(t)? - t;
    let u1 = t - a;
    let t1 = local(u1)?;
    let b;
    if t1 == t {
        let u2 = u1 - MAX_FOLD_SECONDS;
        b = local(u2)? - u2;
        if a == b {
            return Some(u1);
        }
    } else {
        b = t1 - u1;
    }
    let u2 = t - b;
    if local(u2)? == t {
        return Some(u2);
    }
    if t1 == t {
        return Some(u1);
    }
    Some(u1.max(u2))
}

impl DateTime {
    /// `timestamp()` in microseconds: aware via the offset, naive as local time. None where Python raises
    /// (OSError from the platform localtime).
    pub fn timestamp_micros(&self) -> Option<i64> {
        let wall = utc_to_seconds(
            self.year.into(),
            self.month.into(),
            self.day.into(),
            self.hour.into(),
            self.minute.into(),
            self.second.into(),
        )?;
        let seconds = match self.offset {
            Some(off) => wall - i64::from(off),
            None => local_to_seconds(wall)?,
        };
        Some((seconds - EPOCH_SECONDS) * 1_000_000 + i64::from(self.micro))
    }

    /// `datetime.fromtimestamp(t, tz=timezone.utc)`; None where Python raises OverflowError, OSError or
    /// ValueError (outside time_t, gmtime failure, year outside 1..9999).
    pub fn fromtimestamp_utc(t: i128) -> Option<DateTime> {
        let t = i64::try_from(t).ok()?;
        if cfg!(windows) && t < 0 {
            return None;
        }
        let (y, m, d) = ord_to_ymd(t.div_euclid(86_400) + 719_163);
        let s = t.rem_euclid(86_400);
        let dt = DateTime::new(
            y.into(),
            m.into(),
            d.into(),
            (s / 3600).into(),
            (s % 3600 / 60).into(),
            (s % 60).into(),
            0,
            Some(0),
        );
        dt.ok()
    }
}

/// Local wall clock (y, m, d, H, M, S) at a POSIX time.
#[cfg(test)]
pub(super) fn tests_localtime(t: i64) -> (i32, i32, i32, i32, i32, i32) {
    let tm = localtime(t).expect("localtime");
    (tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// `datetime.now().replace(hour=23, minute=59, second=59, microsecond=999999)` (get_max_date's default).
pub fn today_end() -> DateTime {
    // SAFETY: time() with a null pointer only returns the current time.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    #[allow(clippy::useless_conversion)]
    let tm = localtime(i64::from(now)).expect("localtime of the current time");
    DateTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: 23,
        minute: 59,
        second: 59,
        micro: 999_999,
        offset: None,
    }
}
