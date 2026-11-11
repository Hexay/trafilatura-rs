//! xml.py `replace_element_text` and the helpers it shares with `process_element` (list markers, tails, links,
//! images). `fmt` is include_formatting (markdown).
use super::settings::{INLINE_CARRIED, INLINE_CONSUMING, INLINE_FORMATTABLE};
use super::xml::{NEWLINE_ELEMS, SEPARATORS};
use super::xml_md::{code_fence, escape_block_start, escape_inline_specials, hi_html_tag, hi_marker, md_code, md_wrap};
use crate::dom::{Ctx, Node, Result};
use crate::py;
use crate::trafilatura::text::text_chars_test;
use std::sync::LazyLock;

/// `_UNESCAPED_BRACKET_RE`: code spans (kept) or an unescaped bracket.
static UNESCAPED_BRACKET: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(r"(?s)((?<![\\`])(`+)(?!`).+?(?<!`)\2(?!`))|(?<!\\)([\[\]])").expect("valid pattern")
});

/// `_last_char`: the last character of the last emitted string ("" when that string is empty).
pub fn last_char(out: &[String]) -> &str {
    out.last().and_then(|s| s.char_indices().next_back().map(|(i, _)| &s[i..])).unwrap_or("")
}

/// `_consumes_inline_children`.
pub fn consumes_inline_children(el: Node) -> bool {
    INLINE_CONSUMING.contains(&el.tag()) && !el.is_empty()
}

/// `_escape_cell`.
pub fn escape_cell(text: &str, in_cell: bool) -> String {
    if in_cell {
        text.replace('|', "\\|").replace('\n', " ")
    } else {
        text.to_owned()
    }
}

/// `_md_link`.
fn md_link(text: &str, url: Option<&str>, image: bool) -> String {
    let esc = UNESCAPED_BRACKET.replace_all(text, |c: &fancy_regex::Captures<str>| match c.get(1) {
        Some(span) => span.as_str().to_owned(),
        None => format!("\\{}", &c[3]),
    });
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
pub fn image_markup(el: Node, fmt: bool) -> String {
    let alt = format!("{} {}", el.get("title").unwrap_or_default(), el.get("alt").unwrap_or_default());
    let mut alt = py::strip(&alt).to_owned();
    if fmt {
        alt = escape_inline_specials(&alt);
    }
    md_link(&alt, Some(&el.get("src").unwrap_or_default()), true)
}

/// `_list_marker`: "N. " in an ordered list with formatting, else "- ", with nesting indent.
pub fn list_marker(item: Node, fmt: bool) -> String {
    let indent = "  ".repeat(item.iter_ancestors(&["list"]).count().saturating_sub(1));
    if fmt && item.parent().is_some_and(|p| p.get("rend").as_deref() == Some("ol")) {
        return format!("{indent}{}. ", item.iter_siblings(&["item"], true).count() + 1);
    }
    format!("{indent}- ")
}

/// `_heading_prefix`: '#' markers for the head level, 2 by default.
pub fn heading_prefix(el: Node) -> String {
    let level = el.get("rend").unwrap_or_default();
    let number = level.chars().nth(1).and_then(|c| c.to_digit(10)).filter(|n| (1..=6).contains(n)).unwrap_or(2);
    "#".repeat(number as usize)
}

/// `_closes_container`: whether the element carries the trailing content of its enclosing item/cell.
pub fn closes_container(el: Node, tag: &str) -> bool {
    if el.tag() == tag {
        return el.is_empty();
    }
    el.next().is_none_or(|n| tag == "item" && n.tag() == "item")
}

/// `_opens_item`: whether nothing in the enclosing item renders before the element.
pub fn opens_item(el: Node) -> bool {
    let mut el = el;
    while el.tag() != "item" {
        match el.parent() {
            Some(parent) if el.prev().is_none() && !text_chars_test(parent.text().as_deref()) => el = parent,
            _ => return false,
        }
    }
    true
}

/// `_item_needs_marker`: whether the item renders content of its own before any sublist.
pub fn item_needs_marker(item: Node) -> bool {
    if text_chars_test(item.text().as_deref()) {
        return true;
    }
    for child in item.children() {
        if child.tag() == "list" {
            return false;
        }
        let text = child.itertext(true).concat() + &child.tail().unwrap_or_default();
        if text_chars_test(Some(&text)) || child.iter(&["graphic"]).next().is_some() {
            return true;
        }
    }
    false
}

/// `_collect_inline_text`.
fn collect_inline_text(ctx: &Ctx, el: Node, fmt: bool) -> Result<String> {
    let mut parts: Vec<String> = el.text().filter(|t| !t.is_empty()).into_iter().collect();
    for child in el.children() {
        let tag = child.tag();
        if tag == "graphic" {
            parts.push(image_markup(child, fmt));
        } else if tag == "lb" {
            parts.push("\n".into());
        } else if INLINE_FORMATTABLE.contains(&tag) {
            parts.push(replace_element_text(ctx, child, fmt, false)?);
        } else if let Some(t) = child.text().filter(|t| !t.is_empty()) {
            parts.push(t);
        }
        if let Some(t) = child.tail().filter(|t| !t.is_empty()) {
            parts.push(t);
        }
    }
    Ok(parts.concat())
}

/// The markdown rendering of `replace_element_text` (heading, del, hi, code). Code with line breaks becomes a
/// fence; its `lb` children are removed (with their tails) from the serializer's copy, as in Python.
fn markdown_text(ctx: &Ctx, el: Node, text: String, in_cell: bool) -> Result<String> {
    Ok(match el.tag() {
        "article" | "list" | "table" => py::strip(&text).to_owned(),
        "head" if !in_cell => format!("{} {text}", heading_prefix(el)),
        "del" => md_wrap(&text, "~~", "~~"),
        "hi" => {
            let rend = el.get("rend").unwrap_or_default();
            match (hi_marker(&rend), hi_html_tag(&rend)) {
                (Some("`"), _) => md_code(&text),
                (Some(marker), _) => md_wrap(&text, marker, marker),
                (None, Some(tag)) => md_wrap(&text, &format!("<{tag}>"), &format!("</{tag}>")),
                _ => text,
            }
        }
        "code" => {
            let lbs = el.xpath_nodes(".//lb")?;
            if text.contains('\n') || !lbs.is_empty() {
                let mut text = text;
                for lb in lbs {
                    text = format!("{text}\n{}", lb.tail().unwrap_or_default());
                    lb.parent().expect("lb below code").remove(ctx, lb)?;
                }
                let fence = code_fence(&text, 3);
                format!("{fence}\n{text}\n{fence}\n")
            } else {
                md_code(&text)
            }
        }
        _ => text,
    })
}

/// `replace_element_text`: the element's own rendering (the tail is handled by the caller).
pub fn replace_element_text(ctx: &Ctx, el: Node, fmt: bool, in_cell: bool) -> Result<String> {
    let tag = el.tag();
    let mut text =
        if consumes_inline_children(el) { collect_inline_text(ctx, el, fmt)? } else { el.text().unwrap_or_default() };
    if fmt && !text.is_empty() && !in_cell && matches!(tag, "p" | "quote" | "item") {
        text = escape_block_start(&text);
    }
    if fmt && !text.is_empty() {
        text = markdown_text(ctx, el, text, in_cell)?;
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
    Ok(text)
}

/// `_append_spaced`: append, dropping a leading space after a separator.
pub fn append_spaced(out: &mut Vec<String>, text: &str) {
    let text = match text.strip_prefix(' ') {
        Some(rest) if SEPARATORS.contains(&last_char(out)) => rest,
        _ => text,
    };
    if !text.is_empty() {
        out.push(text.to_owned());
    }
}

/// `_append_tail`.
pub fn append_tail(el: Node, out: &mut Vec<String>, in_cell: bool, in_item: bool, fmt: bool) {
    let Some(tail) = el.tail().filter(|t| !t.is_empty()) else { return };
    let tag = el.tag();
    let core = py::strip(&tail);
    if fmt && tag == "list" && in_item && !in_cell && !core.is_empty() {
        if let Some(parent) = el.parent() {
            // blank line and parent item indent, else CommonMark continues the last sub-item
            let indent = " ".repeat(list_marker(parent, true).chars().count());
            out.push(format!("\u{2424}\n{indent}{core}"));
            return;
        }
    }
    if in_cell || in_item || tag == "list" {
        let mut core = core.to_owned();
        let starts_space = tail.chars().next().is_some_and(py::is_space);
        if !core.is_empty() && (starts_space || !INLINE_FORMATTABLE.contains(&tag)) {
            core = format!(" {core}");
        }
        let ends_space = tail.chars().next_back().is_some_and(py::is_space);
        if ends_space && el.next().is_some_and(|n| INLINE_CARRIED.contains(&n.tag())) {
            core.push(' ');
        }
        append_spaced(out, &escape_cell(&core, in_cell));
        return;
    }
    if NEWLINE_ELEMS.contains(&tag) {
        out.push(tail.trim_start_matches(py::is_space).to_owned());
    } else {
        out.push(tail);
    }
}
