//! trafilatura 2.2.0 external.py: readability/jusText fallbacks and the choice between them and the own extraction.
use super::baseline::basic_cleaning;
use super::htmlprocessing::{prune_unwanted_nodes, tree_cleaning};
use super::htmlprocessing_convert::convert_tags;
use super::settings::{Focus, Options};
use super::xpaths::{OVERALL_DISCARD_XPATH, SANITIZED_XPATH};
use crate::dom::{html_element, Ctx, Method, Node, Result};
use crate::justext::{classify_paragraphs, make_paragraphs, revise_paragraph_classification, Params, Stoplist};
use crate::py;
use crate::readability::Document;
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::text::trim;
use crate::trafilatura::utils::fromstring_bytes;
use std::collections::HashSet;

const TEI_VALID_TAGS: &[&str] = &[
    "ab", "body", "cell", "code", "del", "div", "graphic", "head", "hi", "item", "lb", "list", "p", "quote", "ref",
    "row", "table",
];
/// Adopt justext only when the text it replaces is at most this much longer.
const JUSTEXT_OVERRIDE_RATIO: usize = 3;

pub type Extraction = (Node, String, usize);

fn joined_text(n: Node) -> String {
    trim(&n.itertext(true).join(" "))
}

/// `try_readability`: the summary re-parsed as bytes, or an empty `HtmlElement()` when anything fails.
fn try_readability(ctx: &Ctx, tree: Node) -> Result<Node> {
    let summary = Document::new(tree, 25, 250).with_drop_tags(&["script", "style", "fencedframe"]).summary(ctx);
    match summary.ok().and_then(|s| fromstring_bytes(ctx, &s)) {
        Some(n) => Ok(n),
        None => html_element(ctx, "HtmlElement"),
    }
}

/// `_prefer_readability`.
fn prefer_readability(
    body: Node,
    algo: Node,
    algo_text: &str,
    len_text: usize,
    len_algo: usize,
    opts: &Options,
) -> Result<bool> {
    if len_algo == 0 || len_algo == len_text || len_text > 2 * len_algo {
        return Ok(false);
    }
    let recall = opts.focus == Focus::Recall;
    Ok(len_text == 0
        || (len_algo > 2 * len_text && !algo_text.starts_with('{'))
        || (len_algo > opts.min_extracted_size * 2
            && (body.xpath_strings(".//p//text()")?.is_empty()
                || body.findall(".//table").len() > body.findall(".//p").len()))
        || (recall && len_algo as f64 > 1.5 * len_text as f64 && !algo_text.starts_with('{'))
        || (recall
            && body.xpath_nodes(".//head")?.is_empty()
            && !algo.xpath_nodes(".//h2|.//h3|.//h4")?.is_empty()
            && len_algo > len_text))
}

/// `compare_extraction(cleaned_tree, raw_tree, body, text, len_text, options)`.
pub fn compare_extraction(env: &mut Env, cleaned_tree: Node, raw_tree: Node, own: Extraction) -> Result<Extraction> {
    let (ctx, opts) = (env.ctx, env.opts);
    let (mut body, mut text, mut len_text) = own;
    if opts.focus == Focus::Recall && len_text > opts.min_extracted_size * 10 {
        return Ok((body, text, len_text));
    }
    let mut raw_tree = raw_tree;
    if opts.focus == Focus::Precision {
        raw_tree = prune_unwanted_nodes(ctx, raw_tree, &OVERALL_DISCARD_XPATH, false)?;
    }
    let algo = try_readability(ctx, raw_tree)?;
    let algo_text = trim(&algo.tostring(Method::Text, true));
    let len_algo = py::len(&algo_text);
    let use_readability = prefer_readability(body, algo, &algo_text, len_text, len_algo, opts)?;
    if use_readability {
        (body, text, len_text) = (algo, algo_text, len_algo);
    }
    let mut jt_result = false;
    if !body.xpath_nodes(SANITIZED_XPATH)?.is_empty() || len_text < opts.min_extracted_size {
        let (body2, text2, len2) = justext_rescue(ctx, cleaned_tree)?;
        if !text2.is_empty() && len_text <= JUSTEXT_OVERRIDE_RATIO * len2 {
            (body, text, len_text) = (body2, text2, len2);
            jt_result = true;
        }
    }
    if use_readability && !jt_result {
        return sanitize_tree(ctx, body, opts);
    }
    Ok((body, text, len_text))
}

/// `try_justext` with target_language None (the full JT_STOPLIST); any jusText exception gives an empty body.
fn try_justext(ctx: &Ctx, tree: Node) -> Result<Node> {
    let body = ctx.element("body")?;
    let params = Params {
        length_low: 50,
        length_high: 150,
        stopwords_low: 0.1,
        stopwords_high: 0.2,
        max_link_density: 0.25,
        no_headings: true,
    };
    let Ok(mut paragraphs) = make_paragraphs(tree) else { return Ok(body) };
    classify_paragraphs(&mut paragraphs, Stoplist::jt(), &params);
    revise_paragraph_classification(&mut paragraphs, 150);
    for p in paragraphs.iter().filter(|p| !p.is_boilerplate()) {
        let el = ctx.element("p")?;
        el.set_text(Some(&p.text))?;
        body.append(el)?;
    }
    Ok(body)
}

/// `justext_rescue`: jusText on the basic-cleaned tree.
pub fn justext_rescue(ctx: &Ctx, tree: Node) -> Result<Extraction> {
    let tree = basic_cleaning(ctx, tree)?;
    let body = try_justext(ctx, tree)?;
    let text = joined_text(body);
    let n = py::len(&text);
    Ok((body, text, n))
}

/// `sanitize_tree`: readability output converted to trafilatura's vocabulary.
fn sanitize_tree(ctx: &Ctx, tree: Node, opts: &Options) -> Result<Extraction> {
    let cleaned = tree_cleaning(ctx, tree, opts)?;
    if !opts.links {
        cleaned.strip_tags(ctx, &["a"]);
    }
    cleaned.strip_tags(ctx, &["span"]);
    let cleaned = convert_tags(ctx, cleaned, opts)?;
    let mut seen_groups: HashSet<Option<Node>> = HashSet::new();
    for tr in cleaned.iter(&["tr"]) {
        let parent = tr.parent();
        let kids = tr.children();
        if !seen_groups.contains(&parent) && kids.iter().any(|c| c.tag() == "th") {
            seen_groups.insert(parent);
            for c in kids.into_iter().filter(|c| c.tag() == "th") {
                c.set("role", Some("head"))?;
            }
        }
    }
    for el in cleaned.iter(&["td", "th", "tr"]) {
        el.set_tag(if el.tag() == "tr" { "row" } else { "cell" })?;
    }
    let invalid: Vec<&str> = cleaned.iter(&["*"]).map(|e| e.tag()).filter(|t| !TEI_VALID_TAGS.contains(t)).collect();
    cleaned.strip_tags(ctx, &invalid);
    let text = joined_text(cleaned);
    let n = py::len(&text);
    Ok((cleaned, text, n))
}
