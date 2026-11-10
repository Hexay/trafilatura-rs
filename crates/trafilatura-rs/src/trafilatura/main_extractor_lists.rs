//! main_extractor.py `handle_lists` and its nested-element helpers.
use super::htmlprocessing::{handle_textnode, process_node, truthy, Env};
use super::main_extractor::{is_text_element, update_elem_rendition};
use crate::dom::{Error, Node, Result};
use crate::py;

/// `add_sub_element`.
fn add_sub_element(new_child_elem: Node, subelem: Node, processed: Node) -> Result<()> {
    let sub = new_child_elem.sub_element(processed.tag())?;
    sub.set_text(processed.text().as_deref())?;
    sub.set_tail(processed.tail().as_deref())?;
    for (attr, _) in subelem.attrs() {
        sub.set(&attr, subelem.get(&attr).as_deref())?;
    }
    Ok(())
}

/// `process_nested_elements`.
fn process_nested_elements(env: &mut Env, child: Node, new_child_elem: Node) -> Result<()> {
    new_child_elem.set_text(child.text().as_deref())?;
    for subelem in child.iter_descendants(&["*"]) {
        if subelem.tag() == "list" {
            if let Some(processed) = handle_lists(env, subelem)? {
                new_child_elem.append(processed)?;
            }
        } else if let Some(processed) = handle_textnode(env, subelem, false, false)? {
            add_sub_element(new_child_elem, subelem, processed)?;
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
                new_child_elem.set_text(processed.text().as_deref())?;
                if let Some(tail) = processed.tail().filter(|t| !py::strip(t).is_empty()) {
                    let text = new_child_elem.text().ok_or_else(|| {
                        Error::Type("unsupported operand type(s) for +=: 'NoneType' and 'str'".into())
                    })?;
                    new_child_elem.set_text(Some(&format!("{text} {tail}")))?;
                }
                processed_element.append(new_child_elem)?;
            }
        } else {
            process_nested_elements(env, child, new_child_elem)?;
            if let Some(tail) = child.tail().filter(|t| !py::strip(t).is_empty()) {
                let kids: Vec<Node> = new_child_elem.children().into_iter().filter(|e| e.tag() != "done").collect();
                if let Some(&last) = kids.last() {
                    match last.tail().filter(|t| !py::strip(t).is_empty()) {
                        None => last.set_tail(Some(&tail))?,
                        Some(lt) => last.set_tail(Some(&format!("{lt} {tail}")))?,
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
