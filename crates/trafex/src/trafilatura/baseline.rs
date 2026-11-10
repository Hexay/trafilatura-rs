//! trafilatura/baseline.py `baseline` for an already parsed tree (what bare_extraction passes).
use super::baseline_json::{loads, Value};
use super::htmlprocessing::attribute_error;
use super::text::trim;
use super::utils::load_html;
use super::xml::delete_element;
use super::xpaths::BASIC_CLEAN_XPATH;
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;
use std::collections::HashSet;

/// `basic_cleaning`.
pub fn basic_cleaning(ctx: &Ctx, tree: Node) -> Result<Node> {
    for elem in tree.xpath_nodes(BASIC_CLEAN_XPATH)? {
        delete_element(ctx, elem)?;
    }
    Ok(tree)
}

fn add_p(postbody: Node, text: &str) -> Result<()> {
    postbody.sub_element("p")?.set_text(Some(text))
}

fn append_text(acc: &mut String, text: &str) {
    if !acc.is_empty() {
        acc.push(' ');
    }
    acc.push_str(text);
}

/// The `articleBody` text of one JSON-LD script, or None when Python's `if json_body:` is false.
fn json_article_text(ctx: &Ctx, script_text: &str) -> Result<Option<String>> {
    let body = match loads(script_text) {
        Ok(Value::Dict { article_body: Some(v), .. }) => *v,
        _ => return Ok(None),
    };
    match body {
        Value::Null | Value::Bool(false) | Value::Number(false) | Value::List(false) => Ok(None),
        Value::Dict { non_empty: false, .. } => Ok(None),
        Value::Str { s, .. } if s.is_empty() => Ok(None),
        Value::Str { lone_surrogate: true, .. } => {
            Err(Error::Runtime("UnicodeEncodeError: surrogates not allowed".into()))
        }
        Value::Str { s, .. } => {
            if s.contains("<p>") {
                let tree = load_html(ctx, &s).ok_or_else(|| attribute_error("'NoneType' object has no attribute 'text_content'"))?;
                Ok(Some(trim(&tree.text_content())))
            } else {
                Ok(Some(trim(&s)))
            }
        }
        // Every other truthy value ends in a TypeError: `"<p>" in x`, load_html(x), or str + None from trim(x).
        _ => Err(Error::Type("non-str articleBody".into())),
    }
}

/// `baseline`: (body, text, text length in code points).
pub fn baseline(ctx: &Ctx, tree: Node) -> Result<(Node, String, usize)> {
    let postbody = ctx.element("body")?;
    let mut temp_text = String::new();
    for elem in tree.findall(r#".//script[@type="application/ld+json"]"#) {
        let Some(text) = elem.text().filter(|t| !t.is_empty() && t.contains("articleBody")) else { continue };
        if let Some(t) = json_article_text(ctx, &text)? {
            add_p(postbody, &t)?;
            append_text(&mut temp_text, &t);
        }
    }
    if py::len(&temp_text) > 100 {
        let n = py::len(&temp_text);
        return Ok((postbody, temp_text, n));
    }
    let tree = basic_cleaning(ctx, tree)?;
    let mut temp_text = String::new();
    for article in tree.findall(".//article") {
        let text = trim(&article.text_content());
        if py::len(&text) > 100 {
            add_p(postbody, &text)?;
            append_text(&mut temp_text, &text);
        }
    }
    if !postbody.is_empty() {
        let n = py::len(&temp_text);
        return Ok((postbody, temp_text, n));
    }
    let mut results = HashSet::new();
    let mut temp_text = String::new();
    for element in tree.iter(&["blockquote", "code", "p", "pre", "q", "quote"]) {
        let entry = trim(&element.text_content());
        if !results.contains(&entry) {
            add_p(postbody, &entry)?;
            append_text(&mut temp_text, &entry);
            results.insert(entry);
        }
    }
    if py::len(&temp_text) > 100 {
        let n = py::len(&temp_text);
        return Ok((postbody, temp_text, n));
    }
    let postbody = ctx.element("body")?;
    if let Some(body_elem) = tree.find(".//body") {
        let elem = postbody.sub_element("p")?;
        let joined = body_elem.itertext(true).iter().map(|e| trim(e)).collect::<Vec<_>>().join("\n");
        elem.set_text(Some(&joined))?;
        let text = elem.text().unwrap_or_default();
        let n = py::len(&text);
        return Ok((postbody, text, n));
    }
    // html2txt(tree, clean=False) finds no body either, so it yields "".
    add_p(postbody, "")?;
    Ok((postbody, String::new(), 0))
}
