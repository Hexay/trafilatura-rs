//! trafilatura 2.3.1 core.py: `trafilatura_sequence`, `bare_extraction` and `extract` for txt, markdown and json
//! output (no language checks).
use super::baseline::{baseline, html2txt_len};
use super::deduplication::content_fingerprint;
use super::external::{compare_extraction, justext_rescue};
use super::htmlprocessing::{prune_unwanted_nodes, tree_cleaning};
use super::htmlprocessing_convert::convert_tags;
use super::main_extractor::elem_text;
use super::main_extractor_comments::extract_comments;
use super::main_extractor_extract::{extract_content, Extraction};
use super::metadata::extract_metadata;
use super::output::determine_returnstring;
use super::settings::{Focus, Options};
use super::utils::load_html;
use super::xml::delete_element;
use super::xpaths::{RAW_TREE_PRUNE_XPATH, REMOVE_COMMENTS_AND_LISTS_XPATH, REMOVE_COMMENTS_XPATH};
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;
use crate::py::value::Value;
pub use crate::trafilatura::core::Request;
use crate::trafilatura::deduplication::{duplicate_test, Lru};
use crate::trafilatura::document::Document;
use crate::trafilatura::filters;
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::links::Urls;
use crate::trafilatura::main_extractor_extract::body_text;
use crate::trafilatura::settings::Format;

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
        extract(html, &self.opts, &Request::default(), &mut self.lru)
    }
}

/// `trafilatura.extract(html, url=..., record_id=..., ...)`: `Ok(None)` where Python returns None, `Err` where it
/// raises (2.x no longer catches RuntimeError).
pub fn extract(html: &str, opts: &Options, req: &Request, lru: &mut Lru) -> Result<Option<String>> {
    let ctx = Ctx::new();
    let mut document = match bare_extraction(&ctx, html, opts, req, lru) {
        Err(Error::Value(_) | Error::Type(_)) => return Ok(None),
        other => other?,
    };
    // txt formats only output the id and fingerprint in their metadata header
    if !matches!(opts.format, Format::Txt | Format::Markdown) || opts.with_metadata {
        document.id = req.record_id.map_or(Value::None, Value::str);
        let content = format!("{} {}", document.title.to_py_str(), document.raw_text.to_py_str());
        document.fingerprint = Value::Str(content_fingerprint(&content));
    }
    determine_returnstring(&ctx, &document, opts).map(Some)
}

/// `bare_extraction` up to the TypeError/ValueError catch.
pub fn bare_extraction(ctx: &Ctx, html: &str, opts: &Options, req: &Request, lru: &mut Lru) -> Result<Document> {
    let tree = load_html(ctx, html).ok_or_else(|| Error::Value("empty HTML tree".into()))?;
    let mut document = Document::default();
    if opts.with_metadata {
        document = extract_metadata(ctx, tree, req, opts)?;
        filters::check_metadata(&document, opts)?;
    }
    filters::prune_xpath(tree, opts, |node| delete_element(ctx, node, true))?;
    let doc_url = document.url.as_str().map(str::to_owned);
    let urls =
        Urls { hooks: req.hooks, page: req.url, convert: req.url.filter(|u| !u.is_empty()).or(doc_url.as_deref()) };
    let mut env = Env { ctx, opts, lru, urls };
    let ((postbody, temp_text), (commentsbody, temp_comments)) = trafilatura_sequence(&mut env, tree)?;
    if let Some(max) = opts.max_tree_size {
        if postbody.len() > max {
            postbody.strip_tags(ctx, &["hi"]);
        }
        if postbody.len() > max {
            return Err(Error::Value("output tree too long".into()));
        }
    }
    if py::len(&temp_text) < opts.min_output_size && py::len(&temp_comments) < opts.min_output_comm_size {
        return Err(Error::Value("text and comments not long enough".into()));
    }
    if opts.dedup && duplicate_test(postbody, opts, env.lru) {
        return Err(Error::Value("duplicate document".into()));
    }
    document.raw_text = Value::Str(temp_text);
    document.body = Some(postbody);
    document.commentsbody = Some(commentsbody);
    Ok(document)
}

/// `_forum_thread_page`: a JSON-LD script declaring a DiscussionForumPosting.
pub fn forum_thread_page(tree: Node) -> bool {
    tree.findall(r#".//script[@type="application/ld+json"]"#)
        .into_iter()
        .any(|s| s.text().is_some_and(|t| py::re::search(DISCUSSION_FORUM_POSTING, false, &t)))
}

/// The raw-tree prunes at the start of `trafilatura_sequence` (in place, as in Python).
pub fn prune_raw_tree(env: &Env, tree: Node, is_forum: bool) -> Result<Node> {
    let tree = prune_unwanted_nodes(env.ctx, tree, &RAW_TREE_PRUNE_XPATH, false)?;
    if !env.opts.comments && (env.opts.focus == Focus::Precision || !is_forum) {
        return prune_unwanted_nodes(env.ctx, tree, &REMOVE_COMMENTS_AND_LISTS_XPATH, false);
    }
    Ok(tree)
}

/// `_prepare_tree`: (converted, pre-conversion backup) from a cleaned copy.
pub fn prepare_tree(env: &Env, tree: Node) -> Result<(Node, Node)> {
    let cleaned = tree_cleaning(env.ctx, tree.deep_copy(env.ctx), env.opts)?;
    let backup = cleaned.deep_copy(env.ctx);
    Ok((convert_tags(env.ctx, cleaned, env.opts, env.urls.hooks, env.urls.convert)?, backup))
}

/// `_extract_and_compare`: cascade stages 1-2.
pub fn extract_and_compare(env: &mut Env, cleaned: Node, backup: Node, tree: Node) -> Result<Extraction> {
    let result = extract_content(env, cleaned)?;
    if env.opts.fast {
        return Ok(result);
    }
    compare_extraction(env, backup, tree.deep_copy(env.ctx), result)
}

/// `trafilatura_sequence`: the body and the comments body, each with its text.
pub fn trafilatura_sequence(env: &mut Env, tree: Node) -> Result<(Extraction, Extraction)> {
    let (ctx, opts) = (env.ctx, env.opts);
    let is_forum = forum_thread_page(tree);
    let tree = prune_raw_tree(env, tree, is_forum)?;
    let (mut cleaned, backup) = prepare_tree(env, tree)?;
    let mut comments = (ctx.element("body")?, String::new());
    let mut forum_posts = None;
    if opts.comments {
        let (c_body, c_text) = extract_comments(env, cleaned)?;
        if !c_text.is_empty() && is_forum {
            // thread forum: the "comments" are the posts, routed into the body (salvaged below if dropped)
            forum_posts = Some(c_body);
            cleaned = convert_tags(ctx, backup.deep_copy(ctx), opts, env.urls.hooks, env.urls.convert)?;
        } else {
            comments = (c_body, c_text);
        }
    }
    if opts.focus == Focus::Precision && !is_forum {
        cleaned = prune_unwanted_nodes(ctx, cleaned, &REMOVE_COMMENTS_XPATH, false)?;
    }
    let mut result = extract_and_compare(env, cleaned, backup, tree)?;
    if py::len(&result.1) < opts.min_extracted_size && opts.focus != Focus::Precision {
        let rescued = baseline_rescue(ctx, tree, result)?;
        forum_posts = forum_posts.filter(|_| !rescued.1);
        result = rescued.0;
    }
    // short-circuit as in Python: html2txt only runs once the cheaper gates pass
    let len_text = py::len(&result.1);
    let due = opts.focus == Focus::Balanced && 0 < len_text && len_text < ESCALATION_MAX_LENGTH;
    if due && escalation_due(opts, len_text, html2txt_len(tree)?) {
        let esc_tree = escalation_tree(env, tree, is_forum, !comments.1.is_empty())?;
        if let Some(better) = escalate(env, esc_tree, len_text)? {
            result = better;
            forum_posts = None;
        }
    }
    if let Some(posts) = forum_posts {
        result = salvage_forum_posts(result, posts)?;
    }
    Ok((result, comments))
}

/// Stage 3: the baseline on the original tree, accepted only if it adds text; (result, accepted).
pub fn baseline_rescue(ctx: &Ctx, tree: Node, result: Extraction) -> Result<(Extraction, bool)> {
    let (b_body, b_text, b_len) = baseline(ctx, tree)?;
    Ok(if b_len > py::len(&result.1) { ((b_body, b_text), true) } else { (result, false) })
}

/// Stage-4 gate: a balanced extraction that is short and covers little of the page text.
pub fn escalation_due(opts: &Options, len_text: usize, page_len: usize) -> bool {
    opts.focus == Focus::Balanced
        && 0 < len_text
        && len_text < ESCALATION_MAX_LENGTH
        && (len_text as f64) < ESCALATION_PAGE_SHARE * page_len as f64
}

/// The escalation input: the tree itself on a thread forum or with comments off (already pruned), else a copy
/// without comment containers (and comment lists, if comments were captured).
pub fn escalation_tree(env: &Env, tree: Node, is_forum: bool, captured: bool) -> Result<Node> {
    if is_forum || !env.opts.comments {
        return Ok(tree);
    }
    let xpath: &[&str] = if captured { &REMOVE_COMMENTS_AND_LISTS_XPATH } else { &REMOVE_COMMENTS_XPATH };
    prune_unwanted_nodes(env.ctx, tree.deep_copy(env.ctx), xpath, false)
}

/// Stage 4, recall escalation on `esc_tree`: the recall retry or the jusText candidate when clearly longer.
pub fn escalate(env: &mut Env, esc_tree: Node, len_text: usize) -> Result<Option<Extraction>> {
    let (ctx, opts) = (env.ctx, env.opts);
    let r_opts = Options { focus: Focus::Recall, ..opts.clone() };
    let mut r_env = Env { ctx, opts: &r_opts, lru: &mut *env.lru, urls: env.urls };
    // Python catches any Exception from either candidate.
    let retry =
        caught(prepare_tree(&r_env, esc_tree).and_then(|(c, b)| extract_and_compare(&mut r_env, c, b, esc_tree)))?;
    let jt = if opts.fast { None } else { caught(justext_rescue(ctx, esc_tree.deep_copy(ctx)))? };
    let r_len = retry.as_ref().map_or(0, |r| py::len(&r.1));
    let j_len = jt.as_ref().map_or(0, |j| py::len(&j.1));
    if j_len > r_len && j_len as f64 > ESCALATION_JUSTEXT_RATIO * len_text as f64 {
        return Ok(jt);
    }
    if r_len >= opts.min_extracted_size && r_len as f64 > ESCALATION_ACCEPT_RATIO * len_text as f64 {
        return Ok(retry);
    }
    Ok(None)
}

/// `try: ... except Exception`; Unsupported is the port's, not Python's, so it still escapes.
fn caught<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Err(e @ Error::Unsupported(_)) => Err(e),
        r => Ok(r.ok()),
    }
}

/// Thread-forum salvage: append the captured posts missing from the body.
fn salvage_forum_posts(result: Extraction, posts: Node) -> Result<Extraction> {
    let body = result.0;
    let texts: Vec<String> = body.children().into_iter().map(elem_text).filter(|t| !t.is_empty()).collect();
    let existing = texts.join("\n");
    let salvaged: Vec<Node> = posts
        .children()
        .into_iter()
        .filter(|&el| {
            let t = elem_text(el);
            !t.is_empty() && !existing.contains(&t)
        })
        .collect();
    if salvaged.is_empty() {
        return Ok(result);
    }
    body.extend(&salvaged)?;
    Ok((body, body_text(body)))
}
