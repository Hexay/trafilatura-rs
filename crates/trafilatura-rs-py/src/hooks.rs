//! The Python-side leaves of metadata extraction (trafilatura_rs::hooks), called back into the running interpreter.
//! Functions are looked up at call time in the modules that use them, so test-time patches apply.
use crate::{dates, memo};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use trafilatura_rs::dom::{Error, Result};
use trafilatura_rs::hooks::Hooks;
use trafilatura_rs::py::datetime::DateTime;

pub struct PyHooks;

fn convert(e: PyErr) -> Error {
    Python::attach(|py| {
        if e.is_instance_of::<PyValueError>(py) {
            Error::Value(e.to_string())
        } else {
            Error::Unsupported(format!("Python raised {e}"))
        }
    })
}

pub(crate) fn to_datetime(obj: &Bound<'_, PyAny>) -> PyResult<Option<DateTime>> {
    if obj.is_none() {
        return Ok(None);
    }
    let int = |name: &str| -> PyResult<i64> { obj.getattr(name)?.extract() };
    let offset = obj.call_method0("utcoffset")?;
    let offset = if offset.is_none() {
        None
    } else {
        let part = |name: &str| -> PyResult<i64> { offset.getattr(name)?.extract() };
        if part("microseconds")? != 0 {
            return Err(PyTypeError::new_err("sub-second UTC offset"));
        }
        Some((part("days")? * 86400 + part("seconds")?) as i32)
    };
    Ok(Some(DateTime {
        year: int("year")? as i32,
        month: int("month")? as u32,
        day: int("day")? as u32,
        hour: int("hour")? as u32,
        minute: int("minute")? as u32,
        second: int("second")? as u32,
        micro: int("microsecond")? as u32,
        offset,
    }))
}

fn func<'py>(py: Python<'py>, module: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)
}

fn call<T>(f: impl FnOnce(Python<'_>) -> PyResult<T>) -> Result<T> {
    Python::attach(f).map_err(convert)
}

impl Hooks for PyHooks {
    fn dateutil_parse(&self, s: &str) -> Result<Option<DateTime>> {
        call(|py| {
            if let Some(native) = dates::enabled(py).then(|| dates::dateutil_parse(py, s)).flatten() {
                return Ok(native);
            }
            let kwargs = PyDict::new(py);
            kwargs.set_item("fuzzy", false)?;
            match func(py, "htmldate.extractors", "dateutil_parse")?.call((s,), Some(&kwargs)) {
                Ok(dt) => to_datetime(&dt).map_err(|e| PyTypeError::new_err(e.to_string())),
                Err(e)
                    if e.is_instance_of::<PyOverflowError>(py)
                        || e.is_instance_of::<PyTypeError>(py)
                        || e.is_instance_of::<PyValueError>(py) =>
                {
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        })
    }

    fn external_date(&self, s: &str, min_year: i32, max_year: i32) -> Result<Option<DateTime>> {
        call(|py| {
            if dates::external_date_is_none(py, s, min_year, max_year) {
                return Ok(None);
            }
            let datetime = func(py, "datetime", "datetime")?;
            let (lo, hi) = (datetime.call1((min_year, 1, 1))?, datetime.call1((max_year, 1, 1))?);
            if !func(py, "htmldate.extractors", "has_date_cue")?.call1((s, lo, hi))?.is_truthy()? {
                return Ok(None);
            }
            let external = func(py, "htmldate.extractors", "_external_date")?;
            if dates::enabled(py) {
                return memo::external_date(py, &external, s);
            }
            to_datetime(&external.call1((s,))?)
        })
    }

    fn is_valid_url(&self, url: &str) -> Result<bool> {
        call(|py| func(py, "trafilatura.metadata", "is_valid_url")?.call1((url,))?.extract())
    }

    fn get_base_url(&self, url: &str) -> Result<String> {
        call(|py| func(py, "trafilatura.metadata", "get_base_url")?.call1((url,))?.extract())
    }

    fn safe_base_url(&self, url: &str) -> Result<String> {
        call(|py| func(py, "trafilatura.metadata", "safe_base_url")?.call1((url,))?.extract())
    }

    fn validated_url(&self, url: &str) -> Result<Option<String>> {
        call(|py| {
            let (valid, parsed): (bool, Bound<'_, PyAny>) =
                func(py, "trafilatura.metadata", "validate_url")?.call1((url,))?.extract()?;
            if !valid {
                return Ok(None);
            }
            func(py, "trafilatura.metadata", "normalize_url")?.call1((parsed,))?.extract().map(Some)
        })
    }

    fn extract_domain(&self, url: &str) -> Result<Option<String>> {
        call(|py| {
            let kwargs = PyDict::new(py);
            kwargs.set_item("fast", true)?;
            func(py, "trafilatura.metadata", "extract_domain")?.call((url,), Some(&kwargs))?.extract()
        })
    }
}
