//! trafilatura/xml.py: tree helpers and the plain-text serializer `xmltotxt`.
use super::main_extractor_table::py_int;
use super::text::{sanitize, text_chars_test};
use super::xml_unescape::unescape;
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;

const NEWLINE_ELEMS: &[&str] = &["code", "graphic", "head", "lb", "list", "p", "quote", "row", "table"];
const SPECIAL_FORMATTING: &[&str] = &["del", "head", "hi", "ref"];
const NESTING_WHITELIST: &[&str] = &["cell", "figure", "item", "note", "quote"];
const HI_FORMATTING: &[(&str, &str)] = &[("#b", "**"), ("#i", "*"), ("#u", "__"), ("#t", "`")];

/// `delete_element`: remove with children, joining the tail to the previous element or parent.
pub fn delete_element(ctx: &Ctx, element: Node) -> Result<()> {
    let Some(parent) = element.parent() else { return Ok(()) };
    if let Some(tail) = element.tail().filter(|t| !t.is_empty()) {
        match element.prev() {
            None => parent.set_text(Some(&(parent.text().unwrap_or_default() + &tail)))?,
            Some(prev) => prev.set_tail(Some(&(prev.tail().unwrap_or_default() + &tail)))?,
        }
    }
    parent.remove(ctx, element)
}

/// `merge_with_parent`.
pub fn merge_with_parent(ctx: &Ctx, element: Node, include_formatting: bool) -> Result<()> {
    let Some(parent) = element.parent() else { return Ok(()) };
    let mut full_text = replace_element_text(element, include_formatting)?;
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

/// `remove_empty_elements`.
pub fn remove_empty_elements(ctx: &Ctx, tree: Node) -> Result<Node> {
    for element in tree.iter(&["*"]) {
        if element.is_empty()
            && !text_chars_test(element.text().as_deref())
            && !text_chars_test(element.tail().as_deref())
        {
            if let Some(parent) = element.parent() {
                if element.tag() != "graphic" && parent.tag() != "code" {
                    parent.remove(ctx, element)?;
                }
            }
        }
    }
    Ok(tree)
}

/// `strip_double_tags`.
pub fn strip_double_tags(ctx: &Ctx, tree: Node) -> Result<Node> {
    for elem in tree.xpath_nodes(".//head | .//code | .//p")?.into_iter().rev() {
        for subelem in elem.iter_descendants(&["code", "head", "p"]) {
            let parent_tag = subelem.parent().map(|p| p.tag());
            if subelem.tag() == elem.tag() && !parent_tag.is_some_and(|t| NESTING_WHITELIST.contains(&t)) {
                if parent_tag.is_none() {
                    return Err(Error::Runtime("AttributeError: 'NoneType' object has no attribute 'tag'".into()));
                }
                merge_with_parent(ctx, subelem, false)?;
            }
        }
    }
    Ok(tree)
}

/// `replace_element_text`.
pub fn replace_element_text(element: Node, include_formatting: bool) -> Result<String> {
    let text = element.text();
    let mut elem_text = text.clone().unwrap_or_default();
    let tag = element.tag();
    if include_formatting && text.as_deref().is_some_and(|t| !t.is_empty()) {
        match tag {
            "head" => {
                let number = match element.get("rend") {
                    None => 2,
                    Some(rend) => {
                        let second = rend
                            .chars()
                            .nth(1)
                            .ok_or_else(|| Error::Runtime("IndexError: string index out of range".into()))?;
                        py_int(&second.to_string()).ok().and_then(|n| usize::try_from(n).ok()).unwrap_or(2)
                    }
                };
                elem_text = format!("{} {elem_text}", "#".repeat(number));
            }
            "del" => elem_text = format!("~~{elem_text}~~"),
            "hi" => {
                let rend = element.get("rend");
                if let Some(&(_, md)) = HI_FORMATTING.iter().find(|(k, _)| Some(*k) == rend.as_deref()) {
                    elem_text = format!("{md}{elem_text}{md}");
                }
            }
            "code" => {
                elem_text =
                    if elem_text.contains('\n') { format!("```\n{elem_text}\n```") } else { format!("`{elem_text}`") };
            }
            _ => {}
        }
    }
    if tag == "ref" && !elem_text.is_empty() {
        let link_text = format!("[{elem_text}]");
        elem_text = match element.get("target").filter(|t| !t.is_empty()) {
            Some(target) => format!("{link_text}({target})"),
            None => link_text,
        };
    }
    if tag == "cell" && !elem_text.is_empty() && !element.is_empty() {
        if element.child(0).is_some_and(|c| c.tag() == "p") {
            elem_text = format!("{elem_text} ");
        }
    } else if tag == "item" && !elem_text.is_empty() {
        elem_text = format!("- {elem_text}\n");
    }
    Ok(elem_text)
}

fn in_cell(element: Node) -> Result<bool> {
    Ok(!element.xpath_nodes("ancestor::cell")?.is_empty())
}

/// `process_element`.
pub fn process_element(element: Node, out: &mut Vec<String>, include_formatting: bool) -> Result<()> {
    let text = element.text();
    if text.is_some() {
        out.push(replace_element_text(element, include_formatting)?);
    }
    for child in element.children() {
        process_element(child, out, include_formatting)?;
    }
    let tag = element.tag();
    let tail = element.tail();
    if text.is_none() && tail.is_none() {
        if tag == "graphic" {
            let t = format!("{} {}", element.get("title").unwrap_or_default(), element.get("alt").unwrap_or_default());
            out.extend(["![".into(), py::strip(&t).to_owned(), "]".into(), "(".into()]);
            out.extend([element.get("src").unwrap_or_default(), ")".into()]);
        } else if NEWLINE_ELEMS.contains(&tag) {
            if tag == "row" {
                let max_span = match element.get("span") {
                    Some(s) => py_int(&s)?,
                    None => 1,
                };
                let cell_count = element.xpath_nodes(".//cell")?.len() as i128;
                out.push("|".repeat(usize::try_from(max_span - cell_count).unwrap_or(0)) + "\n");
                if !element.xpath_nodes("./cell[@role='head']")?.is_empty() {
                    out.push(format!("\n{}\n", "---|".repeat(usize::try_from(max_span).unwrap_or(0))));
                }
            } else {
                out.push("\n".into());
            }
        } else if tag != "cell" {
            return Ok(());
        }
    }
    if tag == "p" && include_formatting && !in_cell(element)? {
        out.push("\n\u{2424}\n".into());
    } else if NEWLINE_ELEMS.contains(&tag) && !in_cell(element)? {
        out.extend(["\n".into(), "\n".into()]);
    } else if tag == "cell" {
        out.push(" | ".into());
    } else if tag == "comments" {
        out.push("\n\n".into());
    } else if !SPECIAL_FORMATTING.contains(&tag) {
        out.push(" ".into());
    }
    if let Some(tail) = tail {
        out.push(tail);
    }
    Ok(())
}

/// `xmltotxt`.
pub fn xmltotxt(xmloutput: Node, include_formatting: bool) -> Result<String> {
    let mut out = Vec::new();
    process_element(xmloutput, &mut out, include_formatting)?;
    Ok(unescape(&sanitize(&out.concat(), false, false).unwrap_or_default()))
}
