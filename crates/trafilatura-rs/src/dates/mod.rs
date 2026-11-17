//! Native shortcuts for the date parsing htmldate delegates to python-dateutil and dateparser (see
//! docs/metadata-formats-scope.md). Each returns a result only where it provably equals the Python call for the
//! versions in `REPRODUCES_DATES` (python/trafilatura_rs/_versions.py); otherwise the caller runs Python.
mod dateparser;
pub mod dateutil;
#[rustfmt::skip]
mod regexes;
#[cfg(test)]
mod tests;

use crate::py::int::py_int;
use crate::py::is_digit;
use regexes::{WORD, YEAR_OR_MONTH};

/// `_fold_month_name(token)`; None where Rust's lowercasing (a newer Unicode) might differ from Python's.
fn fold_month_name(token: &str) -> Option<String> {
    let folded = token.replace(['ı', 'İ'], "i");
    let stable = |c: char| (c as u32) < 0x530 || c.to_lowercase().eq(std::iter::once(c));
    folded.chars().all(stable).then(|| folded.to_lowercase())
}

/// htmldate's `has_date_cue(s, datetime(min_year, 1, 1), datetime(max_year, 1, 1))`; None when unknown.
pub fn has_date_cue(s: &str, min_year: i32, max_year: i32) -> Option<bool> {
    if let Some(m) = YEAR_OR_MONTH.search(s) {
        let found = m.get(0).expect("group 0").as_str();
        if !found.starts_with(is_digit) {
            return Some(true);
        }
        let year = py_int(found).ok()?;
        return Some((i128::from(min_year)..=i128::from(max_year)).contains(&year));
    }
    let months = dateparser::month_words();
    for word in WORD.findall(s) {
        if months.contains(fold_month_name(word)?.as_str()) {
            return Some(true);
        }
    }
    Some(false)
}

/// Whether `_external_date(s) if has_date_cue(s, ...) else None` (htmldate.extractors) is certainly None. Non-ASCII
/// strings are tried only with `unicode` (see `unicode_versions`).
pub fn external_date_is_none(s: &str, min_year: i32, max_year: i32, unicode: bool) -> bool {
    has_date_cue(s, min_year, max_year) == Some(false) || dateparser::no_locale_applies(s, unicode)
}

/// htmldate's `_external_date(s)` (dateparser's `get_date_data(s)["date_obj"]`) on the local date `today` (an
/// ordinal); None when unknown. Only for the `unicode_versions()` installed, and a local timezone tzlocal finds.
pub fn external_date(s: &str, today: i64) -> Option<Option<crate::py::datetime::DateTime>> {
    dateparser::data_consistent().then(|| dateparser::date_obj(s, today)).flatten()
}

/// `regex` releases other than the stamped one whose four generated tables are byte-identical to the committed ones
/// and whose dateparser output equals the port's (CONTRIBUTING.md, after a `regex` bump).
const SAME_TABLES_REGEX: &[&str] = &["2026.11.15"];

/// Whether the native dateparser holds for this `unicodedata.unidata_version` and `regex.__version__`: dateparser
/// reads strings through both, so only the versions its tables were generated (or checked) with qualify.
pub fn unicode_supported(unidata: &str, regex: &str) -> bool {
    let (stamped_unidata, stamped_regex) = dateparser::versions();
    unidata == stamped_unidata && (regex == stamped_regex || SAME_TABLES_REGEX.contains(&regex))
}
