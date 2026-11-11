//! trafilatura 2.3.1 baseline.py: `basic_cleaning`, `baseline` (JSON, articles, paragraphs, teasers, body dump)
//! and `html2txt`, for element input (what the pipeline passes; the element is copied, not mutated).
use super::baseline_json::{collect_json_content, render_text, spaced_text, BLOCK_ELEMS};
use super::settings::{DEDUPE_SCAN_CAP, MIN_DUPLICATE_LENGTH};
use super::xml::delete_element;
use super::xpaths::BASIC_CLEAN_XPATH;
use crate::dom::{Ctx, Node, Result};
use crate::py;
use crate::trafilatura::text::{remove_control_characters, trim, without_control_characters};
use std::collections::HashSet;

pub type Extraction = (Node, String, usize);

const MIN_CONTENT_LENGTH: usize = 100;
const SCRAPED_TAGS: &[&str] = &["blockquote", "code", "p", "pre", "q", "quote"];

/// `basic_cleaning`.
pub fn basic_cleaning(ctx: &Ctx, tree: Node) -> Result<Node> {
    for elem in tree.xpath_nodes(BASIC_CLEAN_XPATH)? {
        delete_element(ctx, elem, true)?;
    }
    Ok(tree)
}

/// `block_text`: text_content() with word boundaries kept at block tags.
pub fn block_text(ctx: &Ctx, element: Node) -> Result<String> {
    spaced_text(element.deep_copy(ctx))
}

/// `_build_body`: one paragraph per text; with `dedupe`, long texts already contained are dropped.
fn build_body(ctx: &Ctx, texts: impl IntoIterator<Item = String>, dedupe: bool) -> Result<(Node, String)> {
    let postbody = ctx.element("body")?;
    let mut temp_text = String::new();
    let mut temp_len = 0;
    for text in texts {
        let text = remove_control_characters(&text);
        let len = py::len(&text);
        if !text.is_empty()
            && (!dedupe || len <= MIN_DUPLICATE_LENGTH || temp_len > DEDUPE_SCAN_CAP || !temp_text.contains(&text))
        {
            postbody.sub_element("p")?.set_text(Some(&text))?;
            if !temp_text.is_empty() {
                temp_text.push('\n');
                temp_len += 1;
            }
            temp_text.push_str(&text);
            temp_len += len;
        }
    }
    Ok((postbody, temp_text))
}

/// `_attempt`: the body when it carries more than MIN_CONTENT_LENGTH code points.
fn attempt(ctx: &Ctx, texts: impl IntoIterator<Item = String>, dedupe: bool) -> Result<Option<Extraction>> {
    let (body, text) = build_body(ctx, texts, dedupe)?;
    let n = py::len(&text);
    Ok((n > MIN_CONTENT_LENGTH).then_some((body, text, n)))
}

/// `baseline(tree)`.
pub fn baseline(ctx: &Ctx, tree: Node) -> Result<Extraction> {
    let tree = tree.deep_copy(ctx);
    let (json_bodies, json_teasers) = collect_json_content(tree)?;
    if let Some(r) = attempt(ctx, json_bodies.iter().map(|b| render_text(ctx, b)), true)? {
        return Ok(r);
    }
    let tree = basic_cleaning(ctx, tree)?;
    let mut article_texts = Vec::new();
    for elem in tree.xpath_nodes(".//article[not(ancestor::article)]")? {
        let text = block_text(ctx, elem)?;
        if py::len(&text) > MIN_CONTENT_LENGTH {
            article_texts.push(text);
        }
    }
    if let Some(longest) = article_texts.iter().map(|t| py::len(t)).max() {
        let cutoff = longest as f64 / 5.0;
        let kept = article_texts.into_iter().filter(|t| py::len(t) as f64 >= cutoff);
        if let Some(r) = attempt(ctx, kept, false)? {
            return Ok(r);
        }
    }
    let mut paragraphs = Vec::new();
    for elem in tree.iter(SCRAPED_TAGS) {
        if !elem.iter_ancestors(&[]).any(|a| SCRAPED_TAGS.contains(&a.tag())) {
            paragraphs.push(block_text(ctx, elem)?);
        }
    }
    if let Some(r) = attempt(ctx, paragraphs, true)? {
        return Ok(r);
    }
    let teaser = attempt(ctx, json_teasers.iter().map(|t| render_text(ctx, t)), true)?;
    let postbody = ctx.element("body")?;
    if let Some(body_elem) = tree.find(".//body") {
        let p = postbody.sub_element("p")?;
        let joined = body_elem.itertext(true).iter().map(|e| trim(e)).filter(|t| !t.is_empty()).collect::<Vec<_>>();
        p.set_text(Some(&remove_control_characters(&joined.join("\n"))))?;
        let text = p.text().unwrap_or_default();
        let n = py::len(&text);
        if teaser.as_ref().is_none_or(|t| n >= t.2) {
            return Ok((postbody, text, n));
        }
    }
    Ok(teaser.unwrap_or((postbody, String::new(), 0)))
}

/// `html2txt(tree)` (clean=True): whitespace-normalized text of the body with block boundaries spaced.
pub fn html2txt(ctx: &Ctx, tree: Node) -> Result<String> {
    let tree = tree.deep_copy(ctx);
    let body = tree.find(".//body").unwrap_or(tree);
    spaced_text(basic_cleaning(ctx, body)?)
}

/// `len(html2txt(tree))`, read from `tree` in place: `basic_cleaning` as skipped subtrees, `spaced_text`'s block
/// spacing applied per text slot.
pub fn html2txt_len(tree: Node) -> Result<usize> {
    let body = tree.find(".//body").unwrap_or(tree);
    let removed: HashSet<Node> = body.xpath_nodes(BASIC_CLEAN_XPATH)?.into_iter().collect();
    let mut len = py::TrimLen::default();
    body.text_slots(&removed, &mut |owner, pieces| {
        if BLOCK_ELEMS.contains(&owner.tag()) {
            len.feed(" ");
            pieces.iter().for_each(|p| len.feed(&without_control_characters(p)));
        } else {
            pieces.iter().for_each(|p| len.feed(p));
        }
    });
    Ok(len.finish())
}
