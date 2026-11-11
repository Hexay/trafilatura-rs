//! `dateutil.parser.parse(s, fuzzy=False)` (python-dateutil 4.0.0, default `parserinfo`) for printable ASCII strings.
//! Inputs whose result depends on state this port does not model defer to Python: two-digit years (converted
//! around the year dateutil was imported in), a weekday without a day (relativedelta), time zone names in
//! `time.tzname` (tzlocal) or without an offset (warning), offsets of a day or more, and a missing date part with an
//! unknown default.
mod numeric;
mod timelex;
mod ymd;

use crate::py::datetime::DateTime;
use ymd::{days_in_month, py_int, Val, Ymd};

/// Why a native parse stopped: dateutil raises a caught exception (`Invalid`, htmldate returns None) or this port
/// cannot tell (`Defer`).
#[derive(Debug)]
pub(super) enum Stop {
    Invalid,
    Defer,
}

pub(super) type R<T> = Result<T, Stop>;

/// The `default` datetime missing fields come from.
pub enum Default {
    /// `datetime.now()` at midnight: the date is not modelled, so a missing date part defers.
    TodayMidnight,
    Fixed(DateTime),
}

#[derive(Default)]
pub(super) struct Res {
    pub year: Option<i64>,
    pub month: Option<i64>,
    pub day: Option<i64>,
    pub weekday: Option<i64>,
    pub hour: Option<i64>,
    pub minute: Option<i64>,
    pub second: Option<i64>,
    pub microsecond: Option<i64>,
    pub tzname: Option<String>,
    pub tzoffset: Option<i64>,
    pub ampm: Option<i64>,
}

const JUMP: [&str; 18] =
    [" ", ".", ",", ";", "-", "/", "'", "at", "on", "and", "ad", "m", "t", "of", "st", "nd", "rd", "th"];
const WEEKDAYS: [&[&str]; 7] = [
    &["mon", "monday"],
    &["tue", "tuesday"],
    &["wed", "wednesday"],
    &["thu", "thursday"],
    &["fri", "friday"],
    &["sat", "saturday"],
    &["sun", "sunday"],
];
const MONTHS: [&[&str]; 12] = [
    &["jan", "january"],
    &["feb", "february"],
    &["mar", "march"],
    &["apr", "april"],
    &["may"],
    &["jun", "june"],
    &["jul", "july"],
    &["aug", "august"],
    &["sep", "sept", "september"],
    &["oct", "october"],
    &["nov", "november"],
    &["dec", "december"],
];
const HMS: [&[&str]; 3] = [&["h", "hour", "hours"], &["m", "minute", "minutes"], &["s", "second", "seconds"]];
const AMPM: [&[&str]; 2] = [&["am", "a"], &["pm", "p"]];
const UTCZONE: [&str; 4] = ["UTC", "GMT", "Z", "z"];

fn lookup(table: &[&[&str]], name: &str) -> Option<i64> {
    let lower = name.to_ascii_lowercase();
    table.iter().position(|names| names.contains(&lower.as_str())).map(|i| i as i64)
}

pub(super) fn jump(name: &str) -> bool {
    JUMP.contains(&name.to_ascii_lowercase().as_str())
}
pub(super) fn weekday(name: &str) -> Option<i64> {
    lookup(&WEEKDAYS, name)
}
pub(super) fn month(name: &str) -> Option<i64> {
    lookup(&MONTHS, name).map(|m| m + 1)
}
pub(super) fn hms(name: &str) -> Option<i64> {
    lookup(&HMS, name)
}
pub(super) fn ampm(name: &str) -> Option<i64> {
    lookup(&AMPM, name)
}
fn utczone(name: &str) -> bool {
    ["utc", "gmt", "z"].contains(&name.to_ascii_lowercase().as_str())
}

fn could_be_tzname(res: &Res, tzoffset: Option<i64>, token: &str) -> bool {
    res.hour.is_some()
        && res.tzname.is_none()
        && tzoffset.is_none()
        && token.len() <= 5
        && (token.bytes().all(|b| b.is_ascii_uppercase()) || UTCZONE.contains(&token))
}

/// `float(token)` succeeds.
fn is_float(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    ymd::Dec::parse(token).is_ok() || ["nan", "inf", "infinity"].contains(&lower.as_str())
}

/// `parser._parse(timestr)` up to `info.validate`; Err(Invalid) where it returns (None, None).
fn parse_tokens(l: &mut [String]) -> R<(Res, Ymd)> {
    let mut res = Res::default();
    let mut ymd = Ymd::default();
    let n = l.len();
    let tok = |l: &[String], i: usize| -> R<String> { l.get(i).cloned().ok_or(Stop::Invalid) };
    let mut i = 0;
    while i < n {
        if is_float(&l[i]) {
            i = numeric::parse_numeric_token(l, i, &mut ymd, &mut res)?;
        } else if let Some(w) = weekday(&l[i]) {
            res.weekday = Some(w);
        } else if let Some(m) = month(&l[i]) {
            ymd.append(Val::Int(m), Some('M'))?;
            if i + 1 < n {
                if l[i + 1] == "-" || l[i + 1] == "/" {
                    let sep = l[i + 1].clone();
                    ymd.append(Val::Str(&tok(l, i + 2)?), None)?;
                    if i + 3 < n && l[i + 3] == sep {
                        ymd.append(Val::Str(&tok(l, i + 4)?), None)?;
                        i += 2;
                    }
                    i += 2;
                } else if i + 4 < n && l[i + 1] == " " && l[i + 3] == " " && l[i + 2].eq_ignore_ascii_case("of") {
                    if ymd::all_digits(&l[i + 4]) && !l[i + 4].is_empty() {
                        let value = py_int(&l[i + 4])?;
                        if value < 100 {
                            return Err(Stop::Defer);
                        }
                        ymd.append(Val::Str(&value.to_string()), Some('Y'))?;
                    }
                    i += 4;
                }
            }
        } else if let Some(a) = ampm(&l[i]) {
            let hour = res.hour.ok_or(Stop::Invalid)?;
            if !(0..=12).contains(&hour) {
                return Err(Stop::Invalid);
            }
            res.hour = Some(numeric::adjust_ampm(hour, a));
            res.ampm = Some(a);
        } else if could_be_tzname(&res, res.tzoffset, &l[i]) {
            res.tzname = Some(l[i].clone());
            res.tzoffset = (l[i] == "z").then_some(0);
            if i + 1 < n && (l[i + 1] == "+" || l[i + 1] == "-") {
                l[i + 1] = if l[i + 1] == "+" { "-".into() } else { "+".into() };
                res.tzoffset = None;
                if utczone(&l[i]) {
                    res.tzname = None;
                }
            }
        } else if res.hour.is_some() && (l[i] == "+" || l[i] == "-") {
            let signal = if l[i] == "+" { 1 } else { -1 };
            let next = tok(l, i + 1)?;
            let (hour_offset, min_offset) = if next.len() == 4 {
                (py_int(&next[..2])?, py_int(&next[2..])?)
            } else if i + 2 < n && l[i + 2] == ":" {
                let m = py_int(&tok(l, i + 3)?)?;
                i += 2;
                (py_int(&next)?, m)
            } else if next.len() <= 2 {
                (py_int(&next)?, 0)
            } else {
                return Err(Stop::Invalid);
            };
            res.tzoffset = Some(signal * (hour_offset * 3600 + min_offset * 60));
            if i + 5 < n
                && jump(&l[i + 2])
                && l[i + 3] == "("
                && l[i + 5] == ")"
                && l[i + 4].len() >= 3
                && could_be_tzname(&res, None, &l[i + 4])
            {
                res.tzname = Some(l[i + 4].clone());
                i += 4;
            }
            i += 1;
        } else if !jump(&l[i]) {
            return Err(Stop::Invalid);
        }
        i += 1;
    }
    let (year, month, day) = ymd.resolve()?;
    (res.year, res.month, res.day) = (year, month, day);
    Ok((res, ymd))
}

/// `info.validate(res)`: two-digit years defer; time zone normalisation.
fn validate(res: &mut Res, century_specified: bool) -> R<()> {
    if res.year.is_some_and(|y| y < 100) && !century_specified {
        return Err(Stop::Defer);
    }
    if (res.tzoffset == Some(0) && res.tzname.as_deref().is_none_or(str::is_empty))
        || matches!(res.tzname.as_deref(), Some("Z" | "z"))
    {
        res.tzname = Some("UTC".into());
        res.tzoffset = Some(0);
    } else if res.tzoffset != Some(0) && res.tzname.as_deref().is_some_and(|t| !t.is_empty() && utczone(t)) {
        res.tzoffset = Some(0);
    }
    Ok(())
}

/// `_build_naive` then `_build_tzaware` (no tzinfos).
fn build(res: &Res, default: &Default, local_tznames: &[String]) -> R<DateTime> {
    let fixed = match default {
        Default::Fixed(d) => Some(d),
        Default::TodayMidnight if res.year.is_none() || res.month.is_none() || res.day.is_none() => {
            return Err(Stop::Defer)
        }
        Default::TodayMidnight => None,
    };
    if res.weekday.is_some() && res.day.is_none() {
        return Err(Stop::Defer);
    }
    let pick = |v: Option<i64>, d: fn(&DateTime) -> i64, zero: i64| v.unwrap_or_else(|| fixed.map_or(zero, d));
    let year = pick(res.year, |d| d.year.into(), 0);
    let month = pick(res.month, |d| d.month.into(), 0);
    let mut day = pick(res.day, |d| d.day.into(), 0);
    if res.day.is_none() {
        if !(1..=12).contains(&month) {
            return Err(Stop::Invalid);
        }
        day = day.min(days_in_month(year, month));
    }
    let offset = match (res.tzname.as_deref(), res.tzoffset) {
        (Some(name), _) if local_tznames.iter().any(|t| t == name) => return Err(Stop::Defer),
        (_, Some(o)) if o.abs() < 86_400 => Some(o as i32),
        (None, None) => None,
        // an unknown name warns (UnknownTimezoneWarning); a day or more fails in utcoffset()
        _ => return Err(Stop::Defer),
    };
    let time = |v: Option<i64>, d: fn(&DateTime) -> u32| v.unwrap_or_else(|| fixed.map_or(0, |f| d(f).into()));
    DateTime::new(
        year.into(),
        month.into(),
        day.into(),
        time(res.hour, |d| d.hour).into(),
        time(res.minute, |d| d.minute).into(),
        time(res.second, |d| d.second).into(),
        time(res.microsecond, |d| d.micro).into(),
        offset,
    )
    .map_err(|_| Stop::Invalid)
}

/// `htmldate.extractors.dateutil_parse(s, fuzzy=False)` with htmldate's exception handling: `Some(None)` where
/// htmldate catches the error, `None` where this port defers to Python. `local_tznames` is `time.tzname`.
pub fn parse(s: &str, default: &Default, local_tznames: &[String]) -> Option<Option<DateTime>> {
    if !s.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return None;
    }
    let mut tokens = timelex::split(s);
    let mut run = || -> R<DateTime> {
        let (mut res, ymd) = parse_tokens(&mut tokens)?;
        validate(&mut res, ymd.century_specified)?;
        let set = [res.year, res.month, res.day, res.weekday, res.hour, res.minute, res.second, res.microsecond]
            .iter()
            .any(Option::is_some)
            || res.tzname.is_some()
            || res.tzoffset.is_some()
            || res.ampm.is_some();
        if !set {
            return Err(Stop::Invalid);
        }
        build(&res, default, local_tznames)
    };
    match run() {
        Ok(d) => Some(Some(d)),
        Err(Stop::Invalid) => Some(None),
        Err(Stop::Defer) => None,
    }
}
