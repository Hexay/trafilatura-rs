//! trafilatura 2.2.0 main_extractor.py `handle_lists` and `process_nested_elements`.
use super::htmlprocessing::{handle_textnode, process_node};
use super::main_extractor::{define_newelem, is_text_element, update_elem_rendition};
use super::settings::INLINE_CARRIED;
use crate::dom::{Node, Result};
use crate::py;
use crate::trafilatura::htmlprocessing::{truthy, Env};

/// `process_nested_elements`.
fn process_nested_elements(env: &mut Env, child: Node, new_child_elem: Node) -> Result<()> {
    new_child_elem.set_text(child.text().as_deref())?;
    for subelem in child.iter_descendants(&["*"]) {
        if subelem.tag() == "list" {
            if let Some(processed) = handle_lists(env, subelem)? {
                new_child_elem.append(processed)?;
            }
        } else if INLINE_CARRIED.contains(&subelem.tag()) {
            define_newelem(Some(subelem), new_child_elem, true)?;
        } else if let Some(processed) = handle_textnode(env, subelem, false, false)? {
            define_newelem(Some(processed), new_child_elem, false)?;
        }
        subelem.set_tag("done")?;
    }
    Ok(())
}

/// `x is not None and x.strip()`.
fn has_stripped_text(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|t| !py::strip(t).is_empty())
}

/// `handle_lists`.
pub fn handle_lists(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let processed_element = env.ctx.element(element.tag())?;
    if has_stripped_text(&element.text()) {
        let item = processed_element.sub_element("item")?;
        item.set_text(element.text().as_deref())?;
    }
    for child in element.iter_descendants(&["item"]) {
        let new_child_elem = env.ctx.element("item")?;
        if child.is_empty() {
            if let Some(processed) = process_node(env, child)? {
                let mut text = processed.text().unwrap_or_default();
                if let Some(tail) = processed.tail().filter(|t| !py::strip(t).is_empty()) {
                    text = format!("{text} {tail}");
                }
                new_child_elem.set_text(Some(&text))?;
                processed_element.append(new_child_elem)?;
            }
        } else {
            process_nested_elements(env, child, new_child_elem)?;
            if let Some(tail) = child.tail().filter(|t| !py::strip(t).is_empty()) {
                let kids: Vec<Node> = new_child_elem.children().into_iter().filter(|e| e.tag() != "done").collect();
                if let Some(&last) = kids.last() {
                    match last.tail().filter(|t| !py::strip(t).is_empty()) {
                        None => last.set_tail(Some(&tail))?,
                        Some(_) => last.set_tail(Some(&format!("{} {tail}", last.tail().unwrap_or_default())))?,
                    }
                }
            }
        }
        if truthy(&new_child_elem.text()) || !new_child_elem.is_empty() {
            update_elem_rendition(child, new_child_elem)?;
            processed_element.append(new_child_elem)?;
        }
        child.set_tag("done")?;
    }
    element.set_tag("done")?;
    if is_text_element(processed_element) {
        update_elem_rendition(element, processed_element)?;
        return Ok(Some(processed_element));
    }
    Ok(None)
}
