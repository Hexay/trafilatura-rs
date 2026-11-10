//! lxml.html: `fromstring`, `fragment_fromstring`, `HtmlMixin.drop_tree` / `drop_tag`.
use super::mutate::{html_element, remove_node};
use super::raw;
use super::{Ctx, Error, Node, ParserOpts, Result, LXML_HTML_PARSER};
use crate::py::SPACE_CLASS;
use regex::Regex;
use std::sync::LazyLock;

pub enum Input<'a> {
    Str(&'a str),
    Bytes(&'a [u8]),
}

static FULL_HTML_STR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)^[{SPACE_CLASS}]*<(?:html|!doctype)")).unwrap());
static FULL_HTML_BYTES: LazyLock<regex::bytes::Regex> =
    LazyLock::new(|| regex::bytes::Regex::new(r"(?i-u)^[\t\n\x0B\x0C\r ]*<(?:html|!doctype)").unwrap());

const BLOCK_TAGS: &[&str] = &[
    "address", "blockquote", "center", "del", "div", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "ins", "isindex",
    "noscript", "p", "pre", "dir", "dl", "dt", "dd", "li", "menu", "ol", "ul", "table", "caption", "colgroup", "col",
    "thead", "tfoot", "tbody", "tr", "td", "th", "fieldset", "form", "legend", "optgroup", "option",
];

fn is_blank(s: &Option<String>) -> bool {
    s.as_deref().is_none_or(|t| crate::py::strip(t).is_empty())
}

/// `lxml.html.document_fromstring`.
pub fn document_fromstring(ctx: &Ctx, input: &Input, opts: ParserOpts) -> Result<Node> {
    let root = match input {
        Input::Str(s) => ctx.parse_str(s, opts)?,
        Input::Bytes(b) => ctx.parse_bytes(b, opts)?,
    };
    root.ok_or_else(|| Error::Parser("Document is empty".into()))
}

/// `lxml.html.fromstring`.
pub fn fromstring(ctx: &Ctx, input: &Input, opts: ParserOpts) -> Result<Node> {
    let full = match input {
        Input::Str(s) => FULL_HTML_STR.is_match(s),
        Input::Bytes(b) => FULL_HTML_BYTES.is_match(b),
    };
    let doc = document_fromstring(ctx, input, opts)?;
    if full {
        return Ok(doc);
    }
    let bodies: Vec<Node> = doc.children().into_iter().filter(|c| c.tag() == "body").collect();
    let body = bodies.first().copied();
    if let Some(body) = body {
        for &other in &bodies[1..] {
            if let Some(t) = other.text() {
                match body.child(-1) {
                    Some(last) => last.set_tail(Some(&(last.tail().unwrap_or_default() + &t)))?,
                    None => body.set_text(Some(&(body.text().unwrap_or_default() + &t)))?,
                }
            }
            body.extend(&other.children())?;
            other.drop_tree(ctx)?;
        }
    }
    let heads: Vec<Node> = doc.children().into_iter().filter(|c| c.tag() == "head").collect();
    if let Some(&head) = heads.first() {
        for &other in &heads[1..] {
            head.extend(&other.children())?;
            other.drop_tree(ctx)?;
        }
        return Ok(doc);
    }
    let Some(body) = body else { return Ok(doc) };
    if body.len() == 1 && is_blank(&body.text()) && is_blank(&body.child(-1).and_then(|c| c.tail())) {
        return Ok(body.child(0).expect("one child"));
    }
    let block = body.iter(&["*"]).any(|e| BLOCK_TAGS.contains(&e.tag()));
    body.set_tag(if block { "div" } else { "span" })?;
    Ok(body)
}

/// `lxml.html.fragment_fromstring(html)` with `create_parent=False` and the default lxml.html parser.
pub fn fragment_fromstring(ctx: &Ctx, html: &str) -> Result<Node> {
    let wrapped =
        if FULL_HTML_STR.is_match(html) { html.to_owned() } else { format!("<html><body>{html}</body></html>") };
    let doc = document_fromstring(ctx, &Input::Str(&wrapped), LXML_HTML_PARSER)?;
    let bodies: Vec<Node> = doc.children().into_iter().filter(|c| c.tag() == "body").collect();
    if bodies.len() != 1 {
        return Err(Error::Runtime(format!("too many bodies: {}", bodies.len())));
    }
    let body = bodies[0];
    if !is_blank(&body.text()) {
        return Err(Error::Parser("There is leading text".into()));
    }
    let elements = body.children();
    match elements.as_slice() {
        [] => Err(Error::Parser("No elements found".into())),
        [el] => {
            if !is_blank(&el.tail()) {
                return Err(Error::Parser("Element followed by text".into()));
            }
            el.set_tail(None)?;
            Ok(*el)
        }
        _ => Err(Error::Parser("Multiple elements found".into())),
    }
}

impl Node {
    /// `HtmlMixin.drop_tree`.
    pub fn drop_tree(self, ctx: &Ctx) -> Result<()> {
        let parent = self.parent().ok_or_else(|| Error::Runtime("AssertionError: drop_tree on root".into()))?;
        if let Some(tail) = self.tail().filter(|t| !t.is_empty()) {
            match self.prev() {
                None => parent.set_text(Some(&(parent.text().unwrap_or_default() + &tail)))?,
                Some(p) => p.set_tail(Some(&(p.tail().unwrap_or_default() + &tail)))?,
            }
        }
        parent.remove(ctx, self)
    }

    /// `HtmlMixin.drop_tag`.
    pub fn drop_tag(self, ctx: &Ctx) -> Result<()> {
        let parent = self.parent().ok_or_else(|| Error::Runtime("AssertionError: drop_tag on root".into()))?;
        let previous = self.prev();
        if let Some(text) = self.text().filter(|t| !t.is_empty() && self.is_element()) {
            match previous {
                None => parent.set_text(Some(&(parent.text().unwrap_or_default() + &text)))?,
                Some(p) => p.set_tail(Some(&(p.tail().unwrap_or_default() + &text)))?,
            }
        }
        if let Some(tail) = self.tail().filter(|t| !t.is_empty()) {
            if let Some(last) = self.child(-1) {
                last.set_tail(Some(&(last.tail().unwrap_or_default() + &tail)))?;
            } else {
                match previous {
                    None => parent.set_text(Some(&(parent.text().unwrap_or_default() + &tail)))?,
                    Some(p) => p.set_tail(Some(&(p.tail().unwrap_or_default() + &tail)))?,
                }
            }
        }
        let children = self.children();
        replace_single(ctx, parent, self, &children)
    }
}

/// `parent[i:i+1] = elements` where `parent[i] is old` (`_replaceSlice`, step 1, left to right).
fn replace_single(ctx: &Ctx, parent: Node, old: Node, elements: &[Node]) -> Result<()> {
    unsafe {
        let neighbour = raw::previous_element(old.raw());
        remove_node(ctx, old.raw());
        let anchor = if neighbour.is_null() {
            raw::find_child_forwards(parent.raw(), 0)
        } else {
            raw::next_element(neighbour)
        };
        if anchor.is_null() {
            return parent.extend(elements);
        }
        for &el in elements {
            let n = el.raw();
            let next = (*n).next;
            libxml2_sys::xmlAddPrevSibling(anchor, n);
            raw::move_tail(next, n);
        }
    }
    Ok(())
}

/// `lxml.html.Element(tag)`.
pub fn element(ctx: &Ctx, tag: &str) -> Result<Node> {
    html_element(ctx, tag)
}
