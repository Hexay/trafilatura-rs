//! `bare_extraction` as Document slots with `body`/`commentsbody` serialized; python/trafilatura_rs/_bare.py builds
//! the Document (or dict) and the lxml trees from them.
use crate::convert::{self, Fields};
use crate::{extra, options, request, to_py_err, Engine};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use trafilatura_rs::dom::{Ctx, Error, Node, Result};
use trafilatura_rs::py::value::Value;
use trafilatura_rs::trafilatura::core::{Request, EMPTY_TREE};
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::document::Document;
use trafilatura_rs::trafilatura::settings::Options;
use trafilatura_rs::{trafilatura, trafilatura2};

fn document(engine: Engine, ctx: &Ctx, html: &str, opts: &Options, req: &Request) -> Result<Option<Document>> {
    let mut lru = Lru::default();
    let result = match engine {
        Engine::V1_11 => trafilatura::core::extract_document(ctx, html, opts, req, &mut lru),
        Engine::V2_3 => trafilatura2::core::bare_extraction(ctx, html, opts, req, &mut lru),
    };
    match result {
        // 1.11 reads `options.source` while discarding it: AttributeError unless `options` was passed
        Err(Error::Value(m)) if matches!(engine, Engine::V1_11) && m == EMPTY_TREE => Err(Error::Unsupported(m)),
        Err(Error::Value(_) | Error::Type(_)) => Ok(None),
        other => other.map(Some),
    }
}

fn xmltotxt(engine: Engine, ctx: &Ctx, node: Node, formatting: bool) -> Result<String> {
    match engine {
        Engine::V1_11 => trafilatura::xml::xmltotxt(node, formatting),
        Engine::V2_3 => trafilatura2::xml::xmltotxt(ctx, node, formatting),
    }
}

/// The slots bare_extraction sets after extraction: `text`/`comments` for output_format="python", else the
/// extraction's raw text.
fn fields(engine: Engine, ctx: &Ctx, doc: &Document, opts: &Options, python_format: bool) -> Result<Fields> {
    let mut fields = convert::metadata_fields(doc);
    let (mut text, mut comments, mut commentsbody) = (Value::None, Value::None, convert::tree(doc.commentsbody)?);
    if python_format {
        let body_text = Value::Str(xmltotxt(engine, ctx, doc.body.expect("extracted body"), opts.formatting)?);
        fields.retain(|(k, _)| *k != "raw_text");
        fields.push(("raw_text", body_text.clone()));
        text = body_text;
        match doc.commentsbody.filter(|_| opts.comments) {
            Some(c) => comments = Value::Str(xmltotxt(engine, ctx, c, opts.formatting)?),
            None => commentsbody = Value::None,
        }
    }
    fields.extend([
        ("text", text),
        ("comments", comments),
        ("body", convert::tree(doc.body)?),
        ("commentsbody", commentsbody),
    ]);
    Ok(fields)
}

/// `bare_extraction(html, ...)` without deduplication, keywords as in `extract`; None where Python returns None.
#[pyfunction]
#[pyo3(signature = (html, engine, python_format, favor_precision=false, favor_recall=false, include_tables=true,
    no_fallback=false, include_comments=true, output_format="txt", with_metadata=false, url=None, max_date=None,
    extra=None))]
#[allow(clippy::too_many_arguments)]
pub fn bare_extraction<'py>(
    py: Python<'py>,
    html: &str,
    engine: &str,
    python_format: bool,
    favor_precision: bool,
    favor_recall: bool,
    include_tables: bool,
    no_fallback: bool,
    include_comments: bool,
    output_format: &str,
    with_metadata: bool,
    url: Option<&str>,
    max_date: Option<&str>,
    extra: Option<&Bound<'py, PyDict>>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let (engine, mut opts) = options(
        engine,
        favor_precision,
        favor_recall,
        include_tables,
        false,
        no_fallback,
        include_comments,
        output_format,
        with_metadata,
    )?;
    if let Some(extra) = extra {
        extra::apply(&mut opts, extra)?;
    }
    let req = request(url, None, max_date);
    let fields: Option<Fields> = py
        .detach(|| {
            let ctx = Ctx::new();
            match document(engine, &ctx, html, &opts, &req)? {
                Some(doc) => fields(engine, &ctx, &doc, &opts, python_format).map(Some),
                None => Ok(None),
            }
        })
        .map_err(to_py_err)?;
    fields.map(|f| convert::dict(py, &f)).transpose()
}
