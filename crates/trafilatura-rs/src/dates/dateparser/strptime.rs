//! The `strptime` directives dateparser's `_parser` uses (dateparser.utils.strptime, with English names): Python's
//! `_strptime` matches the format's regex at the start, then fails if anything is left.
use regex::Regex;
use std::sync::LazyLock;

const WEEKDAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Num {
    Day,
    Month,
    Year2,
    Year4,
}

/// Per-character tests of one alternative of a directive's regex.
type Alternative<'a> = &'a [fn(u8) -> bool];

/// The length of the first of `alternatives` matching at the start of `token`.
fn first_alternative(token: &[u8], alternatives: &[Alternative]) -> Option<usize> {
    alternatives
        .iter()
        .find(|alt| alt.len() <= token.len() && alt.iter().zip(token).all(|(test, &b)| test(b)))
        .map(|alt| alt.len())
}

fn digit(b: u8) -> bool {
    b.is_ascii_digit()
}

/// `strptime(token, directive)` for %d, %m, %y, %Y alone: the value, or None (ValueError).
pub(super) fn number(token: &str, directive: Num) -> Option<i64> {
    let b = token.as_bytes();
    let len = match directive {
        Num::Day => first_alternative(
            b,
            &[
                &[|c| c == b'3', |c| c == b'0' || c == b'1'],
                &[|c| c == b'1' || c == b'2', digit],
                &[|c| c == b'0', |c| (b'1'..=b'9').contains(&c)],
                &[|c| (b'1'..=b'9').contains(&c)],
                &[|c| c == b' ', |c| (b'1'..=b'9').contains(&c)],
            ],
        ),
        Num::Month => first_alternative(
            b,
            &[
                &[|c| c == b'1', |c| (b'0'..=b'2').contains(&c)],
                &[|c| c == b'0', |c| (b'1'..=b'9').contains(&c)],
                &[|c| (b'1'..=b'9').contains(&c)],
            ],
        ),
        Num::Year2 => first_alternative(b, &[&[digit, digit]]),
        Num::Year4 => first_alternative(b, &[&[digit, digit, digit, digit]]),
    }?;
    if len != b.len() {
        return None;
    }
    let v: i64 = token.trim_start().parse().ok()?;
    Some(match directive {
        Num::Year2 if v <= 68 => v + 2000,
        Num::Year2 => v + 1900,
        // datetime(0, 1, 1) raises
        Num::Year4 if v == 0 => return None,
        _ => v,
    })
}

/// %B or %b (`full`), %A or %a: the 1-based month or weekday index.
pub(super) fn name(token: &str, weekday: bool, full: bool) -> Option<usize> {
    let names: &[&str] = if weekday { &WEEKDAYS } else { &MONTHS };
    let lower = token.to_ascii_lowercase();
    if !token.is_ascii() {
        return None;
    }
    names.iter().position(|n| if full { *n == lower } else { n[..3] == lower }).map(|i| i + 1)
}

/// `_time_parser.time_directives`, as `_strptime` compiles them (IGNORECASE, whitespace as `\s+`).
static TIME_FORMATS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let (h, i, m, s, p, f) = (
        r"(?P<H>2[0-3]|[0-1]\d|\d)",
        r"(?P<I>1[0-2]|0[1-9]|[1-9])",
        r"(?P<M>[0-5]\d|\d)",
        r"(?P<S>6[0-1]|[0-5]\d|\d)",
        r"(?P<p>am|pm)",
        r"(?P<f>[0-9]{1,6})",
    );
    [
        format!("{h}:{m}:{s}"),
        format!(r"{i}:{m}:{s}\s+{p}"),
        format!("{h}:{m}"),
        format!(r"{i}:{m}\s+{p}"),
        format!(r"{i}\s+{p}"),
        format!(r"{h}:{m}:{s}\.{f}"),
        format!(r"{i}:{m}:{s}\.{f}\s+{p}"),
        format!(r"{h}:{m}\s+{p}"),
    ]
    .iter()
    .map(|src| Regex::new(&format!(r"(?i)\A(?:{src})")).expect("a time format"))
    .collect()
});

/// `time_parser(timestring)`: (hour, minute, second, microsecond), or None (ValueError).
pub(super) fn time(timestring: &str) -> Option<(i64, i64, i64, i64)> {
    let t = timestring.trim_matches(|c: char| super::unicode::is(super::unicode::Class::PySpace, c));
    if !t.is_ascii() {
        return None;
    }
    TIME_FORMATS.iter().find_map(|re| {
        let c = re.captures(t)?;
        if c.get(0)?.end() != t.len() {
            return None;
        }
        let int = |name: &str| c.name(name).map(|m| m.as_str().parse::<i64>().expect("digits"));
        let hour = match (int("H"), int("I")) {
            (Some(h), _) => h,
            (None, Some(h)) => {
                let pm = c.name("p").is_some_and(|p| p.as_str().eq_ignore_ascii_case("pm"));
                match (pm, h) {
                    (false, 12) => 0,
                    (true, 12) => 12,
                    (true, h) => h + 12,
                    (false, h) => h,
                }
            }
            (None, None) => 0,
        };
        let second = int("S").unwrap_or(0);
        // datetime() rejects the leap seconds the regex allows; the next format is tried
        if second > 59 {
            return None;
        }
        let micro = c.name("f").map_or(0, |m| format!("{:0<6}", m.as_str()).parse().expect("digits"));
        Some((hour, int("M").unwrap_or(0), second, micro))
    })
}
