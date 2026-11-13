//! trafilatura 2.3.1 output_format="xml": `determine_returnstring`'s last cleaning, xml.py `strip_double_tags`,
//! `remove_empty_elements` and `build_xml_output`. Sanitizing and serialization are 1.11's.
use super::xml::{delete_element, separates_inline};
use super::xml_text::replace_element_text;
use crate::dom::{Ctx, Error, Node, Result};
use crate::trafilatura::document::Document;
use crate::trafilatura::text::text_chars_test;
use crate::trafilatura::xml_output::{clean_attributes, meta_items, serialize_xml};

const NESTING_WHITELIST: &[&str] = &["cell", "figure", "item", "note", "quote"];

/// `is_padding_cell`: an empty cell keeping the following values of its row in their columns.
fn is_padding_cell(el: Node) -> bool {
    el.tag() == "cell" && el.parent().is_some_and(|p| text_chars_test(Some(&p.itertext(true).concat())))
}

/// `keeps_empty`.
fn keeps_empty(el: Node) -> bool {
    match el.parent() {
        None => true,
        Some(parent) => el.tag() == "graphic" || parent.tag() == "code" || separates_inline(el) || is_padding_cell(el),
    }
}

fn is_blank(s: Option<String>) -> bool {
    s.is_none_or(|s| s.is_empty())
}

/// `merge_with_parent` (include_formatting=False).
fn merge_with_parent(ctx: &Ctx, element: Node) -> Result<()> {
    let Some(parent) = element.parent() else { return Ok(()) };
    let mut full_text = replace_element_text(ctx, element, false, false)?;
    if let Some(tail) = element.tail() {
        full_text += &tail;
    }
    if let Some(prev) = element.prev() {
        let t = match prev.tail().filter(|t| !t.is_empty()) {
            Some(pt) => format!("{pt} {full_text}"),
            None => full_text,
        };
        prev.set_tail(Some(&t))?;
    } else if let Some(pt) = parent.text() {
        parent.set_text(Some(&format!("{pt} {full_text}")))?;
    } else {
        parent.set_text(Some(&full_text))?;
    }
    parent.remove(ctx, element)
}

/// `strip_double_tags`.
fn strip_double_tags(ctx: &Ctx, tree: Node) -> Result<()> {
    for elem in tree.xpath_nodes(".//head | .//code | .//p")?.into_iter().rev() {
        for subelem in elem.iter_descendants(&["code", "head", "p"]) {
            let parent_tag = subelem.parent().map(|p| p.tag());
            if subelem.tag() == elem.tag() && !parent_tag.is_some_and(|t| NESTING_WHITELIST.contains(&t)) {
                if parent_tag.is_none() {
                    return Err(Error::Runtime("AttributeError: 'NoneType' object has no attribute 'tag'".into()));
                }
                merge_with_parent(ctx, subelem)?;
            }
        }
    }
    Ok(())
}

/// `remove_empty_elements`, bottom-up.
fn remove_empty_elements(ctx: &Ctx, tree: Node) -> Result<()> {
    for element in tree.iter(&["*"]).collect::<Vec<_>>().into_iter().rev() {
        if element.is_empty()
            && !text_chars_test(element.text().as_deref())
            && !text_chars_test(element.tail().as_deref())
            && !keeps_empty(element)
        {
            delete_element(ctx, element, false)?;
        }
    }
    Ok(())
}

/// `determine_returnstring`'s xml branch: the last cleaning, then `control_xml_output` with `build_xml_output`.
pub fn xml_output(ctx: &Ctx, doc: &Document) -> Result<String> {
    let body = doc.body.expect("extracted body");
    for element in body.iter(&["*"]) {
        if element.is_empty() && is_blank(element.text()) && is_blank(element.tail()) && !keeps_empty(element) {
            delete_element(ctx, element, false)?;
        }
    }
    strip_double_tags(ctx, body)?;
    remove_empty_elements(ctx, body)?;
    let output = ctx.element("doc")?;
    for (name, value) in meta_items(doc)? {
        output.set(name, Some(&value))?;
    }
    body.set_tag("main")?;
    output.append(clean_attributes(body))?;
    let comments = doc.commentsbody.expect("comments body");
    comments.set_tag("comments")?;
    output.append(clean_attributes(comments))?;
    serialize_xml(ctx, output)
}
