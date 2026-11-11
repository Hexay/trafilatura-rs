//! htmldate/validators.py.
use super::{caught, cslice, Env};
use crate::dom::{Error, Result};
use crate::py::datetime::DateTime;
use crate::py::int::py_int;
use crate::py::pyre::{group, Pattern};
use std::collections::HashMap;

/// Lossless whatever the output format.
pub(super) const REFERENCE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S.%f%z";

/// A `collections.Counter` of strings: first-occurrence order, counts.
pub(super) type Counter = Vec<(String, usize)>;

pub(super) fn counter<'a>(items: impl IntoIterator<Item = &'a str>) -> Counter {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut out: Counter = Vec::new();
    for item in items {
        match index.get(item) {
            Some(&i) => out[i].1 += 1,
            None => {
                index.insert(item, out.len());
                out.push((item.to_owned(), 1));
            }
        }
    }
    out
}

/// `_is_in_range`: timestamps (naive = local time), wall clock where the platform has no timestamp.
fn is_in_range(d: &DateTime, earliest: &DateTime, latest: &DateTime) -> bool {
    if !(earliest.year <= d.year && d.year <= latest.year) {
        return false;
    }
    let wall_clock = || earliest.naive_key() <= d.naive_key() && d.naive_key() <= latest.naive_key();
    let (Some(e), Some(x)) = (earliest.timestamp_micros(), d.timestamp_micros()) else { return wall_clock() };
    if e > x {
        return false;
    }
    match latest.timestamp_micros() {
        Some(l) => x <= l,
        None => wall_clock(),
    }
}

/// `is_valid_date` for a datetime.
pub(super) fn is_valid_datetime(d: &DateTime, env: &Env) -> bool {
    is_in_range(d, &env.min, &env.max)
}

/// `is_valid_date` for a string in "%Y-%m-%d" (`_parse_and_validate`).
pub(super) fn is_valid_date_str(s: &str, env: &Env) -> Result<bool> {
    let positional =
        (|| DateTime::ymd(py_int(cslice(s, 0, 4))?, py_int(cslice(s, 5, 7))?, py_int(cslice(s, 8, 10))?))();
    let dateobject = match positional {
        Ok(d) => d,
        Err(Error::Value(_)) => match caught(DateTime::strptime_ymd(s))? {
            Some(d) => d,
            None => return Ok(false),
        },
        Err(e) => return Err(e),
    };
    Ok(is_in_range(&dateobject, &env.min, &env.max))
}

/// `validate_and_convert`.
pub(super) fn validate_and_convert(date_input: Option<DateTime>, outputformat: &str, env: &Env) -> Option<String> {
    date_input.filter(|d| is_valid_datetime(d, env)).map(|d| d.strftime(outputformat))
}

/// `correct_year`.
pub(super) fn correct_year(year: i128) -> i128 {
    if year < 100 {
        year + if year >= 90 { 1900 } else { 2000 }
    } else {
        year
    }
}

/// `plausible_year_filter`.
pub(super) fn plausible_year_filter(html: &str, pattern: &Pattern, yearpat: &Pattern, env: &Env) -> Result<Counter> {
    let occurrences = counter(pattern.findall(html));
    let (min_year, max_year) = (i128::from(env.min.year), i128::from(env.max.year));
    let mut kept = Vec::with_capacity(occurrences.len());
    for (item, count) in occurrences {
        let Some(year_match) = yearpat.search(&item) else { continue };
        let potential_year = correct_year(py_int(group(&year_match, 1).unwrap_or_default())?);
        if min_year <= potential_year && potential_year <= max_year {
            kept.push((item, count));
        }
    }
    Ok(kept)
}

/// `update_reference`: oldest if original, else newest, compared as wall clock; ties keep the reference.
pub(super) fn update_reference(reference: Option<DateTime>, candidate: DateTime, original: bool) -> DateTime {
    match reference {
        None => candidate,
        Some(r) if original && candidate.naive_key() < r.naive_key() => candidate,
        Some(r) if !original && candidate.naive_key() > r.naive_key() => candidate,
        Some(r) => r,
    }
}

/// `compare_values`.
pub(super) fn compare_values(reference: Option<DateTime>, attempt: &str, env: &Env) -> Result<Option<DateTime>> {
    // The strptime(REFERENCE_FORMAT) fallback only reads what fromisoformat rejects: years below 1000,
    // which validation already excluded.
    Ok(match caught(DateTime::fromisoformat(attempt))? {
        Some(candidate) => Some(update_reference(reference, candidate, env.original)),
        None => reference,
    })
}

/// `filter_ymd_candidate` (`bestmatch` is `match.groups()`).
pub(super) fn filter_ymd_candidate(
    bestmatch: Option<&[Option<String>]>,
    copyear: i128,
    env: &Env,
) -> Result<Option<String>> {
    let Some(groups) = bestmatch else { return Ok(None) };
    let parts: Option<Vec<&str>> = groups.iter().take(3).map(|g| g.as_deref()).collect();
    let pagedate =
        parts.ok_or_else(|| Error::Type("sequence item: expected str instance, NoneType found".into()))?.join("-");
    if is_valid_date_str(&pagedate, env)?
        && (copyear == 0 || py_int(groups[0].as_deref().unwrap_or_default())? >= copyear)
    {
        return convert_date(&pagedate, env.format).map(Some);
    }
    Ok(None)
}

/// `convert_date(datestring, "%Y-%m-%d", outputformat)`.
pub(super) fn convert_date(datestring: &str, outputformat: &str) -> Result<String> {
    Ok(DateTime::strptime_ymd(datestring)?.strftime(outputformat))
}

/// `check_extracted_reference`.
pub(super) fn check_extracted_reference(reference: Option<DateTime>, env: &Env) -> Option<String> {
    validate_and_convert(reference, env.format, env)
}
