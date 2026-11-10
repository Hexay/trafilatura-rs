//! trafilatura/main_extractor.py: the `handle_*` family (lists and tables live in main_extractor_lists/_table, the
//! extraction driver in main_extractor_extract).
use super::htmlprocessing::{handle_textnode, process_node, truthy, Env};
pub use super::main_extractor_lists::handle_lists;
use super::main_extractor_table::handle_table;
use super::text::text_chars_test;
use crate::dom::{Error, Node, Result};
use std::collections::HashSet;

pub type PotentialTags = HashSet<&'static str>;

pub const TAG_CATALOG: &[&str] = &["blockquote", "code", "del", "head", "hi", "lb", "list", "p", "pre", "quote"];
const P_FORMATTING: &[&str] = &["hi", "ref"];
const FORMATTING: &[&str] = &["hi", "ref", "span"];
const CODES_QUOTES: &[&str] = &["code", "quote"];
const FORMATTING_PROTECTED: &[&str] = &["cell", "head", "hi", "item", "p", "quote", "ref", "td"];

/// `lxml` child iteration (`for x in el`): the next sibling is fixed when the current child is handed out.
fn iter_children(el: Node) -> impl Iterator<Item = Node> {
    el.child(0).into_iter().flat_map(|first| std::iter::once(first).chain(first.iter_siblings(&[], false)))
}

fn joined_text(el: Node) -> String {
    el.itertext(true).concat()
}

/// `is_text_element`.
pub(crate) fn is_text_element(el: Node) -> bool {
    text_chars_test(Some(&joined_text(el)))
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
    Ok(title.filter(|&t| text_chars_test(Some(&joined_text(t)))))
}

/// `handle_formatting`.
pub fn handle_formatting(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let formatting = process_node(env, element)?;
    if element.is_empty() && formatting.is_none() {
        return Ok(None);
    }
    let parent = element.parent().or_else(|| element.prev());
    if parent.is_none_or(|p| !FORMATTING_PROTECTED.contains(&p.tag())) {
        let processed = env.ctx.element("p")?;
        let formatting = formatting.ok_or_else(|| {
            Error::Type("Argument 'element' has incorrect type (expected lxml.etree._Element, got NoneType)".into())
        })?;
        processed.insert(0, formatting)?;
        return Ok(Some(processed));
    }
    Ok(formatting)
}

/// `update_elem_rendition`.
pub(crate) fn update_elem_rendition(elem: Node, new_elem: Node) -> Result<()> {
    if let Some(rend) = elem.get("rend") {
        new_elem.set("rend", Some(&rend))?;
    }
    Ok(())
}

/// `define_newelem`.
pub fn define_newelem(processed: Option<Node>, orig: Node) -> Result<()> {
    if let Some(p) = processed {
        let child = orig.sub_element(p.tag())?;
        child.set_text(p.text().as_deref())?;
        child.set_tail(p.tail().as_deref())?;
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
    element.find("code").is_some() && element.len() == 1
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
    for child in element.iter(&["*"]) {
        let processed = process_node(env, child)?;
        define_newelem(processed, processed_element)?;
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
fn handle_paragraphs(env: &mut Env, element: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    element.clear_attrs();
    if element.is_empty() {
        return process_node(env, element);
    }
    let processed_element = env.ctx.element(element.tag())?;
    for child in element.iter(&["*"]) {
        if !potential_tags.contains(child.tag()) && child.tag() != "done" {
            continue;
        }
        if let Some(processed_child) = handle_textnode(env, child, false, true)? {
            if processed_child.tag() == "p" {
                let ptext = processed_element.text().filter(|t| !t.is_empty());
                let ctext = processed_child.text();
                let text = match (ptext, ctext) {
                    (Some(p), Some(c)) => Some(format!("{p} {c}")),
                    (Some(_), None) => {
                        return Err(Error::Type("can only concatenate str (not \"NoneType\") to str".into()))
                    }
                    (None, c) => c,
                };
                processed_element.set_text(text.as_deref())?;
                child.set_tag("done")?;
                continue;
            }
            let newsub = env.ctx.element(child.tag())?;
            if P_FORMATTING.contains(&processed_child.tag()) {
                if !processed_child.is_empty() {
                    for item in iter_children(processed_child) {
                        if let Some(t) = item.text().filter(|t| text_chars_test(Some(t))) {
                            item.set_text(Some(&format!(" {t}")))?;
                        }
                        processed_child.strip_tags(env.ctx, &[item.tag()]);
                    }
                }
                if child.tag() == "hi" {
                    newsub.set("rend", child.get("rend").as_deref())?;
                } else if child.tag() == "ref" {
                    if let Some(target) = child.get("target") {
                        newsub.set("target", Some(&target))?;
                    }
                }
            }
            newsub.set_text(processed_child.text().as_deref())?;
            newsub.set_tail(processed_child.tail().as_deref())?;
            processed_element.append(newsub)?;
        }
        child.set_tag("done")?;
    }
    if let Some(last) = processed_element.child(-1) {
        if last.tag() == "lb" && last.tail().is_none() {
            processed_element.remove(env.ctx, last)?;
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
        // TODO: handle_image (only reachable with images=True).
        "graphic" if potential_tags.contains("graphic") => unimplemented!("handle_image"),
        _ => handle_other_elements(env, element, potential_tags),
    }
}
