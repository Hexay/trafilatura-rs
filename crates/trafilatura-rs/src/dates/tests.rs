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
    assert!(external_date_is_none("12 xyzzy 2021", 1995, 2026));
    assert!(!external_date_is_none("Mar 2021", 1995, 2026));
}
