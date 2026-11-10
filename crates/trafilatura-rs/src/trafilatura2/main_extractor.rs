//! trafilatura 2.2.0 main_extractor.py: the `handle_*` family (lists, tables/images and the extraction driver
//! live in main_extractor_lists, main_extractor_table and main_extractor_extract).
use super::htmlprocessing::{handle_textnode, process_node};
pub use super::main_extractor_lists::handle_lists;
use super::main_extractor_table::{handle_image, handle_table};
use super::settings::{INLINE_CARRIED, TAG_CATALOG};
use super::xml::delete_element;
use crate::dom::{Node, Result};
use crate::trafilatura::htmlprocessing::{truthy, Env};
use crate::trafilatura::text::{text_chars_test, trim};
use std::collections::HashSet;

pub type PotentialTags = HashSet<&'static str>;

const P_FORMATTING: &[&str] = &["hi", "ref"];
const FORMATTING: &[&str] = &["hi", "ref", "del", "span"];
const CODES_QUOTES: &[&str] = &["code", "quote"];
const FORMATTING_PROTECTED: &[&str] = &["cell", "head", "hi", "item", "p", "quote", "ref", "td"];
/// Internal attributes carried onto a rewired sub-element.
const KEEP_ATTRS: &[&str] = &["rend", "role", "target", "src", "alt", "title"];

/// `lxml` child iteration (`for x in el`): the next sibling is fixed when the current child is handed out.
pub(crate) fn iter_children(el: Node) -> impl Iterator<Item = Node> {
    el.child(0).into_iter().flat_map(|first| std::iter::once(first).chain(first.iter_siblings(&[], false)))
}

/// `_elem_text`: trimmed plain concatenation of the element's text.
pub fn elem_text(el: Node) -> String {
    trim(&el.itertext(true).concat())
}

/// `is_text_element`.
pub(crate) fn is_text_element(el: Node) -> bool {
    text_chars_test(Some(&el.itertext(true).concat()))
}

/// `_wraps_inline`: ref, or a formatting element wrapping inline content.
fn wraps_inline(el: Node) -> bool {
    !el.is_empty() && (el.tag() == "ref" || el.children().iter().any(|c| INLINE_CARRIED.contains(&c.tag())))
}

/// `handle_titles`.
pub fn handle_titles(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let title = if element.is_empty() {
        process_node(env, element)?
    } else {
        let title = element.deep_copy(env.ctx);
        for child in element.children() {
            if let Some(processed) = handle_textnode(env, child, false, false)? {
                title.append(processed)?;
            }
            child.set_tag("done")?;
        }
        Some(title)
    };
    Ok(title.filter(|&t| is_text_element(t)))
}

/// `handle_formatting`.
pub fn handle_formatting(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let Some(formatting) = process_node(env, element)? else { return Ok(None) };
    let parent = element.parent().or_else(|| element.prev());
    if parent.is_none_or(|p| !FORMATTING_PROTECTED.contains(&p.tag())) {
        let processed = env.ctx.element("p")?;
        processed.insert(0, formatting)?;
        return Ok(Some(processed));
    }
    Ok(Some(formatting))
}

/// `update_elem_rendition`: copy a non-empty rend attribute.
pub(crate) fn update_elem_rendition(elem: Node, new_elem: Node) -> Result<()> {
    if let Some(rend) = elem.get("rend").filter(|r| !r.is_empty()) {
        new_elem.set("rend", Some(&rend))?;
    }
    Ok(())
}

/// `define_newelem`: a sub-element copying tag, text, tail and KEEP_ATTRS, optionally with inline children.
pub fn define_newelem(processed: Option<Node>, orig: Node, keep_children: bool) -> Result<()> {
    let Some(p) = processed else { return Ok(()) };
    let child = orig.sub_element(p.tag())?;
    child.set_text(p.text().as_deref())?;
    child.set_tail(p.tail().as_deref())?;
    for (key, value) in p.attrs() {
        if KEEP_ATTRS.contains(&key.as_str()) {
            child.set(&key, Some(&value))?;
        }
    }
    if keep_children {
        for sub in iter_children(p) {
            if INLINE_CARRIED.contains(&sub.tag()) || sub.tag() == "lb" {
                define_newelem(Some(sub), child, true)?;
                for carried in sub.iter(&["*"]) {
                    carried.set_tag("done")?;
                }
            }
        }
    }
    Ok(())
}

/// `is_code_block_element`.
fn is_code_block_element(element: Node) -> bool {
    if element.get("lang").is_some_and(|l| !l.is_empty()) || element.tag() == "code" {
        return true;
    }
    if element.parent().is_some_and(|p| p.get("class").unwrap_or_default().contains("highlight")) {
        return true;
    }
    let blank = |s: Option<String>| crate::py::strip(&s.unwrap_or_default()).is_empty();
    element.find("code").is_some_and(|code| element.len() == 1 && blank(element.text()) && blank(code.tail()))
}

/// `handle_code_blocks`.
fn handle_code_blocks(env: &mut Env, element: Node) -> Result<Node> {
    let processed = element.deep_copy(env.ctx);
    for child in element.iter(&["*"]) {
        child.set_tag("done")?;
    }
    processed.set_tag("code")?;
    Ok(processed)
}

/// `handle_quotes`.
pub fn handle_quotes(env: &mut Env, element: Node) -> Result<Option<Node>> {
    if is_code_block_element(element) {
        return handle_code_blocks(env, element).map(Some);
    }
    let processed_element = env.ctx.element(element.tag())?;
    processed_element.set_text(element.text().as_deref())?;
    let quote_tags: PotentialTags = TAG_CATALOG.iter().copied().chain(["ref", "graphic"]).collect();
    for child in element.iter_descendants(&[]) {
        let tag = child.tag();
        if tag == "graphic" {
            define_newelem(handle_image(env.ctx, Some(child))?, processed_element, false)?;
        } else if tag == "p" && !child.is_empty() {
            if let Some(p) = handle_paragraphs(env, child, &quote_tags)? {
                processed_element.append(p)?;
            }
        } else if INLINE_CARRIED.contains(&tag) {
            define_newelem(Some(child), processed_element, true)?;
        } else {
            let processed = process_node(env, child)?;
            define_newelem(processed, processed_element, false)?;
        }
        child.set_tag("done")?;
    }
    if is_text_element(processed_element) {
        processed_element.strip_tags(env.ctx, &["quote"]);
        return Ok(Some(processed_element));
    }
    Ok(None)
}

/// `handle_other_elements`.
fn handle_other_elements(env: &mut Env, element: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    if element.tag() == "div" && element.get("class").unwrap_or_default().contains("w3-code") {
        return handle_code_blocks(env, element).map(Some);
    }
    if !potential_tags.contains(element.tag()) {
        return Ok(None);
    }
    if element.tag() == "div" {
        if let Some(processed) = handle_textnode(env, element, false, true)? {
            if text_chars_test(processed.text().as_deref()) {
                processed.clear_attrs();
                if processed.tag() == "div" {
                    processed.set_tag("p")?;
                }
                return Ok(Some(processed));
            }
        }
    }
    Ok(None)
}

/// `handle_paragraphs`.
pub fn handle_paragraphs(env: &mut Env, element: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    element.clear_attrs();
    if element.is_empty() {
        return process_node(env, element);
    }
    let processed_element = env.ctx.element(element.tag())?;
    for child in element.iter(&["*"]) {
        if !potential_tags.contains(child.tag()) && child.tag() != "done" {
            continue;
        }
        if let Some(pc) = handle_textnode(env, child, false, true)? {
            if pc.tag() == "p" {
                let text = match processed_element.text().filter(|t| !t.is_empty()) {
                    Some(p) => Some(format!("{p} {}", pc.text().unwrap_or_default())),
                    None => pc.text(),
                };
                processed_element.set_text(text.as_deref())?;
                child.set_tag("done")?;
                continue;
            }
            let mut newsub = env.ctx.element(child.tag())?;
            if P_FORMATTING.contains(&pc.tag()) {
                if wraps_inline(pc) {
                    define_newelem(Some(pc), processed_element, true)?;
                    child.set_tag("done")?;
                    continue;
                }
                if !pc.is_empty() {
                    for item in iter_children(pc) {
                        if let Some(tail) = item.tail().filter(|t| item.tag() == "lb" && !t.is_empty()) {
                            item.set_tail(Some(&format!(" {}", tail.trim_start_matches(crate::py::is_space))))?;
                        } else if let Some(t) = item.text().filter(|t| text_chars_test(Some(t))) {
                            item.set_text(Some(&format!(" {t}")))?;
                        }
                        pc.strip_tags(env.ctx, &[item.tag()]);
                    }
                }
                if child.tag() == "hi" {
                    newsub.set("rend", Some(&child.get("rend").unwrap_or_default()))?;
                } else if child.tag() == "ref" {
                    if let Some(target) = child.get("target") {
                        newsub.set("target", Some(&target))?;
                    }
                }
            }
            newsub.set_text(pc.text().as_deref())?;
            newsub.set_tail(pc.tail().as_deref())?;
            if pc.tag() == "graphic" {
                if let Some(image) = handle_image(env.ctx, Some(pc))? {
                    newsub = image;
                }
            }
            processed_element.append(newsub)?;
        }
        child.set_tag("done")?;
    }
    if let Some(last) = processed_element.child(-1) {
        if last.tag() == "lb" && last.tail().is_none() {
            delete_element(env.ctx, last, true)?;
        }
        return Ok(Some(processed_element));
    }
    Ok(truthy(&processed_element.text()).then_some(processed_element))
}

/// `handle_textelem`.
pub fn handle_textelem(env: &mut Env, element: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    let tag = element.tag();
    match tag {
        "list" => handle_lists(env, element),
        _ if CODES_QUOTES.contains(&tag) => handle_quotes(env, element),
        "head" => handle_titles(env, element),
        "p" => handle_paragraphs(env, element, potential_tags),
        "lb" => {
            if !text_chars_test(element.tail().as_deref()) {
                return Ok(None);
            }
            let Some(element) = process_node(env, element)? else { return Ok(None) };
            let new_element = env.ctx.element("p")?;
            new_element.set_text(element.tail().as_deref())?;
            Ok(Some(new_element))
        }
        _ if FORMATTING.contains(&tag) => handle_formatting(env, element),
        "table" if potential_tags.contains("table") => handle_table(env, element, potential_tags),
        "graphic" if potential_tags.contains("graphic") => handle_image(env.ctx, Some(element)),
        _ => handle_other_elements(env, element, potential_tags),
    }
}
