//! trafilatura 2.2.0 core.py: `trafilatura_sequence`, `bare_extraction` and `extract` for txt output (no
//! metadata, comments or language checks).
use super::baseline::{baseline, html2txt};
use super::external::{compare_extraction, justext_rescue, Extraction};
use super::htmlprocessing::{prune_unwanted_nodes, tree_cleaning};
use super::htmlprocessing_convert::convert_tags;
use super::main_extractor_extract::extract_content;
use super::settings::{Focus, Options};
use super::utils::load_html;
use super::xml::xmltotxt;
use super::xpaths::REMOVE_COMMENTS_XPATH;
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;
use crate::trafilatura::deduplication::{duplicate_test, Lru};
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::text::normalize_unicode;

const ESCALATION_MAX_LENGTH: usize = 3000;
const ESCALATION_PAGE_SHARE: f64 = 0.2;
const ESCALATION_ACCEPT_RATIO: f64 = 1.5;
const ESCALATION_JUSTEXT_RATIO: f64 = 2.0;
const DISCUSSION_FORUM_POSTING: &str =
    r#""@type"\s*:\s*"DiscussionForumPosting"|"@type"\s*:\s*\[[^\]]*"DiscussionForumPosting""#;

/// Per-process extraction state, as in `crate::trafilatura::core::Extractor`.
pub struct Extractor {
    pub opts: Options,
    lru: Lru,
}

impl Extractor {
    pub fn new(opts: Options) -> Extractor {
        Extractor { opts, lru: Lru::default() }
    }

    pub fn extract(&mut self, html: &str) -> Result<Option<String>> {
        extract(html, &self.opts, &mut self.lru)
    }
}

/// `trafilatura.extract(html, ...)` with txt output: `Ok(None)` where Python returns None, `Err` where it raises
/// (2.2.0 no longer catches RuntimeError).
pub fn extract(html: &str, opts: &Options, lru: &mut Lru) -> Result<Option<String>> {
    assert!(!opts.comments, "include_comments=True (extract_comments) is not ported yet");
    let ctx = Ctx::new();
    let body = match bare_extraction(&ctx, html, opts, lru) {
        Err(Error::Value(_) | Error::Type(_)) => return Ok(None),
        other => other?,
    };
    let text = xmltotxt(&ctx, body, opts.formatting)?;
    let comments = xmltotxt(&ctx, ctx.element("body")?, opts.formatting)?;
    Ok(Some(normalize_unicode(py::strip(&format!("{text}\n{comments}")))))
}

/// `bare_extraction` up to the TypeError/ValueError catch.
fn bare_extraction(ctx: &Ctx, html: &str, opts: &Options, lru: &mut Lru) -> Result<Node> {
    let tree = load_html(ctx, html).ok_or_else(|| Error::Value("empty HTML tree".into()))?;
    let mut env = Env { ctx, opts, lru };
    let (postbody, _, len_text) = trafilatura_sequence(&mut env, tree)?;
    if let Some(max) = opts.max_tree_size {
        if postbody.len() > max {
            postbody.strip_tags(ctx, &["hi"]);
        }
        if postbody.len() > max {
            return Err(Error::Value("output tree too long".into()));
        }
    }
    // len_comments is 0 without comment extraction.
    if len_text < opts.min_output_size && 0 < opts.min_output_comm_size {
        return Err(Error::Value("text and comments not long enough".into()));
    }
    if opts.dedup && duplicate_test(postbody, opts, env.lru) {
        return Err(Error::Value("duplicate document".into()));
    }
    Ok(postbody)
}

/// `_forum_thread_page`: a JSON-LD script declaring a DiscussionForumPosting.
pub fn forum_thread_page(tree: Node) -> bool {
    tree.findall(r#".//script[@type="application/ld+json"]"#)
        .into_iter()
        .any(|s| s.text().is_some_and(|t| py::re::search(DISCUSSION_FORUM_POSTING, false, &t)))
}

/// `_prepare_tree`: (converted, pre-conversion backup) from a cleaned copy.
pub fn prepare_tree(env: &Env, tree: Node) -> Result<(Node, Node)> {
    let cleaned = tree_cleaning(env.ctx, tree.deep_copy(env.ctx), env.opts)?;
    let backup = cleaned.deep_copy(env.ctx);
    Ok((convert_tags(env.ctx, cleaned, env.opts)?, backup))
}

/// `_recall_retry`: cascade stages 1-2 in recall mode.
fn recall_retry(env: &mut Env, esc_tree: Node) -> Result<Extraction> {
    let (cleaned, backup) = prepare_tree(env, esc_tree)?;
    let mut result = extract_content(env, cleaned)?;
    if !env.opts.fast {
        result = compare_extraction(env, backup, esc_tree.deep_copy(env.ctx), result)?;
    }
    Ok(result)
}

/// `trafilatura_sequence` for include_comments=False: the body triple.
pub fn trafilatura_sequence(env: &mut Env, tree: Node) -> Result<Extraction> {
    let (ctx, opts) = (env.ctx, env.opts);
    let is_forum = forum_thread_page(tree);
    // Python prunes copies here and in `escalate`; the source trees are dead afterwards, so pruning in place is equal.
    let mut tree = tree;
    if opts.focus == Focus::Precision || !is_forum {
        tree = prune_unwanted_nodes(ctx, tree, &REMOVE_COMMENTS_XPATH, false)?;
    }
    let (mut cleaned, backup) = prepare_tree(env, tree)?;
    if opts.focus == Focus::Precision && !is_forum {
        cleaned = prune_unwanted_nodes(ctx, cleaned, &REMOVE_COMMENTS_XPATH, false)?;
    }
    let mut result = extract_content(env, cleaned)?;
    if !opts.fast {
        result = compare_extraction(env, backup, tree.deep_copy(ctx), result)?;
    }
    if result.2 < opts.min_extracted_size && opts.focus != Focus::Precision {
        result = baseline(ctx, tree)?;
    }
    // short-circuit as in Python: html2txt only runs once the cheaper gates pass
    let due = opts.focus == Focus::Balanced && 0 < result.2 && result.2 < ESCALATION_MAX_LENGTH;
    if due && escalation_due(opts, result.2, py::len(&html2txt(ctx, tree)?)) {
        if let Some(better) = escalate(env, tree, is_forum, result.2)? {
            result = better;
        }
    }
    Ok(result)
}

/// Stage-4 gate: a balanced extraction that is short and covers little of the page text.
pub fn escalation_due(opts: &Options, len_text: usize, page_len: usize) -> bool {
    opts.focus == Focus::Balanced
        && 0 < len_text
        && len_text < ESCALATION_MAX_LENGTH
        && (len_text as f64) < ESCALATION_PAGE_SHARE * page_len as f64
}

/// Stage 4, recall escalation: the recall retry or the jusText candidate when clearly longer.
pub fn escalate(env: &mut Env, tree: Node, is_forum: bool, len_text: usize) -> Result<Option<Extraction>> {
    let (ctx, opts) = (env.ctx, env.opts);
    let r_opts = Options { focus: Focus::Recall, ..opts.clone() };
    let esc_tree =
        if is_forum { tree } else { prune_unwanted_nodes(ctx, tree, &REMOVE_COMMENTS_XPATH, false)? };
    let mut r_env = Env { ctx, opts: &r_opts, lru: &mut *env.lru };
    // Python catches any Exception from either candidate.
    let retry = recall_retry(&mut r_env, esc_tree).ok();
    let jt = if opts.fast { None } else { justext_rescue(ctx, esc_tree).ok() };
    let (r_len, j_len) = (retry.as_ref().map_or(0, |r| r.2), jt.as_ref().map_or(0, |j| j.2));
    if j_len > r_len && j_len as f64 > ESCALATION_JUSTEXT_RATIO * len_text as f64 {
        return Ok(jt);
    }
    if r_len >= opts.min_extracted_size && r_len as f64 > ESCALATION_ACCEPT_RATIO * len_text as f64 {
        return Ok(retry);
    }
    Ok(None)
}
