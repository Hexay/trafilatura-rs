//! Expected values from python-dateutil 4.0.0 / dateparser 1.5.0; harness/diff_dates.py checks the whole corpus.
use super::dateutil::{parse, Default};
use super::{external_date_is_none, has_date_cue};
use crate::py::datetime::DateTime;

fn dt(year: i32, month: u32, day: u32, hms: (u32, u32, u32), micro: u32, offset: Option<i32>) -> DateTime {
    let (hour, minute, second) = hms;
    DateTime { year, month, day, hour, minute, second, micro, offset }
}

fn parse_pinned(s: &str) -> Option<Option<DateTime>> {
    parse(s, &Default::Fixed(dt(2026, 6, 30, (0, 0, 0), 0, None)), &[])
}

#[test]
fn dateutil_parses() {
    assert_eq!(parse_pinned("2021-03-04T10:11:12+02:00"), Some(Some(dt(2021, 3, 4, (10, 11, 12), 0, Some(7200)))));
    assert_eq!(parse_pinned("2019 03 05 10:00 PM"), Some(Some(dt(2019, 3, 5, (22, 0, 0), 0, None))));
    assert_eq!(parse_pinned("2014-07-03 01:30 UTC"), Some(Some(dt(2014, 7, 3, (1, 30, 0), 0, Some(0)))));
    assert_eq!(parse_pinned("2021 Mar"), Some(Some(dt(2021, 3, 30, (0, 0, 0), 0, None))));
    let micro = dt(2020, 10, 1, (15, 43, 13), 123_456, None);
    assert_eq!(parse_pinned("2020.10.01 15:43:13.123456"), Some(Some(micro)));
}

#[test]
fn dateutil_rejects() {
    for s in ["2020/13/45", "2021-02-29", "2018-12-01 GMT+3"] {
        assert_eq!(parse_pinned(s), Some(None), "{s}");
    }
}

#[test]
fn dateutil_defers() {
    // two-digit year, weekday only, a name dateutil warns about
    for s in ["99", "Monday", "2017-10-12T08:00:00 EST"] {
        assert_eq!(parse_pinned(s), None, "{s}");
    }
    assert_eq!(parse("2014-07-03 01:30 CET", &Default::TodayMidnight, &["CET".into(), "CEST".into()]), None);
}

#[test]
fn external_date() {
    assert_eq!(has_date_cue("Posted by admin in news", 1995, 2026), Some(false));
    assert_eq!(has_date_cue("Mar 2021", 1995, 2026), Some(true));
    assert!(external_date_is_none("12 xyzzy 2021", 1995, 2026, false));
    assert!(!external_date_is_none("Mar 2021", 1995, 2026, true));
}

#[test]
fn native_dateparser() {
    let today = crate::py::datetime::ymd_to_ord(2026, 6, 30);
    let date = |s: &str| super::external_date(s, today).expect("decided");
    let midnight = |y, m, d| Some(dt(y, m, d, (0, 0, 0), 0, None));
    assert_eq!(date("19 febrero 2024"), midnight(2024, 2, 19));
    assert_eq!(date("20 дек 2022, 18:08"), Some(dt(2022, 12, 20, (18, 8, 0), 0, None)));
    assert_eq!(date("آوریل 11, 2017"), midnight(2017, 4, 11));
    assert_eq!(date("Říjen 22, 2016, 08:02:27 odpoledne »"), Some(dt(2016, 10, 22, (20, 2, 27), 0, None)));
    assert_eq!(date("martes 18-Abr-23"), midnight(2023, 4, 18));
    assert_eq!(date("15 - 23 February"), midnight(2023, 2, 15));
    assert_eq!(date("This list was generated on Tue Feb 20 01:44:32 2024"), None);
    assert_eq!(date("»"), None);
    // a two-digit year read as the future goes back a century, and near today it depends on the time zone
    assert_eq!(date("1 Jan 27"), midnight(1927, 1, 1));
    assert_eq!(super::external_date("1 Jul 26", today), None);
}

#[test]
fn external_date_unicode() {
    for s in ["お問い合わせ ｜ プライバシーポリシー © 2015 1", "Αρχική σελίδα 2019", "Главная страница 2018"]
    {
        assert!(external_date_is_none(s, 1995, 2026, true), "{s}");
        assert!(!external_date_is_none(s, 1995, 2026, false), "{s}");
    }
    for s in ["2015年3月4日", "4 марта 2018", "12 Μαρτίου 2019", "2019년 3월 4일", "mars 2021 utc+01:00 xyzzy"]
    {
        assert!(!external_date_is_none(s, 1995, 2026, true), "{s}");
    }
}
