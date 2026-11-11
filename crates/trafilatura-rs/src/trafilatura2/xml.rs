//! trafilatura 2.3.1 xml.py: `delete_element`, `separates_inline` and the plain-text serializer `xmltotxt` /
//! `process_element` (element text rendering lives in xml_text). Markdown output (include_formatting) is not ported.
use super::settings::INLINE_CARRIED;
use super::xml_text::{
    append_spaced, append_tail, closes_container, consumes_inline_children, escape_cell, image_markup,
    item_needs_marker, last_char, list_marker, opens_item, replace_element_text,
};
use crate::dom::{Ctx, Node, Result};
use crate::trafilatura::text::{sanitize, text_chars_test};
use crate::trafilatura::xml_unescape::unescape;

pub(super) const NEWLINE_ELEMS: &[&str] = &["graphic", "head", "lb", "list", "p", "quote", "row", "table"];
const SPECIAL_FORMATTING: &[&str] = &["code", "del", "head", "hi", "ref", "item", "cell"];
pub(super) const SEPARATORS: &[&str] = &[" ", "\n", "|", ""];

/// `delete_element`: remove with children; with `keep_tail` the tail joins the previous element or parent text.
pub fn delete_element(ctx: &Ctx, element: Node, keep_tail: bool) -> Result<()> {
    let Some(parent) = element.parent() else { return Ok(()) };
    if keep_tail {
        if let Some(tail) = element.tail().filter(|t| !t.is_empty()) {
            match element.prev() {
                None => parent.set_text(Some(&(parent.text().unwrap_or_default() + &tail)))?,
                Some(prev) => prev.set_tail(Some(&(prev.tail().unwrap_or_default() + &tail)))?,
            }
        }
    }
    parent.remove(ctx, element)
}

/// `separates_inline`: a line break before inline content.
pub fn separates_inline(el: Node) -> bool {
    el.tag() == "lb" && el.next().is_some_and(|n| INLINE_CARRIED.contains(&n.tag()))
}

fn truthy(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !s.is_empty())
}

fn is_space_char(s: &str) -> bool {
    s.chars().next().is_some_and(crate::py::is_space)
}

/// `process_element` with include_formatting=False.
pub fn process_element(el: Node, out: &mut Vec<String>, in_cell: bool, in_item: bool) -> Result<()> {
    let tag = el.tag();
    let in_cell = in_cell || tag == "cell";
    let in_item = in_item || tag == "item";
    let newline_elem = NEWLINE_ELEMS.contains(&tag);
    if tag == "cell" && el.prev().is_none() {
        out.push("| ".into());
    }
    let last = last_char(out);
    if newline_elem && !in_cell && !in_item && !SEPARATORS.contains(&last) {
        out.push("\n".into());
    } else if matches!(tag, "head" | "p" | "quote" | "table")
        && !is_space_char(last)
        && !SEPARATORS.contains(&last)
        && (truthy(&el.text()) || el.is_empty() || el.child(0).is_some_and(|c| c.tag() != "lb"))
    {
        out.push(" ".into());
    }
    let consumes = consumes_inline_children(el);
    let renders_inline = truthy(&el.text()) || consumes;
    if tag == "item" && !in_cell && item_needs_marker(el) {
        out.push(list_marker(el));
    }
    if renders_inline && (tag != "item" || consumes || text_chars_test(el.text().as_deref())) {
        out.push(escape_cell(&replace_element_text(el, in_cell), in_cell));
    }
    if tag == "list" && in_item && !matches!(last_char(out), "\n" | "") {
        out.push("\n".into());
    }
    if !consumes {
        for child in el.children() {
            process_element(child, out, in_cell, in_item)?;
        }
    }
    if !renders_inline {
        if tag == "graphic" {
            out.push(escape_cell(&image_markup(el), in_cell));
            if let Some(t) = el.tail().filter(|t| !t.is_empty()) {
                out.push(escape_cell(&format!(" {}", crate::py::strip(&t)), in_cell));
            }
        } else if newline_elem {
            if tag == "row" {
                let cells = el.findall("cell");
                if cells.iter().any(|c| c.get("role").as_deref() == Some("head")) {
                    out.push(format!("\n|{}\n", "---|".repeat(cells.len())));
                }
            } else if !in_cell && !(tag == "lb" && in_item && opens_item(el)) {
                out.push("\n".into());
            }
        } else if tag != "cell" && tag != "item" {
            if in_cell {
                append_tail(el, out, in_cell, in_item);
            }
            return Ok(());
        }
    }
    let last_in_item = in_item && closes_container(el, "item");
    let last_in_cell = in_cell && closes_container(el, "cell");
    if newline_elem && !in_cell && !in_item {
        out.push("\n".into());
    } else if tag == "cell" {
        out.push(" | ".into());
    } else if (tag == "head" || tag == "item") && in_cell && !last_in_cell {
        append_spaced(out, " ");
    } else if !SPECIAL_FORMATTING.contains(&tag) && !last_in_item && !last_in_cell {
        append_spaced(out, " ");
    }
    if tag != "graphic" {
        append_tail(el, out, in_cell, in_item);
    }
    if last_in_item && !in_cell {
        out.push("\n".into());
    }
    Ok(())
}

/// `xmltotxt(xmloutput, include_formatting=False)`.
pub fn xmltotxt(xmloutput: Node) -> Result<String> {
    let mut out = Vec::new();
    process_element(xmloutput, &mut out, false, false)?;
    Ok(unescape(&sanitize(&out.concat(), true, false).unwrap_or_default()))
}
