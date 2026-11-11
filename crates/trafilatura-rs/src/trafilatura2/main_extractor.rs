//! trafilatura 2.3.1 main_extractor.py: the `handle_*` family (lists, tables/images and the extraction driver
//! live in main_extractor_lists, main_extractor_table and main_extractor_extract).
use super::htmlprocessing::{handle_textnode, process_node};
pub use super::main_extractor_lists::{handle_lists, process_nested_elements};
use super::main_extractor_table::{handle_image, handle_table};
use super::settings::{INLINE_CARRIED, INLINE_CONSUMING};
use super::xml::delete_element;
use crate::dom::{Node, Result};
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::text::{text_chars_test, trim};
use std::collections::HashSet;

pub type PotentialTags = HashSet<&'static str>;

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

/// `handle_titles`: a heading with children is copied whole, minus children an earlier pass consumed.
pub fn handle_titles(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let title = if element.is_empty() {
        process_node(env, element)?
    } else {
        let title = element.deep_copy(env.ctx);
        title.strip_elements(env.ctx, &["done"], true);
        for child in element.iter_descendants(&["*"]) {
            child.set_tag("done")?;
        }
        Some(title)
    };
    Ok(title.filter(|&t| is_text_element(t)))
}

/// `handle_formatting`: orphan formatting is wrapped in a paragraph.
pub fn handle_formatting(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let Some(formatting) = process_node(env, element)? else { return Ok(None) };
    if element.parent().is_some_and(|p| FORMATTING_PROTECTED.contains(&p.tag())) {
        return Ok(Some(formatting));
    }
    let wrapper = env.ctx.element("p")?;
    wrapper.append(formatting)?;
    Ok(Some(wrapper))
}

/// `update_elem_rendition`: copy a non-empty rend attribute.
pub(crate) fn update_elem_rendition(elem: Node, new_elem: Node) -> Result<()> {
    if let Some(rend) = elem.get("rend").filter(|r| !r.is_empty()) {
        new_elem.set("rend", Some(&rend))?;
    }
    Ok(())
}

/// `_copy_attrs`: KEEP_ATTRS onto a rewired element.
fn copy_attrs(source: Node, target: Node) -> Result<()> {
    for (key, value) in source.attrs() {
        if KEEP_ATTRS.contains(&key.as_str()) {
            target.set(&key, Some(&value))?;
        }
    }
    Ok(())
}

/// `define_newelem`: a sub-element copying tag, text, tail and KEEP_ATTRS, optionally with inline children.
pub fn define_newelem(processed: Option<Node>, orig: Node, keep_children: bool) -> Result<()> {
    let Some(p) = processed else { return Ok(()) };
    let child = orig.sub_element(p.tag())?;
    child.set_text(p.text().as_deref())?;
    child.set_tail(p.tail().as_deref())?;
    copy_attrs(p, child)?;
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
    process_nested_elements(env, element, processed_element)?;
    if is_text_element(processed_element) {
        processed_element.strip_tags(env.ctx, &["quote"]);
        return Ok(Some(processed_element));
    }
    Ok(None)
}

/// `handle_other_elements`: only a div (when divs are potential tags) becomes a paragraph.
fn handle_other_elements(env: &mut Env, element: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    if element.tag() == "div" && element.get("class").unwrap_or_default().contains("w3-code") {
        return handle_code_blocks(env, element).map(Some);
    }
    if element.tag() != "div" || !potential_tags.contains("div") {
        return Ok(None);
    }
    let Some(processed) = handle_textnode(env, element, false, true)? else { return Ok(None) };
    if !text_chars_test(processed.text().as_deref()) {
        return Ok(None);
    }
    processed.clear_attrs();
    processed.set_tag("p")?;
    Ok(Some(processed))
}

/// `handle_paragraphs`.
pub fn handle_paragraphs(env: &mut Env, element: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    if element.is_empty() {
        return process_node(env, element);
    }
    let processed_element = env.ctx.element(element.tag())?;
    let mut unexpected: Vec<&str> = element
        .iter_descendants(&["*"])
        .map(|c| c.tag())
        .filter(|&t| t != "done" && !potential_tags.contains(t))
        .collect();
    unexpected.sort_unstable();
    unexpected.dedup();
    if !unexpected.is_empty() {
        element.strip_tags(env.ctx, &unexpected);
    }
    for child in element.iter(&["*"]) {
        if let Some(pc) = handle_textnode(env, child, false, true)? {
            if pc.tag() == "p" {
                let joined: Vec<String> =
                    [processed_element.text(), pc.text()].into_iter().flatten().filter(|t| !t.is_empty()).collect();
                processed_element.set_text(Some(&joined.join(" ")))?;
                child.set_tag("done")?;
                continue;
            }
            let mut newsub = env.ctx.element(child.tag())?;
            if INLINE_CONSUMING.contains(&pc.tag()) {
                if wraps_inline(pc) {
                    define_newelem(Some(pc), processed_element, true)?;
                    child.set_tag("done")?;
                    continue;
                }
                let mut tags: Vec<&str> = Vec::new();
                for item in iter_children(pc) {
                    if let Some(tail) = item.tail().filter(|t| item.tag() == "lb" && !t.is_empty()) {
                        item.set_tail(Some(&format!(" {}", tail.trim_start_matches(crate::py::is_space))))?;
                    } else if let Some(t) = item.text().filter(|t| text_chars_test(Some(t))) {
                        item.set_text(Some(&format!(" {t}")))?;
                    }
                    if item.is_element() && !tags.contains(&item.tag()) {
                        tags.push(item.tag());
                    }
                }
                if !tags.is_empty() {
                    pc.strip_tags(env.ctx, &tags);
                }
                copy_attrs(child, newsub)?;
            }
            newsub.set_text(pc.text().as_deref())?;
            newsub.set_tail(pc.tail().as_deref())?;
            if pc.tag() == "graphic" {
                if let Some(image) = handle_image(env.ctx, pc)? {
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
    Ok(text_chars_test(processed_element.text().as_deref()).then_some(processed_element))
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
        _ if INLINE_CONSUMING.contains(&tag) => handle_formatting(env, element),
        "table" if potential_tags.contains("table") => handle_table(env, element, potential_tags),
        "graphic" if potential_tags.contains("graphic") => handle_image(env.ctx, element),
        _ => handle_other_elements(env, element, potential_tags),
    }
}
