//! trafilatura/core.py: `bare_extraction` and `extract` for txt output (no metadata or language checks).
use super::baseline::baseline;
use super::deduplication::{duplicate_test, Lru};
use super::external::compare_extraction;
use super::htmlprocessing::{prune_unwanted_nodes, tree_cleaning, Env};
use super::htmlprocessing_convert::convert_tags;
use super::main_extractor_comments::extract_comments;
use super::main_extractor_extract::extract_content;
use super::settings::{Focus, Options};
use super::text::normalize_unicode;
use super::utils::load_html;
use super::xml::xmltotxt;
use super::xpaths::REMOVE_COMMENTS_XPATH;
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;

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
}

/// `trafilatura.extract(html, ...)` with txt output: `Ok(None)` where Python returns None, `Err` where Python
/// raises. `lru` is the deduplication state Python keeps module-global.
pub fn extract(html: &str, opts: &Options, lru: &mut Lru) -> Result<Option<String>> {
    let ctx = Ctx::new();
    let doc = match bare_extraction(&ctx, html, opts, lru) {
        Err(Error::Runtime(msg)) if is_runtime_error(&msg) => return Ok(None),
        other => other?,
    };
    let Some((body, comments)) = doc else { return Ok(None) };
    let text = xmltotxt(body, opts.formatting)?;
    let Some(comments) = comments else { return Ok(Some(normalize_unicode(&text))) };
    let comments = xmltotxt(comments, opts.formatting)?;
    Ok(Some(normalize_unicode(py::strip(&format!("{text}\n{comments}")))))
}

/// Extracted body and, with include_comments, the comments body.
type Document = (Node, Option<Node>);

/// `bare_extraction`: the document, or None where Python catches TypeError/ValueError.
fn bare_extraction(ctx: &Ctx, html: &str, opts: &Options, lru: &mut Lru) -> Result<Option<Document>> {
    match extract_document(ctx, html, opts, lru) {
        Err(Error::Value(_) | Error::Type(_)) => Ok(None),
        other => other.map(Some),
    }
}

fn extract_document(ctx: &Ctx, html: &str, opts: &Options, lru: &mut Lru) -> Result<Document> {
    let tree = load_html(ctx, html).ok_or_else(|| value_error("empty HTML tree"))?;
    let tree_backup = tree.deep_copy(ctx);
    let cleaned = tree_cleaning(ctx, tree, opts)?;
    let cleaned_backup = cleaned.deep_copy(ctx);
    let mut cleaned = convert_tags(ctx, cleaned, opts)?;
    let mut env = Env { ctx, opts, lru };
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
    let (postbody, _, len_text) = result;
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
    Ok((postbody, comments))
}

/// `extract` catches RuntimeError around bare_extraction, which includes RecursionError and NotImplementedError.
fn is_runtime_error(msg: &str) -> bool {
    ["RuntimeError", "RecursionError", "NotImplementedError"].iter().any(|n| msg.starts_with(n))
}
