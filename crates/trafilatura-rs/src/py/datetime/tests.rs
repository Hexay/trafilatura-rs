//! Expected values printed by CPython 3.13.17.
use super::DateTime;

type Fields = (i32, u32, u32, u32, u32, u32, u32, Option<i32>);

fn fields(d: DateTime) -> Fields {
    (d.year, d.month, d.day, d.hour, d.minute, d.second, d.micro, d.offset)
}

const ISO: &[(&str, Option<Fields>)] = &[
    ("2019-01-01T", None),
    ("2019-01-01T120000123", Some((2019, 1, 1, 12, 0, 0, 123000, None))),
    ("2019-01-01T12:00Z-05", None),
    ("2019-01-01é", None),
    ("2019-01-01é12", Some((2019, 1, 1, 12, 0, 0, 0, None))),
    ("2019-01-01T24:00", None),
    ("2019", None),
    ("20190101", Some((2019, 1, 1, 0, 0, 0, 0, None))),
    ("2019-W01", Some((2018, 12, 31, 0, 0, 0, 0, None))),
    ("2019W011", Some((2018, 12, 31, 0, 0, 0, 0, None))),
    ("2019-W01-3", Some((2019, 1, 2, 0, 0, 0, 0, None))),
    ("2020-W53", Some((2020, 12, 28, 0, 0, 0, 0, None))),
    ("2019-W53", None),
    ("2019-01-01T12:00:00.", None),
    ("2019-01-01T12+0530", Some((2019, 1, 1, 12, 0, 0, 0, Some(19800)))),
    ("2019-01-01T12:00:00+0000", Some((2019, 1, 1, 12, 0, 0, 0, Some(0)))),
    ("２０１９-01-01", None),
    ("2019-01-01T12\0", Some((2019, 1, 1, 12, 0, 0, 0, None))),
    ("2019-01-01T12:00+05:00Z", None),
    ("2019-01-01T12.5", Some((2019, 1, 1, 12, 0, 0, 500000, None))),
    ("2019-01-01T1230.5", Some((2019, 1, 1, 12, 30, 0, 500000, None))),
    ("2019-01-01 12:00:00-03:00", Some((2019, 1, 1, 12, 0, 0, 0, Some(-10800)))),
    ("2019-01-01T12:00:00.1234567Z", Some((2019, 1, 1, 12, 0, 0, 123456, Some(0)))),
    ("2019-02-29", None),
    ("2020-02-29", Some((2020, 2, 29, 0, 0, 0, 0, None))),
    ("2019-01-01T12:00:00+24:00", None),
    ("0000-01-01", None),
    ("2019-1-01", None),
    ("2019-01-01T12:00:00:5", Some((2019, 1, 1, 12, 0, 0, 500000, None))),
    ("2019-01-01T12:00:00 ", None),
    ("20190101T1200", Some((2019, 1, 1, 12, 0, 0, 0, None))),
    ("2019-01-01T12:00:00+05:30:15", Some((2019, 1, 1, 12, 0, 0, 0, Some(19815)))),
];

const STRPTIME: &[(&str, Option<(i32, u32, u32)>)] = &[
    ("2020-1-15", Some((2020, 1, 15))),
    ("2020-01-05", Some((2020, 1, 5))),
    ("2020-1-5", Some((2020, 1, 5))),
    ("2020-10-5x", None),
    ("2020-11-31", None),
    ("2020-1- 5", Some((2020, 1, 5))),
    ("２０２０-1-1", Some((2020, 1, 1))),
    ("2020-1-1٣", Some((2020, 1, 13))),
    ("2020-13-1", None),
    ("2020-01-001", None),
    ("2020--1-1", None),
    ("202-01-01", None),
];

#[test]
fn fromisoformat_matches_cpython() {
    for &(s, want) in ISO {
        assert_eq!(DateTime::fromisoformat(s).ok().map(fields), want, "{s:?}");
    }
    assert!(DateTime::fromisoformat("2019-01-01 12:00:00+05:00:00.5").is_err());
}

#[test]
fn strptime_matches_cpython() {
    for &(s, want) in STRPTIME {
        assert_eq!(DateTime::strptime_ymd(s).ok().map(|d| (d.year, d.month, d.day)), want, "{s:?}");
    }
}

#[test]
fn strftime_formats() {
    let d = DateTime::fromisoformat("2019-03-04T05:06:07.000008-05:30").unwrap();
    assert_eq!(d.strftime("%Y-%m-%dT%H:%M:%S.%f%z"), "2019-03-04T05:06:07.000008-0530");
    assert_eq!(d.strftime("%Y-%m-%d"), "2019-03-04");
    let naive = DateTime::ymd(2020, 12, 1).unwrap();
    assert_eq!(naive.strftime("%Y-%m-%dT%H:%M:%S.%f%z"), "2020-12-01T00:00:00.000000");
    let seconds = DateTime { offset: Some(-19815), ..naive };
    assert_eq!(seconds.strftime("%z"), "-053015");
}

#[test]
fn timestamps() {
    let d = DateTime::fromisoformat("2019-03-04T05:06:07.000008-05:30").unwrap();
    assert_eq!(d.timestamp_micros(), Some(1_551_695_767_000_008));
    // naive datetimes are local time: the wall clock of a local instant maps back to it
    let local = DateTime::fromtimestamp_utc(1_600_000_000).unwrap();
    assert_eq!(fields(local), (2020, 9, 13, 12, 26, 40, 0, Some(0)));
    let tm = super::local::tests_localtime(1_600_000_000);
    let naive = DateTime::new(tm.0.into(), tm.1.into(), tm.2.into(), tm.3.into(), tm.4.into(), tm.5.into(), 7, None);
    assert_eq!(naive.unwrap().timestamp_micros(), Some(1_600_000_000_000_007));
    for (t, ok) in [(0, true), (-62_135_596_800, true), (-62_135_596_801, false), (253_402_300_799, true)] {
        assert_eq!(DateTime::fromtimestamp_utc(t).is_some(), ok, "{t}");
    }
    assert!(DateTime::fromtimestamp_utc(253_402_300_800).is_none());
    assert!(DateTime::fromtimestamp_utc(i128::MAX).is_none());
}

#[test]
fn constructor_checks() {
    assert!(DateTime::ymd(2019, 2, 29).is_err());
    assert!(DateTime::ymd(2000, 2, 29).is_ok());
    assert!(DateTime::ymd(1900, 2, 29).is_err());
    assert!(DateTime::ymd(0, 1, 1).is_err());
    assert!(DateTime::ymd(2019, 13, 1).is_err());
}
