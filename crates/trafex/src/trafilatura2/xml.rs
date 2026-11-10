//! trafilatura 2.2.0 xml.py: `delete_element` and the text serializer `xmltotxt` / `process_element`
//! (element text rendering lives in xml_text, markdown-only rewrites in xml_markdown).
use super::utils::{is_last_element_in_cell, is_last_element_in_item};
use super::xml_markdown::{collapse_emphasis, convert_math_tree};
use super::xml_text::{
    consumes_inline_children, escape_cell, heading_prefix, image_markup, list_marker, replace_element_text,
};
use crate::dom::{Ctx, Node, Result};
use crate::py;
use crate::trafilatura::text::sanitize;
use crate::trafilatura::xml_unescape::unescape;

const NEWLINE_ELEMS: &[&str] = &["graphic", "head", "lb", "list", "p", "quote", "row", "table"];
const SPECIAL_FORMATTING: &[&str] = &["code", "del", "head", "hi", "ref", "item", "cell"];
const SEPARATORS: &[&str] = &[" ", "\n", "|", ""];

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

/// `_last_char`: the last character of the last emitted string ("" when that string is empty).
fn last_char(out: &[String]) -> &str {
    out.last().and_then(|s| s.char_indices().next_back().map(|(i, _)| &s[i..])).unwrap_or("")
}

fn truthy(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !s.is_empty())
}

/// `process_element`.
pub fn process_element(
    ctx: &Ctx,
    el: Node,
    out: &mut Vec<String>,
    fmt: bool,
    in_cell: bool,
    in_item: bool,
) -> Result<()> {
    let tag = el.tag();
    let in_cell = in_cell || tag == "cell";
    let in_item = in_item || tag == "item";
    let newline_elem = NEWLINE_ELEMS.contains(&tag);
    if tag == "cell" && el.prev().is_none() {
        out.push("| ".into());
    }
    if newline_elem && !in_cell && !in_item && !SEPARATORS.contains(&last_char(out)) {
        out.push("\n".into());
    }
    let consumes = consumes_inline_children(el);
    let renders_inline = truthy(&el.text()) || consumes;
    if renders_inline {
        out.push(replace_element_text(ctx, el, fmt, Some(in_item), in_cell)?);
    } else if fmt && tag == "head" && !in_cell && !el.is_empty() {
        out.push(format!("{} ", heading_prefix(el)));
    }
    let tail = el.tail();
    if in_cell && tag != "graphic" {
        if let Some(t) = tail.as_deref().filter(|t| !t.is_empty()) {
            let mut t = py::strip(t).to_owned();
            if !t.is_empty() && !matches!(last_char(out), " " | "|" | "") {
                t = format!(" {t}");
            }
            out.push(escape_cell(&t));
        }
    }
    if tag == "list" && in_item && !matches!(last_char(out), "\n" | "") {
        out.push("\n".into());
    }
    if !consumes {
        for child in el.children() {
            process_element(ctx, child, out, fmt, in_cell, in_item)?;
        }
    }
    if !renders_inline {
        if tag == "graphic" {
            let image = format!("{}{}", list_marker(el, Some(in_item), fmt), image_markup(el));
            out.push(if in_cell { escape_cell(&image) } else { image });
            if let Some(t) = tail.as_deref().filter(|t| !t.is_empty()) {
                let t = format!(" {}", py::strip(t));
                out.push(if in_cell { escape_cell(&t) } else { t });
            }
        } else if newline_elem {
            if tag == "row" {
                let cells = el.findall("cell");
                if cells.iter().any(|c| c.get("role").as_deref() == Some("head")) {
                    out.push(format!("\n|{}\n", "---|".repeat(cells.len())));
                }
            } else if !in_cell {
                out.push("\n".into());
            }
        } else if tag != "cell" && tag != "item" {
            return Ok(());
        }
    }
    let last_in_item = in_item && is_last_element_in_item(el);
    if newline_elem && !in_cell && !in_item {
        out.push((if fmt && tag != "row" { "\n\u{2424}\n" } else { "\n" }).into());
    } else if tag == "cell" {
        out.push(" | ".into());
    } else if (tag == "head" || tag == "item") && in_cell && !is_last_element_in_cell(el) {
        out.push(" ".into());
    } else if !SPECIAL_FORMATTING.contains(&tag) && !last_in_item && !is_last_element_in_cell(el) {
        out.push(" ".into());
    }
    if let Some(t) = tail.filter(|t| !t.is_empty()) {
        if !in_cell && tag != "graphic" {
            let mut t = if in_item || tag == "list" { py::strip(&t).to_owned() } else { t };
            if !t.is_empty() && in_item && !SEPARATORS.contains(&last_char(out)) {
                t = format!(" {t}");
            }
            out.push(t);
        }
    }
    if last_in_item && !in_cell {
        out.push("\n".into());
    }
    Ok(())
}

/// `xmltotxt`.
pub fn xmltotxt(ctx: &Ctx, xmloutput: Node, fmt: bool) -> Result<String> {
    let mut out = Vec::new();
    let root = if fmt {
        let copy = xmloutput.deep_copy(ctx);
        convert_math_tree(copy)?;
        collapse_emphasis(ctx, copy, &[])?;
        copy
    } else {
        xmloutput
    };
    process_element(ctx, root, &mut out, fmt, false, false)?;
    Ok(unescape(&sanitize(&out.concat(), true, false).unwrap_or_default()))
}
