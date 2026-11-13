//! trafilatura/core.py: `bare_extraction` and `extract` for txt, markdown and json output (no language checks).
use super::baseline::baseline;
use super::deduplication::{content_fingerprint, duplicate_test, Lru};
use super::document::Document;
use super::external::compare_extraction;
use super::filters;
use super::htmlprocessing::{prune_node, prune_unwanted_nodes, tree_cleaning, Env};
use super::htmlprocessing_convert::convert_tags;
use super::links::Urls;
use super::main_extractor_comments::extract_comments;
use super::main_extractor_extract::extract_content;
use super::metadata::extract_metadata;
use super::output::determine_returnstring;
use super::settings::{Focus, Format, Options};
use super::utils::load_html;
use super::xpaths::REMOVE_COMMENTS_XPATH;
use crate::dom::{Ctx, Error, Result};
use crate::hooks::{Hooks, NoHooks};
use crate::py::value::Value;

/// Per-call `extract()` arguments besides the HTML and the options.
#[derive(Clone, Copy)]
pub struct Request<'a> {
    pub url: Option<&'a str>,
    pub record_id: Option<&'a str>,
    /// htmldate's `max_date` as `set_date_params` formats today ("YYYY-MM-DD"); required with metadata.
    pub max_date: Option<&'a str>,
    pub hooks: &'a dyn Hooks,
}

impl Default for Request<'_> {
    fn default() -> Self {
        Request { url: None, record_id: None, max_date: None, hooks: &NoHooks }
    }
}

/// Extraction state that trafilatura keeps per process: the options and the deduplication LRU, which makes
/// results depend on the order documents are processed in (as in datatrove's per-worker Python).
pub struct Extractor {
    pub opts: Options,
    lru: Lru,
}

fn value_error(what: &str) -> Error {
    Error::Value(what.into())
}

impl Extractor {
    pub fn new(opts: Options) -> Extractor {
        Extractor { opts, lru: Lru::default() }
    }

    pub fn extract(&mut self, html: &str) -> Result<Option<String>> {
        extract(html, &self.opts, &mut self.lru)
    }

    pub fn extract_with(&mut self, html: &str, req: &Request) -> Result<Option<String>> {
        extract_with(html, &self.opts, req, &mut self.lru)
    }
}

/// `trafilatura.extract(html, ...)` without per-call arguments.
pub fn extract(html: &str, opts: &Options, lru: &mut Lru) -> Result<Option<String>> {
    extract_with(html, opts, &Request::default(), lru)
}

/// `trafilatura.extract(html, url=..., record_id=..., ...)`: `Ok(None)` where Python returns None, `Err` where
/// Python raises. `lru` is the deduplication state Python keeps module-global.
pub fn extract_with(html: &str, opts: &Options, req: &Request, lru: &mut Lru) -> Result<Option<String>> {
    let ctx = Ctx::new();
    let document = match bare_extraction(&ctx, html, opts, req, lru) {
        Err(Error::Runtime(msg)) if is_runtime_error(&msg) => return Ok(None),
        other => other?,
    };
    let Some(mut document) = document else { return Ok(None) };
    if !matches!(opts.format, Format::Txt | Format::Markdown) {
        document.id = req.record_id.map_or(Value::None, Value::str);
        let content = format!("{} {}", document.title.to_py_str(), document.raw_text.to_py_str());
        document.fingerprint = Value::Str(content_fingerprint(&content));
    }
    determine_returnstring(&ctx, &document, opts).map(Some)
}

/// `bare_extraction`: the document, or None where Python catches TypeError/ValueError.
fn bare_extraction(ctx: &Ctx, html: &str, opts: &Options, req: &Request, lru: &mut Lru) -> Result<Option<Document>> {
    match extract_document(ctx, html, opts, req, lru) {
        Err(Error::Value(_) | Error::Type(_)) => Ok(None),
        other => other.map(Some),
    }
}

/// The ValueError for a page `load_html` rejects.
pub const EMPTY_TREE: &str = "empty HTML tree";

/// `bare_extraction` up to the TypeError/ValueError catch.
pub fn extract_document(ctx: &Ctx, html: &str, opts: &Options, req: &Request, lru: &mut Lru) -> Result<Document> {
    let tree = load_html(ctx, html).ok_or_else(|| value_error(EMPTY_TREE))?;
    let mut document = Document::default();
    if opts.with_metadata {
        document = extract_metadata(ctx, tree, req, opts)?;
        filters::check_metadata(&document, opts)?;
    }
    filters::prune_xpath(tree, opts, |node| prune_node(ctx, node))?;
    let tree_backup = tree.deep_copy(ctx);
    let cleaned = tree_cleaning(ctx, tree, opts)?;
    let cleaned_backup = cleaned.deep_copy(ctx);
    let doc_url = document.url.as_str().map(str::to_owned);
    let urls =
        Urls { hooks: req.hooks, page: req.url, convert: req.url.filter(|u| !u.is_empty()).or(doc_url.as_deref()) };
    let mut cleaned = convert_tags(ctx, cleaned, opts, urls.hooks, urls.convert)?;
    let mut env = Env { ctx, opts, lru, urls };
    let (comments, len_comments) = if opts.comments {
        let (body, _, n) = extract_comments(&mut env, cleaned)?;
        (Some(body), n)
    } else {
        (None, 0)
    };
    if opts.focus == Focus::Precision {
        cleaned = prune_unwanted_nodes(ctx, cleaned, &REMOVE_COMMENTS_XPATH, false)?;
    }
    let mut result = extract_content(&mut env, cleaned)?;
    if !opts.fast {
        result = compare_extraction(&mut env, cleaned_backup, tree_backup.deep_copy(ctx), result)?;
    }
    if result.2 < opts.min_extracted_size {
        result = baseline(ctx, tree_backup)?;
    }
    let (postbody, temp_text, len_text) = result;
    if let Some(max) = opts.max_tree_size {
        if postbody.len() > max {
            postbody.strip_tags(ctx, &["hi"]);
        }
        if postbody.len() > max {
            return Err(value_error("output tree too long"));
        }
    }
    if len_text < opts.min_output_size && len_comments < opts.min_output_comm_size {
        return Err(value_error("text and comments not long enough"));
    }
    if opts.dedup && duplicate_test(postbody, opts, env.lru) {
        return Err(value_error("duplicate document"));
    }
    document.raw_text = Value::Str(temp_text);
    document.body = Some(postbody);
    document.commentsbody = comments;
    Ok(document)
}

/// `extract` catches RuntimeError around bare_extraction, which includes RecursionError and NotImplementedError.
fn is_runtime_error(msg: &str) -> bool {
    ["RuntimeError", "RecursionError", "NotImplementedError"].iter().any(|n| msg.starts_with(n))
}
