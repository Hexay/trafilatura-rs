//! `trafilatura_rs._core`: extraction entry points. Python-level argument checking, fallback and the trafilatura
//! monkeypatch live in python/trafilatura_rs/.
mod hooks;

use hooks::PyHooks;
use pyo3::create_exception;
use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyException, PyIndexError, PyKeyError, PyRuntimeError, PyTypeError,
    PyValueError,
};
use pyo3::prelude::*;
use std::sync::{LazyLock, Mutex};
use trafilatura_rs::dom::{Ctx, Error};
use trafilatura_rs::htmldate::{find_date, DateParams};
use trafilatura_rs::trafilatura::core::{self, Request};
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::settings::{Focus, Format, Options};
use trafilatura_rs::trafilatura::utils::load_html;

create_exception!(_core, Unsupported, PyException, "The call needs a code path only Python trafilatura has.");

/// trafilatura keeps its deduplication LRU module-global; so does the module-level `extract`.
static GLOBAL_LRU: LazyLock<Mutex<Lru>> = LazyLock::new(|| Mutex::new(Lru::default()));

fn to_py_err(e: Error) -> PyErr {
    match e {
        Error::Value(m) => PyValueError::new_err(m),
        Error::Type(m) => PyTypeError::new_err(m),
        Error::Parser(m) | Error::Syntax(m) => PyValueError::new_err(m),
        Error::Unsupported(m) => Unsupported::new_err(m),
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

fn format(output_format: &str) -> PyResult<Format> {
    match output_format {
        "txt" => Ok(Format::Txt),
        "markdown" => Ok(Format::Markdown),
        "json" => Ok(Format::Json),
        other => Err(PyValueError::new_err(format!("output_format {other:?} is not ported"))),
    }
}

#[allow(clippy::too_many_arguments)]
fn options(
    favor_precision: bool,
    favor_recall: bool,
    include_tables: bool,
    deduplicate: bool,
    no_fallback: bool,
    include_comments: bool,
    output_format: &str,
    with_metadata: bool,
) -> PyResult<Options> {
    let focus = if favor_recall {
        Focus::Recall
    } else if favor_precision {
        Focus::Precision
    } else {
        Focus::Balanced
    };
    let opts = Options {
        focus,
        comments: include_comments,
        tables: include_tables,
        dedup: deduplicate,
        fast: no_fallback,
        with_metadata,
        ..Options::default()
    };
    Ok(opts.with_format(format(output_format)?))
}

fn request<'a>(url: Option<&'a str>, record_id: Option<&'a str>, max_date: Option<&'a str>) -> Request<'a> {
    Request { url, record_id, max_date, hooks: &PyHooks }
}

/// `trafilatura.extract` (trafilatura's defaults) with the process-wide dedup state. `max_date` is today's local
/// date as htmldate's `set_date_params` formats it (needed with metadata). Raises `Unsupported` where only Python
/// trafilatura can answer.
#[pyfunction]
#[pyo3(signature = (html, favor_precision=false, favor_recall=false, include_tables=true, deduplicate=false,
    no_fallback=false, include_comments=true, output_format="txt", with_metadata=false, url=None, record_id=None,
    max_date=None))]
#[allow(clippy::too_many_arguments)]
fn extract(
    py: Python<'_>,
    html: &str,
    favor_precision: bool,
    favor_recall: bool,
    include_tables: bool,
    deduplicate: bool,
    no_fallback: bool,
    include_comments: bool,
    output_format: &str,
    with_metadata: bool,
    url: Option<&str>,
    record_id: Option<&str>,
    max_date: Option<&str>,
) -> PyResult<Option<String>> {
    let opts = options(
        favor_precision,
        favor_recall,
        include_tables,
        deduplicate,
        no_fallback,
        include_comments,
        output_format,
        with_metadata,
    )?;
    let req = request(url, record_id, max_date);
    py.detach(|| {
        if deduplicate {
            core::extract_with(html, &opts, &req, &mut GLOBAL_LRU.lock().expect("lru lock"))
        } else {
            core::extract_with(html, &opts, &req, &mut Lru::default())
        }
    })
    .map_err(to_py_err)
}

/// htmldate's `find_date(trafilatura.utils.load_html(html), original_date=True, extensive_search=True,
/// max_date=max_date, url=url)`, for function-level parity checks.
#[pyfunction]
#[pyo3(signature = (html, max_date, url=None))]
fn _find_date(py: Python<'_>, html: &str, max_date: &str, url: Option<&str>) -> PyResult<Option<String>> {
    let params = DateParams { original_date: true, extensive_search: true, max_date: max_date.to_owned() };
    py.detach(|| {
        let ctx = Ctx::new();
        let Some(tree) = load_html(&ctx, html) else { return Ok(None) };
        find_date(&ctx, tree, url, &params, &PyHooks)
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
    #[pyo3(signature = (favor_precision=false, favor_recall=false, include_tables=true, deduplicate=false,
        no_fallback=false, include_comments=true, output_format="txt", with_metadata=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        favor_precision: bool,
        favor_recall: bool,
        include_tables: bool,
        deduplicate: bool,
        no_fallback: bool,
        include_comments: bool,
        output_format: &str,
        with_metadata: bool,
    ) -> PyResult<Self> {
        let opts = options(
            favor_precision,
            favor_recall,
            include_tables,
            deduplicate,
            no_fallback,
            include_comments,
            output_format,
            with_metadata,
        )?;
        Ok(Extractor { inner: core::Extractor::new(opts) })
    }

    #[pyo3(signature = (html, url=None, record_id=None, max_date=None))]
    fn extract(
        &mut self,
        py: Python<'_>,
        html: &str,
        url: Option<&str>,
        record_id: Option<&str>,
        max_date: Option<&str>,
    ) -> PyResult<Option<String>> {
        let inner = &mut self.inner;
        let req = request(url, record_id, max_date);
        py.detach(|| inner.extract_with(html, &req)).map_err(to_py_err)
    }
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(_find_date, m)?)?;
    m.add_function(wrap_pyfunction!(reset_caches, m)?)?;
    m.add_class::<Extractor>()?;
    m.add("Unsupported", m.py().get_type::<Unsupported>())?;
    Ok(())
}
