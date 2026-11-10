//! trafilatura/external.py: readability/jusText fallbacks and the choice between them and the own extraction.
use super::htmlprocessing::{prune_unwanted_nodes, tree_cleaning, Env};
use super::htmlprocessing_convert::convert_tags;
use super::settings::{Focus, Options};
use super::text::trim;
use super::xpaths::{OVERALL_DISCARD_XPATH, PAYWALL_DISCARD_XPATH, REMOVE_COMMENTS_XPATH};
use crate::dom::{Ctx, Error, Method, Node, Result};
use crate::justext::{custom_justext, Stoplist};
use crate::py;
use crate::readability::try_readability;

pub const SANITIZED_XPATH: &str = ".//aside|.//audio|.//button|.//fieldset|.//figure|.//footer|.//iframe|.//input|\
    .//label|.//link|.//nav|.//noindex|.//noscript|.//object|.//option|.//select|.//source|.//svg|.//time";

const TEI_VALID_TAGS: &[&str] =
    &["ab", "body", "cell", "code", "del", "div", "graphic", "head", "hi", "item", "lb", "list", "p", "quote", "ref", "row", "table"];

pub type Extraction = (Node, String, usize);

fn joined_text(n: Node) -> String {
    trim(&n.itertext(true).join(" "))
}

/// `try_justext` with target_language None (the full JT_STOPLIST).
fn try_justext(ctx: &Ctx, tree: Node) -> Result<Option<Node>> {
    let body = ctx.element("body")?;
    let paragraphs = match custom_justext(tree, Stoplist::jt()) {
        Ok(p) => p,
        Err(Error::Value(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    for p in paragraphs.iter().filter(|p| !p.is_boilerplate()) {
        let el = ctx.element("p")?;
        el.set_text(Some(&p.text))?;
        body.append(el)?;
    }
    Ok(Some(body))
}

/// `justext_rescue(tree, options, postbody, 0, '')`: returns the jusText result when it has any text.
fn justext_rescue(ctx: &Ctx, tree: Node) -> Result<Option<Extraction>> {
    let tree = prune_unwanted_nodes(ctx, tree, &PAYWALL_DISCARD_XPATH, false)?;
    let tree = prune_unwanted_nodes(ctx, tree, &REMOVE_COMMENTS_XPATH, false)?;
    Ok(try_justext(ctx, tree)?.and_then(|body| {
        let text = joined_text(body);
        let n = py::len(&text);
        (n > 0).then_some((body, text, n))
    }))
}

/// `sanitize_tree`. Python's `tree.findall(SANITIZED_XPATH)` is an ElementPath lookup of a `|` union, which lxml
/// silently answers with `[]`, so that removal loop never runs.
fn sanitize_tree(ctx: &Ctx, tree: Node, opts: &Options) -> Result<Extraction> {
    let cleaned = tree_cleaning(ctx, tree, opts)?;
    if !opts.links {
        cleaned.strip_tags(ctx, &["a"]);
    }
    cleaned.strip_tags(ctx, &["span"]);
    let cleaned = convert_tags(ctx, cleaned, opts)?;
    for el in cleaned.iter(&["td", "th", "tr"]) {
        match el.tag() {
            "tr" => el.set_tag("row")?,
            "th" => {
                el.set("role", Some("head"))?;
                el.set_tag("cell")?;
            }
            _ => el.set_tag("cell")?,
        }
    }
    let invalid: Vec<&str> = cleaned.iter(&["*"]).map(|e| e.tag()).filter(|t| !TEI_VALID_TAGS.contains(t)).collect();
    cleaned.strip_tags(ctx, &invalid);
    let text = joined_text(cleaned);
    let n = py::len(&text);
    Ok((cleaned, text, n))
}

/// `compare_extraction(tree, backup_tree, body, text, len_text, options)`.
pub fn compare_extraction(env: &mut Env, tree: Node, backup_tree: Node, own: Extraction) -> Result<Extraction> {
    let (ctx, opts) = (env.ctx, env.opts);
    let (mut body, mut text, mut len_text) = own;
    if opts.focus == Focus::Recall && len_text > opts.min_extracted_size * 10 {
        return Ok((body, text, len_text));
    }
    let mut backup_tree = prune_unwanted_nodes(ctx, backup_tree, &PAYWALL_DISCARD_XPATH, false)?;
    if opts.focus == Focus::Precision {
        backup_tree = prune_unwanted_nodes(ctx, backup_tree, &OVERALL_DISCARD_XPATH, false)?;
    }
    let algo = try_readability(ctx, backup_tree)
        .ok_or_else(|| Error::Type("Type 'NoneType' cannot be serialized.".into()))?;
    let algo_text = trim(&algo.tostring(Method::Text, true));
    let len_algo = py::len(&algo_text);
    let min2 = opts.min_extracted_size * 2;
    let use_readability = if len_algo == 0 || len_algo == len_text {
        false
    } else if len_text == 0 {
        true
    } else if len_text > 2 * len_algo {
        false
    } else if len_algo > 2 * len_text && !algo_text.starts_with('{') {
        true
    } else if body.xpath_strings(".//p//text()")?.is_empty() && len_algo > min2 {
        true
    } else if body.findall(".//table").len() > body.findall(".//p").len() && len_algo > min2 {
        true
    } else {
        opts.focus == Focus::Recall
            && body.xpath_nodes(".//head")?.is_empty()
            && !algo.xpath_nodes(".//h2|.//h3|.//h4")?.is_empty()
            && len_algo > len_text
    };
    if use_readability {
        (body, text, len_text) = (algo, algo_text, len_algo);
    }
    let mut jt_result = false;
    if !body.xpath_nodes(SANITIZED_XPATH)?.is_empty() || len_text < opts.min_extracted_size {
        if let Some((body2, text2, len2)) = justext_rescue(ctx, tree)? {
            jt_result = true;
            if len_text <= 4 * len2 {
                (body, text, len_text) = (body2, text2, len2);
            }
        }
    }
    if use_readability && !jt_result {
        return sanitize_tree(ctx, body, opts);
    }
    Ok((body, text, len_text))
}
