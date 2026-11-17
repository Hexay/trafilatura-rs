//! `trafilatura_rs._core`: extraction entry points per reproduced trafilatura version. Python-level argument checking,
//! engine selection, fallback and the trafilatura monkeypatch live in python/trafilatura_rs/.
mod bare;
mod convert;
mod dates;
mod dedup;
mod extra;
mod extractor;
mod funcs;
mod hooks;
mod langid;
mod memo;
mod tree;

use hooks::PyHooks;
use pyo3::create_exception;
use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyException, PyIndexError, PyKeyError, PyRuntimeError, PyTypeError,
    PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use trafilatura_rs::dom::{Ctx, Error, Node, Result};
use trafilatura_rs::htmldate::{find_date, DateParams};
use trafilatura_rs::trafilatura::core::Request;
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::settings::{Focus, Format, Options};
use trafilatura_rs::trafilatura::utils::load_html;
use trafilatura_rs::{trafilatura, trafilatura2};
use tree::{Input, SameThread};

create_exception!(_core, Unsupported, PyException, "The call needs a code path only Python trafilatura has.");

/// The trafilatura release an extraction reproduces.
#[derive(Clone, Copy)]
pub(crate) enum Engine {
    V1_11,
    V2_3,
}

impl Engine {
    pub(crate) fn parse(version: &str) -> PyResult<Engine> {
        match version {
            "1.11.0" => Ok(Engine::V1_11),
            "2.3.1" => Ok(Engine::V2_3),
            other => Err(PyValueError::new_err(format!("no engine for trafilatura {other}"))),
        }
    }

    fn extract(self, html: &str, opts: &Options, req: &Request, lru: &mut Lru) -> Result<Option<String>> {
        match self {
            Engine::V1_11 => trafilatura::core::extract_with(html, opts, req, lru),
            Engine::V2_3 => trafilatura2::core::extract(html, opts, req, lru),
        }
    }

    /// `extract` given an lxml tree, copied as 2.3.1 copies it; 1.11 cleans the caller's tree in place instead.
    fn extract_tree(
        self,
        ctx: &Ctx,
        tree: Node,
        opts: &Options,
        req: &Request,
        lru: &mut Lru,
    ) -> Result<Option<String>> {
        match self {
            Engine::V1_11 => Err(Error::Unsupported("trafilatura 1.11 changes the caller's tree".into())),
            Engine::V2_3 => trafilatura2::core::extract_tree(ctx, tree, opts, req, lru),
        }
    }

    /// The version's `load_html` for str input.
    pub(crate) fn load_html(self, ctx: &Ctx, html: &str) -> Option<Node> {
        match self {
            Engine::V1_11 => load_html(ctx, html),
            Engine::V2_3 => trafilatura2::utils::load_html(ctx, html),
        }
    }
}

pub(crate) fn to_py_err(e: Error) -> PyErr {
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
                "XMLSyntaxError" => xml_syntax_error(msg),
                "XPathSyntaxError" | "XPathEvalError" => {
                    let kind = kind.to_owned();
                    Python::attach(|py| lxml_error(py, &kind, (msg.as_str(),)))
                        .unwrap_or_else(|| Unsupported::new_err(format!("{kind}: {msg}")))
                }
                _ => PyRuntimeError::new_err(m),
            }
        }
    }
}

/// lxml.etree.XMLSyntaxError from "code\x1fline\x1fcolumn\x1fmessage", or Unsupported (rerun in Python) without lxml.
fn xml_syntax_error(msg: String) -> PyErr {
    let mut parts = msg.splitn(4, '\x1f');
    let mut num = || parts.next().and_then(|n| n.parse::<i64>().ok()).unwrap_or(0);
    let (code, line, column) = (num(), num(), num());
    let text = msg.rsplit('\x1f').next().unwrap_or_default().to_owned();
    Python::attach(|py| lxml_error(py, "XMLSyntaxError", (text.as_str(), code, line, column, "<string>")))
        .unwrap_or_else(|| Unsupported::new_err(format!("XMLSyntaxError: {msg}")))
}

/// lxml.etree's exception `class` built from `args`; None without lxml.
fn lxml_error<'py>(py: Python<'py>, class: &str, args: impl pyo3::call::PyCallArgs<'py>) -> Option<PyErr> {
    let class = py.import("lxml.etree").and_then(|m| m.getattr(class)).ok()?;
    class.call1(args).ok().map(PyErr::from_value)
}

fn format(output_format: &str) -> PyResult<Format> {
    match output_format {
        "txt" => Ok(Format::Txt),
        "markdown" => Ok(Format::Markdown),
        "json" => Ok(Format::Json),
        "xml" => Ok(Format::Xml),
        "html" => Ok(Format::Html),
        "csv" => Ok(Format::Csv),
        other => Err(PyValueError::new_err(format!("output_format {other:?} is not ported"))),
    }
}

/// The engine and its options; `no_fallback` is 1.11's name for 2.x's `fast`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn options(
    engine: &str,
    favor_precision: bool,
    favor_recall: bool,
    include_tables: bool,
    deduplicate: bool,
    no_fallback: bool,
    include_comments: bool,
    output_format: &str,
    with_metadata: bool,
) -> PyResult<(Engine, Options)> {
    let engine = Engine::parse(engine)?;
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
    }
    .with_format(format(output_format)?);
    Ok((engine, opts))
}

pub(crate) fn request<'a>(url: Option<&'a str>, record_id: Option<&'a str>, max_date: Option<&'a str>) -> Request<'a> {
    Request { url, record_id, max_date, hooks: &PyHooks }
}

/// `trafilatura.extract` of trafilatura `engine` (trafilatura's defaults) with the process-wide dedup state, on a
/// str or (2.3.1) an lxml tree, see tree.rs. `max_date` is today's local date as htmldate's `set_date_params` formats it (needed with metadata); `extra`
/// sets the remaining options (see extra.rs). Raises `Unsupported` where only Python trafilatura can answer.
#[pyfunction]
#[pyo3(signature = (html, engine, favor_precision=false, favor_recall=false, include_tables=true, deduplicate=false,
    no_fallback=false, include_comments=true, output_format="txt", with_metadata=false, url=None, record_id=None,
    max_date=None, extra=None))]
#[allow(clippy::too_many_arguments)]
fn extract(
    py: Python<'_>,
    html: &Bound<'_, PyAny>,
    engine: &str,
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
    extra: Option<&Bound<'_, PyDict>>,
) -> PyResult<Option<String>> {
    let (engine, mut opts) = options(
        engine,
        favor_precision,
        favor_recall,
        include_tables,
        deduplicate,
        no_fallback,
        include_comments,
        output_format,
        with_metadata,
    )?;
    if let Some(extra) = extra {
        extra::apply(&mut opts, extra)?;
    }
    let req = request(url, record_id, max_date);
    let ctx = Ctx::new();
    let result = match Input::new(&ctx, html)? {
        Input::Html(html) => py.detach(|| dedup::run(deduplicate, |lru| engine.extract(html, &opts, &req, lru))),
        Input::Tree(tree) => {
            let job = SameThread((&ctx, tree));
            py.detach(|| {
                let (ctx, tree) = job.get();
                dedup::run(deduplicate, |lru| engine.extract_tree(ctx, tree, &opts, &req, lru))
            })
        }
    };
    result.map_err(to_py_err)
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

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(_find_date, m)?)?;
    m.add_function(wrap_pyfunction!(dedup::reset_caches, m)?)?;
    m.add_function(wrap_pyfunction!(dedup::_dedup_undo, m)?)?;
    m.add_function(wrap_pyfunction!(dedup::_dedup_commit, m)?)?;
    m.add_function(wrap_pyfunction!(dedup::_dedup_get, m)?)?;
    m.add_function(wrap_pyfunction!(dedup::_dedup_put, m)?)?;
    m.add_function(wrap_pyfunction!(dedup::_dedup_increment, m)?)?;
    m.add_function(wrap_pyfunction!(funcs::html2txt, m)?)?;
    m.add_function(wrap_pyfunction!(funcs::baseline, m)?)?;
    m.add_function(wrap_pyfunction!(funcs::extract_metadata, m)?)?;
    m.add_function(wrap_pyfunction!(funcs::find_date, m)?)?;
    m.add_function(wrap_pyfunction!(bare::bare_extraction, m)?)?;
    m.add_function(wrap_pyfunction!(dates::_native_dateutil, m)?)?;
    m.add_function(wrap_pyfunction!(dates::_native_external_date, m)?)?;
    m.add_function(wrap_pyfunction!(dates::_native_dateparser, m)?)?;
    m.add_function(wrap_pyfunction!(langid::_native_language, m)?)?;
    m.add_class::<extractor::Extractor>()?;
    m.add("Unsupported", m.py().get_type::<Unsupported>())?;
    Ok(())
}
