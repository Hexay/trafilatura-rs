//! xml.py `replace_element_text` and the helpers it shares with `process_element` (list markers, links, images).
use super::settings::{INLINE_CONSUMING, INLINE_FORMATTABLE};
use super::utils::{is_element_in_item, is_in_table_cell, item_if_first_element};
use super::xml_markdown::format_markdown;
use crate::dom::{Ctx, Node, Result};
use crate::py;

/// `_consumes_inline_children`.
pub fn consumes_inline_children(el: Node) -> bool {
    INLINE_CONSUMING.contains(&el.tag()) && !el.is_empty()
}

/// `_escape_cell`.
pub fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// `_heading_prefix`: `#` times the rend level digit, default 2.
pub fn heading_prefix(el: Node) -> String {
    let level = el.get("rend").unwrap_or_default();
    let digit = level.chars().nth(1).filter(|c| ('1'..='6').contains(c));
    "#".repeat(digit.map_or(2, |d| d as usize - '0' as usize))
}

/// `_md_link`.
pub fn md_link(text: &str, url: Option<&str>, image: bool) -> String {
    let esc = text.replace('[', "\\[").replace(']', "\\]");
    let prefix = if image { "!" } else { "" };
    let Some(url) = url else { return format!("{prefix}[{esc}]") };
    let safe = if url.chars().any(|c| " <>()".contains(c)) {
        format!("<{}>", url.replace('\\', "\\\\").replace('<', "\\<").replace('>', "\\>"))
    } else {
        url.to_owned()
    };
    format!("{prefix}[{esc}]({safe})")
}

/// `_image_markup`.
pub fn image_markup(el: Node) -> String {
    let alt = format!("{} {}", el.get("title").unwrap_or_default(), el.get("alt").unwrap_or_default());
    md_link(py::strip(&alt), Some(&el.get("src").unwrap_or_default()), true)
}

/// `_list_marker`: "- " (or "N. " for ordered lists in markdown) with nesting indent, for an item's first content.
pub fn list_marker(el: Node, in_item: Option<bool>, fmt: bool) -> String {
    if !in_item.unwrap_or_else(|| is_element_in_item(el)) {
        return String::new();
    }
    let Some(item) = item_if_first_element(el) else { return String::new() };
    if is_in_table_cell(el) {
        return String::new();
    }
    let depth = item.iter_ancestors(&["list"]).count();
    let indent = "  ".repeat(depth.saturating_sub(1));
    if fmt && item.parent().is_some_and(|p| p.get("rend").as_deref() == Some("ol")) {
        let n = item.iter_siblings(&["item"], true).count() + 1;
        return format!("{indent}{n}. ");
    }
    format!("{indent}- ")
}

/// `_collect_inline_text`.
fn collect_inline_text(ctx: &Ctx, el: Node, fmt: bool) -> Result<String> {
    let mut parts: Vec<String> = el.text().filter(|t| !t.is_empty()).into_iter().collect();
    for child in el.children() {
        let tag = child.tag();
        if tag == "graphic" {
            parts.push(image_markup(child));
        } else if tag == "lb" {
            parts.push("\n".into());
        } else if INLINE_FORMATTABLE.contains(&tag) {
            parts.push(replace_element_text(ctx, child, fmt, None, false)?);
        } else if let Some(t) = child.text().filter(|t| !t.is_empty()) {
            parts.push(t);
        }
        if let Some(t) = child.tail().filter(|t| !t.is_empty()) {
            parts.push(t);
        }
    }
    Ok(parts.concat())
}

/// `replace_element_text`: the element's own rendering (the tail is handled by the caller).
pub fn replace_element_text(ctx: &Ctx, el: Node, fmt: bool, in_item: Option<bool>, in_cell: bool) -> Result<String> {
    let tag = el.tag();
    let mut text =
        if consumes_inline_children(el) { collect_inline_text(ctx, el, fmt)? } else { el.text().unwrap_or_default() };
    if fmt && !text.is_empty() {
        text = format_markdown(ctx, el, text, in_cell)?;
    }
    if tag == "ref" {
        let stripped = py::strip(&text).to_owned();
        if !stripped.is_empty() {
            let target = el.get("target").filter(|t| !t.is_empty());
            text = text.replacen(&stripped, &md_link(&stripped, target.as_deref(), false), 1);
        }
    }
    if tag == "cell" {
        text = py::strip(&text).to_owned();
        if !text.is_empty() && !el.is_empty() {
            text.push(' ');
        }
    }
    text = format!("{}{text}", list_marker(el, in_item, fmt));
    Ok(if in_cell { escape_cell(&text) } else { text })
}
