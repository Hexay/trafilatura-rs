//! `lxml.sax.saxify` (lxml 6.1.3 src/lxml/sax.py `ElementTreeProducer`) reduced to the events a jusText handler reads.
//! HTML-parser trees carry no namespaces, so every name is `(None, tag)` and qname == tag; attributes are never
//! read by ParagraphMaker and building them cannot fail for str keys, so they are not produced.
use crate::dom::{Error, Node, Result, COMMENT_TAG, PI_TAG};

pub trait ContentHandler {
    fn start_document(&mut self) {}
    fn end_document(&mut self) {}
    fn start_element(&mut self, _local_name: &str) {}
    fn end_element(&mut self, _local_name: &str) {}
    fn characters(&mut self, _content: &str) {}
    fn processing_instruction(&mut self, _target: &str, _data: Option<&str>) {}
}

/// `saxify(element, handler)`: PI siblings before the element, the element, PI siblings after it.
pub fn saxify(element: Node, handler: &mut impl ContentHandler) -> Result<()> {
    handler.start_document();
    let mut before = Vec::new();
    let mut sibling = element.prev();
    while let Some(s) = sibling.filter(|s| s.tag() == PI_TAG) {
        before.push(s);
        sibling = s.prev();
    }
    for s in before.into_iter().rev() {
        recursive_saxify(s, handler)?;
    }
    recursive_saxify(element, handler)?;
    let mut sibling = element.next();
    while let Some(s) = sibling.filter(|s| s.tag() == PI_TAG) {
        recursive_saxify(s, handler)?;
        sibling = s.next();
    }
    handler.end_document();
    Ok(())
}

fn emit_nonempty(handler: &mut impl ContentHandler, text: Option<String>) {
    if let Some(t) = text.filter(|t| !t.is_empty()) {
        handler.characters(&t);
    }
}

fn recursive_saxify(element: Node, handler: &mut impl ContentHandler) -> Result<()> {
    let tag = element.tag();
    if tag == COMMENT_TAG || tag == PI_TAG {
        if tag == PI_TAG {
            // lxml passes element.target; ParagraphMaker ignores PIs, so the target is not reconstructed.
            handler.processing_instruction("", element.text().as_deref());
        }
        emit_nonempty(handler, element.tail());
        return Ok(());
    }
    if !element.is_element() {
        // _getNsTag(Entity) -> _utf8(<function>) raises TypeError.
        return Err(Error::Type("Argument must be bytes or unicode".to_owned()));
    }
    handler.start_element(tag);
    emit_nonempty(handler, element.text());
    for child in element.children() {
        recursive_saxify(child, handler)?;
    }
    handler.end_element(tag);
    emit_nonempty(handler, element.tail());
    Ok(())
}
