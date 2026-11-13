//! output_format="xml": xml.py `build_xml_output` / `control_xml_output` and utils.py `sanitize_tree`. The tree is
//! serialized, reparsed without blank text and pretty-printed by libxml2, as lxml does.
use super::document::Document;
use super::text::sanitize;
use super::xml::{remove_empty_elements, strip_double_tags};
use crate::dom::{Ctx, Error, Method, Node, Result};
use crate::py;
use crate::py::value::Value;

const WITH_ATTRIBUTES: &[&str] = &["cell", "row", "del", "graphic", "head", "hi", "item", "list", "ref"];
const SPACING_PROTECTED: &[&str] = &["code", "pre"];
const FORMATTING_PROTECTED: &[&str] = &["cell", "head", "hi", "item", "p", "quote", "ref", "td"];

/// `clean_attributes`.
pub fn clean_attributes(tree: Node) -> Node {
    for elem in tree.iter(&["*"]) {
        if !WITH_ATTRIBUTES.contains(&elem.tag()) {
            elem.clear_attrs();
        }
    }
    tree
}

/// `meta_items`: the non-empty META_ATTRIBUTES as strings, lists joined with ";".
pub fn meta_items(doc: &Document) -> Result<Vec<(&'static str, String)>> {
    let fields = [
        ("sitename", &doc.sitename),
        ("title", &doc.title),
        ("author", &doc.author),
        ("date", &doc.date),
        ("url", &doc.url),
        ("hostname", &doc.hostname),
        ("description", &doc.description),
        ("categories", &doc.categories),
        ("tags", &doc.tags),
        ("license", &doc.license),
        ("id", &doc.id),
        ("fingerprint", &doc.fingerprint),
        ("language", &doc.language),
    ];
    let mut out = Vec::new();
    for (name, value) in fields {
        let rendered = match value {
            _ if !value.truthy() => continue,
            Value::Str(s) => s.clone(),
            Value::List(items) => {
                let strs: Option<Vec<&str>> = items.iter().map(Value::as_str).collect();
                strs.ok_or_else(|| Error::Unsupported("uncaught TypeError in ';'.join".into()))?.join(";")
            }
            _ => return Err(Error::Unsupported("uncaught TypeError in ';'.join".into())),
        };
        out.push((name, rendered));
    }
    Ok(out)
}

/// `utils.sanitize_tree`: trim text and tails, drop attributes with an undeclared namespace prefix.
pub fn sanitize_tree(tree: Node) -> Result<Node> {
    for elem in tree.iter(&[]) {
        let parent_tag = elem.parent().map_or("", |p| p.tag());
        let preserve = SPACING_PROTECTED.contains(&elem.tag()) || SPACING_PROTECTED.contains(&parent_tag);
        let trailing =
            FORMATTING_PROTECTED.contains(&elem.tag()) || FORMATTING_PROTECTED.contains(&parent_tag) || preserve;
        for (key, _) in elem.attrs().into_iter().filter(|(k, _)| k.contains(':')) {
            elem.pop_attr(&key);
        }
        if let Some(text) = elem.text().filter(|t| !t.is_empty()) {
            elem.set_text(sanitize(&text, preserve, trailing).as_deref())?;
        }
        if let Some(tail) = elem.tail().filter(|t| !t.is_empty()) {
            elem.set_tail(sanitize(&tail, preserve, trailing).as_deref())?;
        }
    }
    Ok(tree)
}

/// `build_xml_output`: `<doc>` with the metadata as attributes, `<main>` and `<comments>`.
fn build_xml_output(ctx: &Ctx, doc: &Document, body: Node) -> Result<Node> {
    let output = ctx.element("doc")?;
    for (name, value) in meta_items(doc)? {
        output.set(name, Some(&value))?;
    }
    body.set_tag("main")?;
    output.append(clean_attributes(body))?;
    if let Some(comments) = doc.commentsbody {
        comments.set_tag("comments")?;
        output.append(clean_attributes(comments))?;
    }
    Ok(output)
}

/// The end of `control_xml_output`: sanitize, round-trip through CONTROL_PARSER, pretty-print, strip.
pub fn serialize_xml(ctx: &Ctx, output: Node) -> Result<String> {
    let output = sanitize_tree(output)?;
    // an XMLSyntaxError escapes extract(): Python raises it
    let reparsed = ctx
        .parse_xml_noblanks(&output.tostring(Method::Xml, true))
        .map_err(|_| Error::Unsupported("XMLSyntaxError in control_xml_output".into()))?;
    Ok(py::strip(&reparsed.tostring_pretty()).to_owned())
}

/// `determine_returnstring`'s xml branch: the last cleaning, then `control_xml_output`.
pub fn xml_output(ctx: &Ctx, doc: &Document) -> Result<String> {
    let body = doc.body.expect("extracted body");
    for element in body.iter(&["*"]) {
        if element.tag() != "graphic" && element.is_empty() && element.text().is_none_or(|t| t.is_empty()) {
            if let Some(parent) = element.parent().filter(|p| p.tag() != "code") {
                if element.tail().is_none_or(|t| t.is_empty()) {
                    parent.remove(ctx, element)?;
                }
            }
        }
    }
    strip_double_tags(ctx, body)?;
    remove_empty_elements(ctx, body)?;
    let output = build_xml_output(ctx, doc, body)?;
    serialize_xml(ctx, output)
}
