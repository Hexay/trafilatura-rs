//! Tree preparation passes: `remove_unlikely_candidates`, `transform_misused_divs_into_paragraphs`.
use super::new_p;
use super::regexes::{DIV_TO_P_ELEMENTS, OK_MAYBE_CANDIDATE, UNLIKELY_CANDIDATES};
use crate::dom::{Ctx, Method, Node, Result};
use crate::py;

pub fn remove_unlikely_candidates(ctx: &Ctx, doc: Node) -> Result<()> {
    for elem in doc.findall(".//*") {
        let attrs: Vec<String> =
            [elem.get("class"), elem.get("id")].into_iter().flatten().filter(|a| !a.is_empty()).collect();
        let attrs = attrs.join(" ");
        if py::len(&attrs) < 2 {
            continue;
        }
        if !matches!(elem.tag(), "body" | "html")
            && UNLIKELY_CANDIDATES.is_match(&attrs)
            && !OK_MAYBE_CANDIDATE.is_match(&attrs)
        {
            elem.drop_tree(ctx)?;
        }
    }
    Ok(())
}

fn is_blank(s: &Option<String>) -> bool {
    s.as_deref().is_none_or(|t| py::strip(t).is_empty())
}

/// Python searches the XML serialization of the children (tails included). There '<' only opens start tags,
/// end tags, comments and PIs, so without comments/PIs the search equals testing "<" + each descendant tag;
/// otherwise serialize as Python does.
fn has_block_child(elem: Node) -> bool {
    let children = elem.children();
    let mut saw_other = false;
    for d in children.iter().flat_map(|c| c.iter(&[])) {
        if !d.is_element() {
            saw_other = true;
        } else if DIV_TO_P_ELEMENTS.is_match(&format!("<{}", d.tag())) {
            return true;
        }
    }
    // no match can straddle two children: a serialization never ends in "<"
    saw_other && children.iter().any(|c| DIV_TO_P_ELEMENTS.is_match(&c.tostring(Method::Xml, true)))
}

pub fn transform_misused_divs_into_paragraphs(ctx: &Ctx, doc: Node) -> Result<()> {
    for elem in doc.findall(".//div") {
        if !has_block_child(elem) {
            elem.set_tag("p")?;
        }
    }
    for elem in doc.findall(".//div") {
        let text = elem.text();
        if !is_blank(&text) {
            let p = new_p(ctx, text.as_deref().unwrap_or_default())?;
            elem.set_text(None)?;
            elem.insert(0, p)?;
        }
        for (pos, child) in elem.children().into_iter().enumerate().rev() {
            let tail = child.tail();
            if !is_blank(&tail) {
                let p = new_p(ctx, tail.as_deref().unwrap_or_default())?;
                child.set_tail(None)?;
                elem.insert(pos as isize + 1, p)?;
            }
            if child.tag() == "br" {
                child.drop_tree(ctx)?;
            }
        }
    }
    Ok(())
}
