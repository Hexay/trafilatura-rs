//! trafilatura 2.3.1 xml.py markdown helpers (include_formatting=True): code spans/fences, emphasis wrapping, the
//! tree passes `xmltotxt` runs on its copy (inline-special escaping with LaTeX math conversion, emphasis collapse,
//! adjacent-hi merge, block whitespace) and block-start escaping. The small regexes are matched by hand.
use crate::dom::{Ctx, Node, Result};
use crate::py;

const INLINE_SPECIAL_CHARS: [char; 7] = ['*', '_', '`', '~', '[', ']', '<'];
const MERGEABLE_RENDS: [&str; 4] = ["#b", "#i", "#t", "#u"];

/// `HI_FORMATTING`.
pub fn hi_marker(rend: &str) -> Option<&'static str> {
    match rend {
        "#b" => Some("**"),
        "#i" => Some("*"),
        "#t" => Some("`"),
        _ => None,
    }
}

/// `HI_HTML_TAGS`.
pub fn hi_html_tag(rend: &str) -> Option<&'static str> {
    match rend {
        "#sup" => Some("sup"),
        "#sub" => Some("sub"),
        "#u" => Some("u"),
        _ => None,
    }
}

/// `_code_fence`: the shortest backtick run of at least `min_len` absent from `text`.
pub fn code_fence(text: &str, min_len: usize) -> String {
    let (mut fence_len, mut run) = (min_len, 0);
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            if run >= fence_len {
                fence_len = run + 1;
            }
        } else {
            run = 0;
        }
    }
    "`".repeat(fence_len)
}

fn code_span(text: &str) -> String {
    let fence = code_fence(text, 1);
    if text.starts_with('`') || text.ends_with('`') {
        return format!("{fence} {text} {fence}");
    }
    format!("{fence}{text}{fence}")
}

/// `_md_wrap`: markers around the stripped text, flanking whitespace left outside.
pub fn md_wrap(text: &str, opening: &str, closing: &str) -> String {
    let stripped = py::strip(text);
    if stripped.is_empty() {
        return text.to_owned();
    }
    text.replacen(stripped, &format!("{opening}{stripped}{closing}"), 1)
}

/// `_md_code`.
pub fn md_code(text: &str) -> String {
    let stripped = py::strip(text);
    if stripped.is_empty() {
        return text.to_owned();
    }
    text.replacen(stripped, &code_span(stripped), 1)
}

fn at(text: &str, pos: usize, lit: &str) -> bool {
    text[pos..].starts_with(lit)
}

/// `\\\[(.+?)\\\]` (DOTALL) at `pos` with `(?<!\S)` before it: the match end.
fn block_math_at(text: &str, pos: usize) -> Option<usize> {
    if !at(text, pos, "\\[") || text[..pos].chars().next_back().is_some_and(|c| !py::is_space(c)) {
        return None;
    }
    let inner = pos + 2;
    let first = text[inner..].chars().next()?;
    text[inner + first.len_utf8()..].find("\\]").map(|i| inner + first.len_utf8() + i + 2)
}

/// `\\\((.+?)\\\)` (no DOTALL) at `pos`: the match end.
fn inline_math_at(text: &str, pos: usize) -> Option<usize> {
    if !at(text, pos, "\\(") {
        return None;
    }
    let inner = pos + 2;
    let first = text[inner..].chars().next().filter(|&c| c != '\n')?;
    let rest = inner + first.len_utf8();
    let close = rest + text[rest..].find("\\)")?;
    (!text[rest..close].contains('\n')).then_some(close + 2)
}

/// Rewrite each non-overlapping match of `matcher` (scanning left to right) with `repl(inner)`.
fn sub_math(text: &str, matcher: fn(&str, usize) -> Option<usize>, repl: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let (mut last, mut pos) = (0, 0);
    while pos < text.len() {
        if let Some(end) = matcher(text, pos) {
            out.push_str(&text[last..pos]);
            out.push_str(&repl(&text[pos + 2..end - 2]));
            last = end;
            pos = end;
        } else {
            pos += text[pos..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out.push_str(&text[last..]);
    out
}

/// `_convert_math`: LaTeX delimiters to `$$`/`$`.
fn convert_math(text: &str) -> String {
    let text = sub_math(text, block_math_at, |m| format!("\n$$\n{}\n$$\n", py::strip(m)));
    sub_math(&text, inline_math_at, |m| format!("${m}$"))
}

/// `_escape_inline_specials`: each special char gets an escaping backslash, preceding backslashes doubled.
pub fn escape_inline_specials(text: &str) -> String {
    let mut text = text.to_owned();
    for ch in INLINE_SPECIAL_CHARS {
        if !text.contains(ch) {
            continue;
        }
        let mut out = String::with_capacity(text.len() + 8);
        let mut run = 0;
        for c in text.chars() {
            if c == '\\' {
                run += 1;
                continue;
            }
            let n = if c == ch { 2 * run + 1 } else { run };
            out.extend(std::iter::repeat_n('\\', n));
            out.push(c);
            run = 0;
        }
        out.extend(std::iter::repeat_n('\\', run));
        text = out;
    }
    text
}

/// `_escape_and_convert_math`: prose escaped, math spans (`_MATH_SPAN_RE`) converted.
fn escape_and_convert_math(text: &str) -> String {
    let mut out = String::new();
    let (mut last, mut pos) = (0, 0);
    while pos < text.len() {
        match block_math_at(text, pos).or_else(|| inline_math_at(text, pos)) {
            Some(end) => {
                out.push_str(&escape_inline_specials(&text[last..pos]));
                out.push_str(&convert_math(&text[pos..end]));
                last = end;
                pos = end;
            }
            None => pos += text[pos..].chars().next().map_or(1, char::len_utf8),
        }
    }
    out.push_str(&escape_inline_specials(&text[last..]));
    out
}

fn is_code(el: Node) -> bool {
    el.tag() == "code" || (el.tag() == "hi" && hi_marker(&el.get("rend").unwrap_or_default()) == Some("`"))
}

/// `_escape_and_convert_math_tree`: texts and tails rewritten in place, code subtrees left verbatim.
pub fn escape_and_convert_math_tree(el: Node) -> Result<()> {
    if is_code(el) {
        return Ok(());
    }
    if let Some(t) = el.text().filter(|t| !t.is_empty()) {
        el.set_text(Some(&escape_and_convert_math(&t)))?;
    }
    for child in el.children() {
        escape_and_convert_math_tree(child)?;
        if let Some(t) = child.tail().filter(|t| !t.is_empty()) {
            child.set_tail(Some(&escape_and_convert_math(&t)))?;
        }
    }
    Ok(())
}

fn blank(s: Option<String>) -> bool {
    py::strip(&s.unwrap_or_default()).is_empty()
}

/// `_collapse_emphasis`: a hi whose only content is a nested hi with an already active marker absorbs it.
pub fn collapse_emphasis(ctx: &Ctx, el: Node, active: &[&'static str]) -> Result<()> {
    let mut active = active.to_vec();
    if el.tag() == "hi" {
        if let Some(here) = hi_marker(&el.get("rend").unwrap_or_default()) {
            active.push(here);
        }
        while let Some(child) = el.child(0).filter(|c| {
            el.len() == 1
                && blank(el.text())
                && c.tag() == "hi"
                && blank(c.tail())
                && hi_marker(&c.get("rend").unwrap_or_default()).is_some_and(|m| active.contains(&m))
        }) {
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

/// `_strip_block_whitespace`.
pub fn strip_block_whitespace(el: Node) -> Result<()> {
    for p in el.iter(&["p", "quote", "head"]).collect::<Vec<_>>() {
        if let Some(t) = p.text().filter(|t| !t.is_empty()) {
            let t = if p.is_empty() { py::strip(&t) } else { t.trim_start_matches(py::is_space) };
            p.set_text(Some(t))?;
        }
        if let Some(last) = p.child(-1) {
            if let Some(t) = last.tail().filter(|t| !t.is_empty()) {
                last.set_tail(Some(t.trim_end_matches(py::is_space)))?;
            }
        }
    }
    Ok(())
}

/// `_merge_adjacent_hi`: childless siblings with the same mergeable rend become one.
pub fn merge_adjacent_hi(ctx: &Ctx, el: Node) -> Result<()> {
    for child in el.children() {
        merge_adjacent_hi(ctx, child)?;
    }
    let children = el.children();
    for i in (0..children.len().saturating_sub(1)).rev() {
        let (curr, nxt) = (children[i], children[i + 1]);
        let rend = curr.get("rend");
        if curr.tag() == "hi"
            && nxt.tag() == "hi"
            && rend == nxt.get("rend")
            && rend.as_deref().is_some_and(|r| MERGEABLE_RENDS.contains(&r))
            && curr.is_empty()
            && nxt.is_empty()
            && blank(curr.tail())
        {
            let text =
                curr.text().unwrap_or_default() + &curr.tail().unwrap_or_default() + &nxt.text().unwrap_or_default();
            curr.set_text(Some(&text))?;
            curr.set_tail(nxt.tail().as_deref())?;
            el.remove(ctx, nxt)?;
        }
    }
    Ok(())
}

/// `_escape_block_start`: a backslash before a leading heading/quote/list marker.
pub fn escape_block_start(text: &str) -> String {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 {
        return text.to_owned();
    }
    let rest = &text[indent..];
    let ends_marker = |after: &str| after.is_empty() || after == "\n" || after.starts_with([' ', '\t']);
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    let marker = if (1..=6).contains(&hashes) && ends_marker(&rest[hashes..]) {
        Some(indent)
    } else if rest.starts_with('>') {
        Some(indent)
    } else if rest.starts_with(['-', '+']) && ends_marker(&rest[1..]) {
        Some(indent)
    } else {
        let digits: usize = rest.chars().take_while(|&c| py::int::decimal_value(c).is_some()).map(char::len_utf8).sum();
        let count = rest[..digits].chars().count();
        let after = &rest[digits..];
        ((1..=9).contains(&count) && after.starts_with(['.', ')']) && ends_marker(&after[1..]))
            .then_some(indent + digits)
    };
    match marker {
        Some(pos) => format!("{}\\{}", &text[..pos], &text[pos..]),
        None => text.to_owned(),
    }
}
