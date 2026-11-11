//! baseline.py's embedded-JSON sources: schema.org JSON-LD walks, the Discourse preload and `_render_text`.
use super::json::{loads, Value};
use crate::dom::{document_fromstring, Ctx, Error, Input, Node, Result, LXML_HTML_PARSER};
use crate::py;
use crate::trafilatura::text::{remove_control_characters, trim};
use crate::trafilatura::xml_unescape::unescape;
use regex::Regex;
use std::sync::LazyLock;

const JSON_TEXT_KEYS: &[&str] = &["articleBody", "reviewBody"];
const DESCRIPTION_TYPES: &[&str] = &["Product", "VideoObject"];
const JSON_HOOKS: &[&str] = &[
    "articleBody",
    "reviewBody",
    "recipeInstructions",
    "acceptedAnswer",
    "\"Product\"",
    "\"VideoObject\"",
    "\"HowTo\"",
];
const HTML_TAG_NAMES: &str = "a|abbr|address|article|aside|b|blockquote|body|br|caption|cite|code|dd|del|div|dl|dt|\
    em|figcaption|figure|footer|h[1-6]|head|header|hr|html|i|img|ins|kbd|li|main|mark|nav|\
    ol|p|pre|q|quote|s|section|small|span|strong|sub|summary|sup|table|tbody|td|tfoot|th|\
    thead|time|title|tr|u|ul";
pub const BLOCK_ELEMS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "br",
    "dd",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "td",
    "th",
    "tr",
    "ul",
];
static FULL_HTML: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&py::re::translate(r"^\s*<(?:html|!doctype)", true)).unwrap());

fn as_list(v: Option<&Value>) -> &[Value] {
    match v {
        None | Some(Value::Null) => &[],
        Some(Value::List(items)) => items,
        Some(v) => std::slice::from_ref(v),
    }
}

/// `t in str(value)` for the @type value: a str is itself; a container's repr keeps every nested key/value
/// string verbatim (escapes only touch non-letters), and scalars' reprs never hold these words.
fn type_mentions(v: &Value, t: &str) -> bool {
    match v {
        Value::Str(s) => s.contains(t),
        Value::List(items) => items.iter().any(|i| type_mentions(i, t)),
        Value::Dict(items) => items.iter().any(|(k, i)| k.contains(t) || type_mentions(i, t)),
        _ => false,
    }
}

fn str_value(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

/// `_walk_json`.
fn walk_json(node: Option<&Value>, bodies: &mut Vec<String>, teasers: &mut Vec<String>) {
    for item in as_list(node) {
        if !matches!(item, Value::Dict(_)) {
            continue;
        }
        for key in JSON_TEXT_KEYS {
            if let Some(s) = str_value(item.get(key)).filter(|s| !s.is_empty()) {
                bodies.push(s.to_owned());
            }
        }
        for key in ["recipeInstructions", "step"] {
            for step in as_list(item.get(key)) {
                match step {
                    Value::Str(s) => bodies.push(s.clone()),
                    Value::Dict(_) => {
                        let subs = std::iter::once(step).chain(as_list(step.get("itemListElement")));
                        bodies.extend(subs.filter_map(|sub| str_value(sub.get("text")).map(str::to_owned)));
                    }
                    _ => {}
                }
            }
        }
        if let Some(answer) = item.get("acceptedAnswer") {
            if let Some(t) = str_value(answer.get("text")) {
                bodies.push(t.to_owned());
            }
        }
        let typed = item.get("@type").is_some_and(|t| DESCRIPTION_TYPES.iter().any(|d| type_mentions(t, d)));
        if typed {
            if let Some(d) = str_value(item.get("description")) {
                teasers.push(d.to_owned());
            }
        }
        for container in ["@graph", "mainEntity"] {
            walk_json(item.get(container), bodies, teasers);
        }
    }
}

/// `_discourse_texts`: post HTML from a Discourse `data-preloaded` attribute.
fn discourse_texts(tree: Node) -> Result<Vec<String>> {
    let Some(node) = tree.find(r#".//div[@id="data-preloaded"]"#) else { return Ok(Vec::new()) };
    let Ok(Value::Dict(preloaded)) = loads(&node.get("data-preloaded").unwrap_or_default(), true) else {
        return Ok(Vec::new());
    };
    let mut texts = Vec::new();
    for (key, value) in &preloaded {
        if !key.starts_with("topic_") {
            continue;
        }
        let Some(Ok(topic)) = value.as_str().map(|s| loads(s, true)) else { continue };
        let Some(posts) = topic.get("post_stream").and_then(|p| p.get("posts")) else { continue };
        match posts {
            Value::List(items) => {
                texts.extend(items.iter().filter_map(|p| str_value(p.get("cooked")).map(str::to_owned)));
            }
            // iterating a str yields chars and a dict its keys: no post dicts either way
            Value::Str(_) | Value::Dict(_) => {}
            _ => return Err(Error::Type("object is not iterable".into())),
        }
    }
    Ok(texts)
}

/// `_collect_json_content`: (full-text bodies, teaser descriptions).
pub fn collect_json_content(tree: Node) -> Result<(Vec<String>, Vec<String>)> {
    let (mut bodies, mut teasers) = (Vec::new(), Vec::new());
    for elem in tree.findall(r#".//script[@type="application/ld+json"]"#) {
        let Some(text) = elem.text().filter(|t| !t.is_empty()) else { continue };
        if !JSON_HOOKS.iter().any(|h| text.contains(h)) {
            continue;
        }
        if let Ok(v) = loads(&text, false) {
            walk_json(Some(&v), &mut bodies, &mut teasers);
        }
    }
    bodies.extend(discourse_texts(tree)?);
    Ok((bodies, teasers))
}

/// baseline.py `_spaced_text`: the text with block boundaries spaced (mutates `element`).
pub fn spaced_text(element: Node) -> Result<String> {
    for elem in element.iter(BLOCK_ELEMS) {
        let spaced = |s: Option<String>| match s.filter(|s| !s.is_empty()) {
            Some(s) => format!(" {}", remove_control_characters(&s)),
            None => " ".to_owned(),
        };
        elem.set_text(Some(&spaced(elem.text())))?;
        elem.set_tail(Some(&spaced(elem.tail())))?;
    }
    Ok(trim(&element.text_content()))
}

/// `block_text(fragment_fromstring(raw, create_parent="div"))`: the fragment body's spaced text (the wrapper div
/// only adds whitespace, which trimming removes).
fn fragment_text(ctx: &Ctx, raw: &str) -> Result<String> {
    let wrapped = if FULL_HTML.is_match(raw) { raw.to_owned() } else { format!("<html><body>{raw}</body></html>") };
    let doc = document_fromstring(ctx, &Input::Str(&wrapped), LXML_HTML_PARSER)?;
    let bodies: Vec<Node> = doc.children().into_iter().filter(|c| c.tag() == "body").collect();
    match bodies.as_slice() {
        [body] if doc.tag() == "html" => spaced_text(*body),
        _ => Err(Error::Runtime("AssertionError".into())),
    }
}

/// `_render_text`: clean text from an embedded-JSON value that may carry (escaped) HTML.
pub fn render_text(ctx: &Ctx, raw: &str) -> String {
    let raw = remove_control_characters(&unescape(raw));
    let markup = py::re::compile(&format!(r"</({HTML_TAG_NAMES})>|<({HTML_TAG_NAMES})(\s[^<>]*=[^<>]*)?/?>"), true);
    if markup.is_match(&raw) {
        if let Ok(t) = fragment_text(ctx, &raw) {
            return t;
        }
    }
    trim(&raw)
}
