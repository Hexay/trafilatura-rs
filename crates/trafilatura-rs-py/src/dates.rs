//! The native date shortcuts (`trafilatura_rs::dates`) behind `PyHooks`: on when the installed dateparser and
//! python-dateutil are the versions `trafilatura_rs._versions.REPRODUCES_DATES` names, else every call runs Python.
use crate::hooks::to_datetime;
use pyo3::prelude::*;
use std::sync::{Mutex, OnceLock};
use trafilatura_rs::dates::{self, dateutil};
use trafilatura_rs::py::datetime::DateTime;

static ENABLED: OnceLock<bool> = OnceLock::new();
static UNICODE: OnceLock<bool> = OnceLock::new();
static LOCAL_TZNAMES: OnceLock<Vec<String>> = OnceLock::new();

pub(crate) fn enabled(py: Python<'_>) -> bool {
    *ENABLED.get_or_init(|| {
        let check = || py.import("trafilatura_rs._versions")?.call_method0("native_dates")?.extract();
        check().unwrap_or(false)
    })
}

/// Whether the running `unicodedata` and `regex` are the ones the non-ASCII dateparser shortcut was generated for.
fn unicode(py: Python<'_>) -> bool {
    *UNICODE.get_or_init(|| {
        let check = || -> PyResult<bool> {
            let unidata: String = py.import("unicodedata")?.getattr("unidata_version")?.extract()?;
            let regex: String = py.import("regex")?.getattr("__version__")?.extract()?;
            Ok((unidata.as_str(), regex.as_str()) == dates::unicode_versions())
        };
        check().unwrap_or(false)
    })
}

/// `_external_date(s) if has_date_cue(s, ...) else None` is certainly None.
pub(crate) fn external_date_is_none(py: Python<'_>, s: &str, min_year: i32, max_year: i32) -> bool {
    enabled(py) && dates::external_date_is_none(s, min_year, max_year, unicode(py))
}

/// (`_external_date`, its parser, whether `trafilatura_rs._dates.stock_external_date` holds) last checked.
type Checked = Option<(Py<PyAny>, Py<PyAny>, bool)>;
static STOCK: Mutex<Checked> = Mutex::new(None);

fn today(py: Python<'_>) -> PyResult<i64> {
    py.import("datetime")?.getattr("date")?.call_method0("today")?.call_method0("toordinal")?.extract()
}

/// Whether `function` (htmldate's `_external_date`) and htmldate's parser are what the native port reproduces.
fn stock_external_date(py: Python<'_>, function: &Bound<'_, PyAny>) -> PyResult<bool> {
    let parser = py.import("htmldate.extractors")?.getattr("get_external_parser")?.call0()?;
    let known =
        |s: &Checked| s.as_ref().filter(|(f, p, _)| f.bind(py).is(function) && p.bind(py).is(&parser)).map(|s| s.2);
    if let Some(ok) = known(&STOCK.lock().expect("stock lock")) {
        return Ok(ok);
    }
    // not under the lock: the check runs Python
    let ok = py.import("trafilatura_rs._dates")?.call_method1("stock_external_date", (function, &parser))?.extract()?;
    *STOCK.lock().expect("stock lock") = Some((function.clone().unbind(), parser.unbind(), ok));
    Ok(ok)
}

/// `function(s)` (htmldate's `_external_date`) natively, or None to run Python.
pub(crate) fn external_date(
    py: Python<'_>,
    function: &Bound<'_, PyAny>,
    s: &str,
) -> PyResult<Option<Option<DateTime>>> {
    if !(enabled(py) && unicode(py) && stock_external_date(py, function)?) {
        return Ok(None);
    }
    let today = today(py)?;
    // a bug in the port must not fail the extraction: Python decides instead
    Ok(std::panic::catch_unwind(|| dates::external_date(s, today)).ok().flatten())
}

/// `time.tzname` when a name there makes dateutil use tzlocal with a nonzero offset; empty for a fixed UTC zone.
fn local_tznames(py: Python<'_>) -> &'static [String] {
    LOCAL_TZNAMES.get_or_init(|| {
        let read = || -> PyResult<Vec<String>> {
            let time = py.import("time")?;
            let utc = time.getattr("timezone")?.extract::<i64>()? == 0
                && (time.getattr("daylight")?.extract::<i64>()? == 0
                    || time.getattr("altzone")?.extract::<i64>()? == 0);
            Ok(if utc { Vec::new() } else { time.getattr("tzname")?.extract()? })
        };
        read().unwrap_or_else(|_| ["UTC", "GMT"].map(String::from).to_vec())
    })
}

/// The `default` of `htmldate.extractors.dateutil_parse`: dateutil's own `parse` (today at midnight) or a
/// `functools.partial` of it setting only a naive `default`; None for anything else.
fn dateutil_default(py: Python<'_>) -> PyResult<Option<dateutil::Default>> {
    let f = py.import("htmldate.extractors")?.getattr("dateutil_parse")?;
    let parse = py.import("dateutil.parser")?.getattr("parse")?;
    if f.is(&parse) {
        return Ok(Some(dateutil::Default::TodayMidnight));
    }
    let partial = py.import("functools")?.getattr("partial")?;
    if !f.is_instance(&partial)? || !f.getattr("func")?.is(&parse) || f.getattr("args")?.len()? != 0 {
        return Ok(None);
    }
    let keywords = f.getattr("keywords")?;
    if keywords.len()? != 1 {
        return Ok(None);
    }
    let Ok(default) = keywords.get_item("default") else { return Ok(None) };
    if !default.getattr("tzinfo")?.is_none() {
        return Ok(None);
    }
    Ok(to_datetime(&default)?.map(dateutil::Default::Fixed))
}

/// `dateutil_parse(s, fuzzy=False)` as htmldate handles it, or None to run Python.
pub(crate) fn dateutil_parse(py: Python<'_>, s: &str) -> Option<Option<DateTime>> {
    let default = dateutil_default(py).ok().flatten()?;
    dateutil::parse(s, &default, local_tznames(py))
}

fn as_tuple(d: DateTime) -> (i32, u32, u32, u32, u32, u32, u32, Option<i32>) {
    (d.year, d.month, d.day, d.hour, d.minute, d.second, d.micro, d.offset)
}

/// Differential tests: the native `dateutil_parse` with `default` (None: today at midnight), as "defer", None or
/// (year, month, day, hour, minute, second, microsecond, utc offset seconds).
#[pyfunction]
#[pyo3(signature = (s, default=None))]
pub(crate) fn _native_dateutil<'py>(
    py: Python<'py>,
    s: &str,
    default: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let default = match default {
        Some(d) => dateutil::Default::Fixed(to_datetime(&d)?.expect("a datetime")),
        None => dateutil::Default::TodayMidnight,
    };
    Ok(match dateutil::parse(s, &default, local_tznames(py)) {
        None => "defer".into_pyobject(py)?.into_any(),
        Some(None) => py.None().into_bound(py),
        Some(Some(d)) => as_tuple(d).into_pyobject(py)?.into_any(),
    })
}

/// Differential tests: the native `_external_date(s)` on the local date `today` (an ordinal; None: today), as
/// "defer", None or (year, month, day, hour, minute, second, microsecond, None).
#[pyfunction]
#[pyo3(signature = (s, today=None))]
pub(crate) fn _native_dateparser<'py>(py: Python<'py>, s: &str, today: Option<i64>) -> PyResult<Bound<'py, PyAny>> {
    let today = match today {
        Some(t) => t,
        None => self::today(py)?,
    };
    Ok(match dates::external_date(s, today) {
        None => "defer".into_pyobject(py)?.into_any(),
        Some(None) => py.None().into_bound(py),
        Some(Some(d)) => as_tuple(d).into_pyobject(py)?.into_any(),
    })
}

/// Differential tests: (`has_date_cue`, whether `_external_date` is provably None) natively.
#[pyfunction]
pub(crate) fn _native_external_date(py: Python<'_>, s: &str, min_year: i32, max_year: i32) -> (Option<bool>, bool) {
    (dates::has_date_cue(s, min_year, max_year), dates::external_date_is_none(s, min_year, max_year, unicode(py)))
}
