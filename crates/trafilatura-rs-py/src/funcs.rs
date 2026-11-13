//! `html2txt`, `baseline`, `extract_metadata` and htmldate's `find_date` on str input, for trafilatura `engine`.
//! Argument checks, object construction and the fallback live in python/trafilatura_rs/_funcs.py.
use crate::convert::{self, Fields};
use crate::hooks::PyHooks;
use crate::{request, to_py_err, Engine};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use trafilatura_rs::dom::Ctx;
use trafilatura_rs::htmldate::{self, DateParams};
use trafilatura_rs::trafilatura::settings::Options;
use trafilatura_rs::{trafilatura, trafilatura2};

#[pyfunction]
#[pyo3(signature = (html, engine, clean=true))]
pub fn html2txt(py: Python<'_>, html: &str, engine: &str, clean: bool) -> PyResult<String> {
    let engine = Engine::parse(engine)?;
    py.detach(|| {
        let ctx = Ctx::new();
        let Some(tree) = engine.load_html(&ctx, html) else { return Ok(String::new()) };
        match engine {
            Engine::V1_11 => trafilatura::baseline::html2txt(&ctx, tree, clean),
            Engine::V2_3 => trafilatura2::baseline::html2txt(&ctx, tree, clean, false),
        }
    })
    .map_err(to_py_err)
}

/// `baseline(html)` as (the text of each `<p>` of the returned body, the text).
#[pyfunction]
pub fn baseline(py: Python<'_>, html: &str, engine: &str) -> PyResult<(Vec<String>, String)> {
    let engine = Engine::parse(engine)?;
    py.detach(|| {
        let ctx = Ctx::new();
        let Some(tree) = engine.load_html(&ctx, html) else { return Ok((Vec::new(), String::new())) };
        let (body, text, _) = match engine {
            Engine::V1_11 => trafilatura::baseline::baseline(&ctx, tree)?,
            Engine::V2_3 => trafilatura2::baseline::baseline(&ctx, tree)?,
        };
        Ok((body.children().into_iter().map(|p| p.text().unwrap_or_default()).collect(), text))
    })
    .map_err(to_py_err)
}

/// `extract_metadata(html, default_url, date_config, author_blacklist=...)` as Document slots, None where
/// `load_html` fails. `author_blacklist` is lowercased already.
#[pyfunction]
#[pyo3(signature = (html, engine, max_date, default_url=None, original_date=true, extensive_search=true,
    author_blacklist=Vec::new()))]
#[allow(clippy::too_many_arguments)]
pub fn extract_metadata<'py>(
    py: Python<'py>,
    html: &str,
    engine: &str,
    max_date: &str,
    default_url: Option<&str>,
    original_date: bool,
    extensive_search: bool,
    author_blacklist: Vec<String>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let engine = Engine::parse(engine)?;
    let opts =
        Options { author_blacklist, original_date, extensive_date_search: extensive_search, ..Options::default() };
    let req = request(default_url, None, Some(max_date));
    let fields: Option<Fields> = py
        .detach(|| {
            let ctx = Ctx::new();
            let Some(tree) = engine.load_html(&ctx, html) else { return Ok(None) };
            let doc = match engine {
                Engine::V1_11 => trafilatura::metadata::extract_metadata(&ctx, tree, &req, &opts)?,
                Engine::V2_3 => trafilatura2::metadata::extract_metadata(&ctx, tree, &req, &opts)?,
            };
            Ok(Some(convert::metadata_fields(&doc)))
        })
        .map_err(to_py_err)?;
    fields.map(|f| convert::dict(py, &f)).transpose()
}

/// htmldate's `find_date(html, extensive_search, original_date, url=url, max_date=max_date)` with its own
/// `load_html` (str that is not a URL); `max_date=None` is htmldate's default, the end of today.
#[pyfunction]
#[pyo3(signature = (html, extensive_search=true, original_date=false, url=None, max_date=None))]
pub fn find_date(
    py: Python<'_>,
    html: &str,
    extensive_search: bool,
    original_date: bool,
    url: Option<&str>,
    max_date: Option<&str>,
) -> PyResult<Option<String>> {
    let params = DateParams { original_date, extensive_search, max_date: max_date.unwrap_or_default().to_owned() };
    py.detach(|| {
        let ctx = Ctx::new();
        let Some(tree) = htmldate::load_html(&ctx, html) else { return Ok(None) };
        htmldate::find_date(&ctx, tree, url, &params, &PyHooks)
    })
    .map_err(to_py_err)
}
