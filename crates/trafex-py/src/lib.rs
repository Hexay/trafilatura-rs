//! `trafex._core`: extraction entry points. Python-level argument checking and the trafilatura monkeypatch live
//! in python/trafex/__init__.py.
use pyo3::exceptions::{PyAssertionError, PyAttributeError, PyIndexError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use std::sync::{LazyLock, Mutex};
use trafex::dom::Error;
use trafex::trafilatura::core;
use trafex::trafilatura::deduplication::Lru;
use trafex::trafilatura::settings::{Focus, Options};

/// trafilatura keeps its deduplication LRU module-global; so does the module-level `extract`.
static GLOBAL_LRU: LazyLock<Mutex<Lru>> = LazyLock::new(|| Mutex::new(Lru::default()));

fn to_py_err(e: Error) -> PyErr {
    match e {
        Error::Value(m) => PyValueError::new_err(m),
        Error::Type(m) => PyTypeError::new_err(m),
        Error::Parser(m) | Error::Syntax(m) => PyValueError::new_err(m),
        Error::Runtime(m) => {
            let (kind, msg) = m.split_once(": ").unwrap_or(("RuntimeError", &m));
            let msg = msg.to_owned();
            match kind {
                "AttributeError" => PyAttributeError::new_err(msg),
                "IndexError" => PyIndexError::new_err(msg),
                "KeyError" => PyKeyError::new_err(msg),
                "AssertionError" => PyAssertionError::new_err(msg),
                _ => PyRuntimeError::new_err(m),
            }
        }
    }
}

fn options(favor_precision: bool, favor_recall: bool, include_tables: bool, deduplicate: bool, no_fallback: bool) -> Options {
    let focus = if favor_recall {
        Focus::Recall
    } else if favor_precision {
        Focus::Precision
    } else {
        Focus::Balanced
    };
    Options { focus, comments: false, tables: include_tables, dedup: deduplicate, fast: no_fallback, ..Options::default() }
}

/// `trafilatura.extract` (txt output, include_comments=False) with the process-wide dedup state.
#[pyfunction]
#[pyo3(signature = (html, favor_precision=false, favor_recall=false, include_tables=true, deduplicate=false, no_fallback=false))]
fn extract(
    py: Python<'_>,
    html: &str,
    favor_precision: bool,
    favor_recall: bool,
    include_tables: bool,
    deduplicate: bool,
    no_fallback: bool,
) -> PyResult<Option<String>> {
    let opts = options(favor_precision, favor_recall, include_tables, deduplicate, no_fallback);
    py.detach(|| {
        if deduplicate {
            core::extract(html, &opts, &mut GLOBAL_LRU.lock().expect("lru lock"))
        } else {
            core::extract(html, &opts, &mut Lru::default())
        }
    })
    .map_err(to_py_err)
}

/// Clear the module-level dedup state (`trafilatura.meta.reset_caches` equivalent for this module).
#[pyfunction]
fn reset_caches() {
    GLOBAL_LRU.lock().expect("lru lock").clear();
}

/// An extractor with its own dedup state, independent of the module-level one.
#[pyclass]
struct Extractor {
    inner: core::Extractor,
}

#[pymethods]
impl Extractor {
    #[new]
    #[pyo3(signature = (favor_precision=false, favor_recall=false, include_tables=true, deduplicate=false, no_fallback=false))]
    fn new(favor_precision: bool, favor_recall: bool, include_tables: bool, deduplicate: bool, no_fallback: bool) -> Self {
        let opts = options(favor_precision, favor_recall, include_tables, deduplicate, no_fallback);
        Extractor { inner: core::Extractor::new(opts) }
    }

    fn extract(&mut self, py: Python<'_>, html: &str) -> PyResult<Option<String>> {
        let inner = &mut self.inner;
        py.detach(|| inner.extract(html)).map_err(to_py_err)
    }
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(reset_caches, m)?)?;
    m.add_class::<Extractor>()?;
    Ok(())
}
