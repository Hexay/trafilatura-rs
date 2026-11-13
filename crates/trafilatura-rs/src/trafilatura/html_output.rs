//! output_format="html": htmlprocessing.py `convert_to_html` / `build_html_output` of trafilatura 1.11 (`v2`
//! false) and 2.3.1. The Python exceptions raised here escape extract(), so they are reproduced.
use super::document::Document;
use super::xml_output::meta_items;
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;
use crate::py::int::py_int;

const CONVERTED_V1: &[&str] = &["list", "item", "code", "quote", "head", "lb", "img", "ref", "hi"];
const CONVERTED_V2: &[&str] = &["list", "item", "code", "quote", "head", "lb", "graphic", "ref", "hi", "row", "cell"];

/// lxml's TypeError for a non-str attribute value.
fn not_str(type_name: &str) -> Error {
    Error::Type(format!("Argument must be bytes or unicode, got '{type_name}'"))
}

fn key_error(key: Option<&str>) -> Error {
    Error::Runtime(format!("KeyError: {}", key.map_or("None".into(), |k| format!("'{k}'"))))
}

/// `HTML_TAG_MAPPING` (REND_TAG_MAPPING inverted, the last tag per rend winning).
fn html_tag(rend: &str) -> Option<&'static str> {
    Some(match rend {
        "#i" => "i",
        "#b" => "strong",
        "#u" => "u",
        "#t" => "var",
        "#sub" => "sub",
        "#sup" => "sup",
        _ => return None,
    })
}

fn heading(rend: Option<String>) -> Result<String> {
    let rend = rend.ok_or_else(|| Error::Type("'NoneType' object is not subscriptable".into()))?;
    let level = rend.chars().skip(1).collect::<String>();
    py_int(&level)
        .map(|n| format!("h{n}"))
        .map_err(|_| Error::Value(format!("invalid literal for int() with base 10: '{level}'")))
}

fn new_tag(elem: Node, v2: bool) -> Result<String> {
    let rend = elem.get("rend");
    Ok(match elem.tag() {
        "list" => "ul".into(),
        "item" => "li".into(),
        "code" => "pre".into(),
        "quote" => "blockquote".into(),
        "head" => heading(if v2 { rend.or(Some("h3".into())) } else { rend })?,
        "lb" => "br".into(),
        "img" => "graphic".into(),
        "graphic" => "img".into(),
        "ref" => "a".into(),
        "hi" => {
            let rend = if v2 { rend.unwrap_or_else(|| "#i".into()) } else { rend.ok_or_else(|| key_error(None))? };
            html_tag(&rend).ok_or_else(|| key_error(Some(&rend)))?.into()
        }
        "row" => "tr".into(),
        _ if elem.get("role").as_deref() == Some("head") => "th".into(),
        _ => "td".into(),
    })
}

/// `convert_to_html`: the body renamed to HTML tags inside a new `<html>`.
fn convert_to_html(ctx: &Ctx, tree: Node, v2: bool) -> Result<Node> {
    for elem in tree.iter(if v2 { CONVERTED_V2 } else { CONVERTED_V1 }) {
        let tag = new_tag(elem, v2)?;
        elem.set_tag(&tag)?;
        if tag == "a" {
            let target = elem.pop_attr("target");
            if !v2 && target.is_none() {
                return Err(not_str("NoneType"));
            }
            elem.set("href", Some(&target.unwrap_or_default()))?;
        } else if !(v2 && tag == "img") {
            elem.clear_attrs();
        }
    }
    tree.set_tag("body")?;
    let root = ctx.element("html")?;
    root.append(tree)?;
    Ok(root)
}

/// `build_html_output`.
pub fn html_output(ctx: &Ctx, doc: &Document, with_metadata: bool, v2: bool) -> Result<String> {
    let html = convert_to_html(ctx, doc.body.expect("extracted body"), v2)?;
    if with_metadata {
        let head = ctx.element("head")?;
        for (name, value) in meta_items(doc)? {
            if !v2 && matches!(name, "categories" | "tags") {
                return Err(not_str("list"));
            }
            let meta = head.sub_element("meta")?;
            meta.set("name", Some(name))?;
            meta.set("content", Some(&value))?;
        }
        html.insert(0, head)?;
    }
    Ok(py::strip(&html.tostring_pretty()).to_owned())
}
