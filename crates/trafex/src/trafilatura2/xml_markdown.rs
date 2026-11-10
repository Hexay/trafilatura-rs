//! xml.py rewrites that only run with include_formatting (markdown): emphasis/code/heading markers, math
//! delimiters and redundant-emphasis collapsing. Not exercised by the parity configurations.
use super::xml_text::heading_prefix;
use crate::dom::{Ctx, Node, Result};
use crate::py;

const HI_FORMATTING: &[(&str, &str)] = &[("#b", "**"), ("#i", "*"), ("#u", "__"), ("#t", "`")];
const HI_HTML_TAGS: &[(&str, &str)] = &[("#sup", "sup"), ("#sub", "sub")];

fn hi_marker(el: Node) -> Option<&'static str> {
    let rend = el.get("rend").unwrap_or_default();
    HI_FORMATTING.iter().find(|(k, _)| *k == rend).map(|&(_, v)| v)
}

/// `_code_fence`: shortest backtick run of at least `min_len` longer than any run inside `text`.
fn code_fence(text: &str, min_len: usize) -> String {
    let (mut fence, mut run) = (min_len, 0);
    for c in text.chars() {
        if c == '`' {
            run += 1;
            if run >= fence {
                fence = run + 1;
            }
        } else {
            run = 0;
        }
    }
    "`".repeat(fence)
}

fn code_span(text: &str) -> String {
    let fence = code_fence(text, 1);
    if text.starts_with('`') || text.ends_with('`') {
        return format!("{fence} {text} {fence}");
    }
    format!("{fence}{text}{fence}")
}

/// `_md_wrap`: wrap the stripped text, leaving flanking whitespace outside.
fn md_wrap(text: &str, opening: &str, closing: &str) -> String {
    let stripped = py::strip(text);
    if stripped.is_empty() {
        return text.to_owned();
    }
    text.replacen(stripped, &format!("{opening}{stripped}{closing}"), 1)
}

fn md_code(text: &str) -> String {
    let stripped = py::strip(text);
    if stripped.is_empty() {
        return text.to_owned();
    }
    text.replacen(stripped, &code_span(stripped), 1)
}

/// The `include_formatting and elem_text` branch of `replace_element_text`.
pub fn format_markdown(ctx: &Ctx, el: Node, text: String, in_cell: bool) -> Result<String> {
    Ok(match el.tag() {
        "article" | "list" | "table" => py::strip(&text).to_owned(),
        "head" if !in_cell => format!("{} {text}", heading_prefix(el)),
        "del" => md_wrap(&text.replace("~~", "~\\~"), "~~", "~~"),
        "hi" => {
            let rend = el.get("rend").unwrap_or_default();
            match hi_marker(el) {
                Some("`") => md_code(&text),
                Some(m) => md_wrap(&text, m, m),
                None => match HI_HTML_TAGS.iter().find(|(k, _)| *k == rend) {
                    Some((_, t)) => md_wrap(&text, &format!("<{t}>"), &format!("</{t}>")),
                    None => text,
                },
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

/// `_MATH_BLOCK_RE` (`(?<!\S)\\\[(.+?)\\\]`, DOTALL) then `_MATH_INLINE_RE` (`\\\((.+?)\\\)`) substitutions.
fn convert_math(text: &str) -> String {
    let block = replace_delimited(text, "\\[", "\\]", true, |g| format!("\n$$\n{}\n$$\n", py::strip(g)));
    replace_delimited(&block, "\\(", "\\)", false, |g| format!("${g}$"))
}

/// Left-to-right non-overlapping `open(.+?)close`; `block` = DOTALL and a whitespace-or-start lookbehind.
fn replace_delimited(text: &str, open: &str, close: &str, block: bool, render: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let before_ok = !block || text[..i].chars().next_back().is_none_or(py::is_space);
        if rest.starts_with(open) && before_ok {
            let body = &rest[open.len()..];
            let first = body.chars().next().map_or(0, char::len_utf8);
            if first > 0 {
                if let Some(j) = body[first..].find(close).map(|j| j + first) {
                    let group = &body[..j];
                    if block || !group.contains('\n') {
                        out.push_str(&render(group));
                        i += open.len() + j + close.len();
                        continue;
                    }
                }
            }
        }
        let c = rest.chars().next().expect("non-empty");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// `_convert_math_tree`: rewrite math in text/tails, skipping code subtrees.
pub fn convert_math_tree(el: Node) -> Result<()> {
    if el.tag() == "code" || (el.tag() == "hi" && hi_marker(el) == Some("`")) {
        return Ok(());
    }
    if let Some(t) = el.text().filter(|t| !t.is_empty()) {
        el.set_text(Some(&convert_math(&t)))?;
    }
    for child in el.children() {
        convert_math_tree(child)?;
        if let Some(t) = child.tail().filter(|t| !t.is_empty()) {
            child.set_tail(Some(&convert_math(&t)))?;
        }
    }
    Ok(())
}

/// `_collapse_emphasis`: splice out nested `hi` levels whose marker is already active.
pub fn collapse_emphasis(ctx: &Ctx, el: Node, active: &[&'static str]) -> Result<()> {
    let mut active = active.to_vec();
    if el.tag() == "hi" {
        if let Some(m) = hi_marker(el) {
            active.push(m);
        }
        loop {
            let Some(child) = el.child(0) else { break };
            let blank = |s: Option<String>| py::strip(&s.unwrap_or_default()).is_empty();
            let collapsible = blank(el.text())
                && el.len() == 1
                && child.tag() == "hi"
                && blank(child.tail())
                && hi_marker(child).is_some_and(|m| active.contains(&m));
            if !collapsible {
                break;
            }
            el.set_text(Some(&(el.text().unwrap_or_default() + &child.text().unwrap_or_default())))?;
            el.extend(&child.children())?;
            el.remove(ctx, child)?;
        }
    }
    for child in el.children() {
        collapse_emphasis(ctx, child, &active)?;
    }
    Ok(())
}
