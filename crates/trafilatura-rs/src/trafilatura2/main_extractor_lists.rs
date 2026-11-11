//! trafilatura 2.3.1 main_extractor.py `handle_lists`, `process_nested_elements` and `_append_block`.
use super::htmlprocessing::{handle_textnode, process_node};
use super::main_extractor::{define_newelem, handle_paragraphs, is_text_element, update_elem_rendition, PotentialTags};
use super::main_extractor_table::handle_image;
use super::settings::{INLINE_CARRIED, TAG_CATALOG};
use crate::dom::{Node, Result};
use crate::py;
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::text::text_chars_test;

/// `_append_block`: a processed block with the source tail, or that tail kept as text after the parent's content.
pub fn append_block(parent: Node, processed: Option<Node>, source: Node) -> Result<()> {
    if let Some(p) = processed {
        p.set_tail(source.tail().as_deref())?;
        return parent.append(p);
    }
    let Some(tail) = source.tail().filter(|t| text_chars_test(Some(t))) else { return Ok(()) };
    let last = parent.child(-1);
    let before = match last {
        None => parent.text(),
        Some(l) => l.tail(),
    }
    .unwrap_or_default();
    let ends_space = before.chars().next_back().is_some_and(py::is_space);
    let starts_space = tail.chars().next().is_some_and(py::is_space);
    // a dropped block still separates the text around it, a dropped image does not
    let sep = source.tag() != "graphic" && (!before.is_empty() || last.is_some()) && !ends_space && !starts_space;
    let joined = format!("{before}{}{tail}", if sep { " " } else { "" });
    match last {
        None => parent.set_text(Some(&joined)),
        Some(l) => l.set_tail(Some(&joined)),
    }
}

/// `process_nested_elements`: rewire an element's descendants into `new_child_elem`.
pub fn process_nested_elements(env: &mut Env, child: Node, new_child_elem: Node) -> Result<()> {
    let quote_tags: PotentialTags = TAG_CATALOG.iter().copied().chain(["ref", "graphic"]).collect();
    new_child_elem.set_text(child.text().as_deref())?;
    for subelem in child.iter_descendants(&["*"]) {
        let tag = subelem.tag();
        if tag == "list" {
            let processed = handle_lists(env, subelem)?;
            append_block(new_child_elem, processed, subelem)?;
        } else if tag == "p" && !subelem.is_empty() {
            let processed = handle_paragraphs(env, subelem, &quote_tags)?;
            append_block(new_child_elem, processed, subelem)?;
        } else if tag == "graphic" {
            let processed = handle_image(env.ctx, subelem)?;
            append_block(new_child_elem, processed, subelem)?;
        } else if INLINE_CARRIED.contains(&tag) {
            define_newelem(Some(subelem), new_child_elem, true)?;
        } else {
            if let Some(tail) = subelem.tail().filter(|t| text_chars_test(Some(t))) {
                if let Some(last) = subelem.child(-1) {
                    last.set_tail(Some(&(last.tail().unwrap_or_default() + &tail)))?;
                    subelem.set_tail(None)?;
                }
            }
            let processed = handle_textnode(env, subelem, false, false)?;
            define_newelem(processed, new_child_elem, false)?;
        }
        subelem.set_tag("done")?;
    }
    Ok(())
}

/// `handle_lists`.
pub fn handle_lists(env: &mut Env, element: Node) -> Result<Option<Node>> {
    let processed_element = env.ctx.element(element.tag())?;
    if text_chars_test(element.text().as_deref()) {
        let item = processed_element.sub_element("item")?;
        item.set_text(element.text().as_deref())?;
    }
    for child in element.iter_descendants(&["item"]) {
        // items of a nested list are renamed by the recursive call after lxml fetched the next one
        if child.tag() == "done" {
            continue;
        }
        let new_child_elem = env.ctx.element("item")?;
        if child.is_empty() {
            if let Some(processed) = process_node(env, child)? {
                let parts: Vec<String> =
                    [processed.text(), processed.tail()].into_iter().flatten().filter(|t| !t.is_empty()).collect();
                new_child_elem.set_text(Some(&parts.join(" ")))?;
            }
        } else {
            process_nested_elements(env, child, new_child_elem)?;
            if let Some(tail) = child.tail().filter(|t| text_chars_test(Some(t))) {
                if let Some(last) = new_child_elem.child(-1) {
                    let joined = match last.tail().filter(|t| text_chars_test(Some(t))) {
                        Some(lt) => format!("{lt} {tail}"),
                        None => tail,
                    };
                    last.set_tail(Some(&joined))?;
                }
            }
        }
        if is_text_element(new_child_elem) || new_child_elem.iter(&["graphic"]).next().is_some() {
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
