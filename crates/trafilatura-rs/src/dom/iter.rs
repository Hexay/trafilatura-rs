//! lxml iterators with their exact laziness: each keeps the *next* matching node, computed when the current one
//! is handed out, so callers that mutate the tree mid-iteration (trafilatura does) see what Python sees.
use super::raw::{self, P};
use super::{cstr, Node, CDATA, ELEMENT, ENTITY_REF, TEXT};
use crate::py;
use libxml2_sys as ffi;
use std::ptr;

/// `_MultiTagMatcher`: empty = every element-like node; `"*"` = elements only; names = elements with that name.
/// Names are matched by interned pointer (as lxml's cacheTags does): every element name in a `Ctx` is interned
/// in its single dict (parse interns, and xmlNewDocNode / xmlNodeSetName / xmlStaticCopyNode use the doc dict).
#[derive(Clone)]
pub struct Tags {
    any_node: bool,
    any_element: bool,
    names: Vec<String>,
    dict: *mut ffi::xmlDict,
    interned: Vec<*const ffi::xmlChar>,
}

impl Tags {
    pub fn new(tags: &[&str]) -> Tags {
        Tags {
            any_node: tags.is_empty(),
            any_element: tags.contains(&"*"),
            names: tags.iter().filter(|t| **t != "*").map(|t| t.to_string()).collect(),
            dict: ptr::null_mut(),
            interned: Vec::new(),
        }
    }

    /// `new`, with names resolved against the dict of `node`'s document.
    pub(crate) fn for_node(tags: &[&str], node: P) -> Tags {
        let mut t = Tags::new(tags);
        unsafe {
            let doc = (*node).doc;
            if !doc.is_null() && !(*doc).dict.is_null() {
                t.dict = (*doc).dict;
                t.interned = t.names.iter().map(|n| ffi::xmlDictExists(t.dict, n.as_ptr(), n.len() as i32)).collect();
            }
        }
        t
    }

    /// No node can match: only named tags, none of which was ever interned in the dict, so no element carries
    /// any of these names.
    pub(crate) fn rejects_all(&self) -> bool {
        !self.any_node && !self.any_element && !self.dict.is_null() && self.interned.iter().all(|p| p.is_null())
    }

    pub(crate) unsafe fn matches(&self, n: P) -> bool {
        if self.any_node {
            return raw::is_element(n);
        }
        if (*n).type_ != ELEMENT {
            return false;
        }
        if self.any_element {
            return true;
        }
        let name = (*n).name;
        if !self.dict.is_null() {
            return self.interned.contains(&name);
        }
        self.names.iter().any(|t| t == cstr(name))
    }
}

/// `_LX__TRAVERSE_TO_NEXT` with only_elements.
pub(crate) unsafe fn traverse_next(top: P, mut n: P) -> P {
    let advance = |mut c: P| {
        while !c.is_null() && !raw::is_element(c) {
            c = (*c).next;
        }
        c
    };
    let mut next = (*n).children;
    if !next.is_null() {
        let t = (*n).type_;
        next = if t == ffi::xmlElementType_XML_ENTITY_REF_NODE || t == ffi::xmlElementType_XML_DTD_NODE {
            ptr::null_mut()
        } else {
            advance(next)
        };
    }
    if next.is_null() && n != top {
        next = advance((*n).next);
        while next.is_null() {
            n = (*n).parent;
            if n.is_null() || n == top || !raw::is_element(n) {
                break;
            }
            next = advance((*n).next);
        }
    }
    next
}

/// `el.iter(*tags)` / `el.iterdescendants(*tags)` (ElementDepthFirstIterator).
pub struct DepthFirst {
    top: P,
    next: P,
    tags: Tags,
}

impl Iterator for DepthFirst {
    type Item = Node;
    fn next(&mut self) -> Option<Node> {
        let current = self.next;
        if current.is_null() {
            return None;
        }
        unsafe {
            let mut n = traverse_next(self.top, current);
            while !n.is_null() && !self.tags.matches(n) {
                n = traverse_next(self.top, n);
            }
            self.next = n;
        }
        Node::from_raw(current)
    }
}

/// `SiblingsIterator` / `AncestorsIterator` (`_ElementMatchIterator`).
pub struct Stepping {
    next: P,
    step: unsafe fn(P) -> P,
    tags: Tags,
}

impl Stepping {
    fn new(from: P, step: unsafe fn(P) -> P, tags: Tags) -> Stepping {
        let mut it = Stepping { next: ptr::null_mut(), step, tags };
        it.store_next(from);
        it
    }

    fn store_next(&mut self, from: P) {
        unsafe {
            let mut n = (self.step)(from);
            while !n.is_null() && !self.tags.matches(n) {
                n = (self.step)(n);
            }
            self.next = n;
        }
    }
}

impl Iterator for Stepping {
    type Item = Node;
    fn next(&mut self) -> Option<Node> {
        let current = self.next;
        if current.is_null() {
            return None;
        }
        self.store_next(current);
        Node::from_raw(current)
    }
}

impl Node {
    pub fn iter(self, tags: &[&str]) -> DepthFirst {
        self.depth_first(tags, true)
    }

    pub fn iter_descendants(self, tags: &[&str]) -> DepthFirst {
        self.depth_first(tags, false)
    }

    fn depth_first(self, tags: &[&str], inclusive: bool) -> DepthFirst {
        let tags = Tags::for_node(tags, self.raw());
        let top = self.raw();
        if tags.rejects_all() {
            return DepthFirst { top, next: ptr::null_mut(), tags };
        }
        let mut it = DepthFirst { top, next: top, tags };
        if !inclusive || unsafe { !it.tags.matches(top) } {
            it.next();
        }
        it
    }

    pub fn iter_siblings(self, tags: &[&str], preceding: bool) -> Stepping {
        let step: unsafe fn(P) -> P = if preceding { raw::previous_element } else { raw::next_element };
        Stepping::new(self.raw(), step, Tags::for_node(tags, self.raw()))
    }

    pub fn iter_ancestors(self, tags: &[&str]) -> Stepping {
        Stepping::new(self.raw(), raw::parent_element, Tags::for_node(tags, self.raw()))
    }

    /// `el.itertext(with_tail=...)`: element text at "start", tails of descendants (comments/PIs included) after.
    pub fn itertext(self, with_tail: bool) -> Vec<String> {
        let mut out = Vec::new();
        unsafe { walk_text(self.raw(), self.raw(), with_tail, &mut out) };
        out
    }

    /// `lxml.html` `text_content()`: XPath `string()` of the element.
    pub fn text_content(self) -> String {
        let mut out = String::new();
        if unsafe { visit_text(self.raw(), &mut |s| out.push_str(s)) } {
            return out;
        }
        unsafe {
            let s = ffi::xmlXPathCastNodeToString(self.raw());
            let out = cstr(s).to_owned();
            ffi::xmlFree.expect("xmlFree")(s as *mut _);
            out
        }
    }

    /// `len(trim(el.text_content()))` without building either string.
    pub fn text_content_trim_len(self) -> usize {
        let mut count = py::TrimLen::default();
        if unsafe { !visit_text(self.raw(), &mut |s| count.feed(s)) } {
            return py::len(&py::Collapse::of(&self.text_content()));
        }
        count.finish()
    }

    /// `trim(el.text_content())` in one pass.
    pub fn text_content_trimmed(self) -> String {
        let mut c = py::Collapse::default();
        if unsafe { visit_text(self.raw(), &mut |s| c.push(s)) } {
            return c.finish();
        }
        py::Collapse::of(&self.text_content())
    }
}

/// Feeds the string-value of `n` (descendant text and CDATA in document order, as xmlBufGetNodeContent) to `f`.
/// Returns false when `n` is not an element or holds entity references (the HTML parser never creates them);
/// callers then discard what was fed and ask libxml2.
unsafe fn visit_text(n: P, f: &mut dyn FnMut(&str)) -> bool {
    (*n).type_ == ELEMENT && feed_text(n, f)
}

unsafe fn feed_text(n: P, f: &mut dyn FnMut(&str)) -> bool {
    let mut c = (*n).children;
    while !c.is_null() {
        match (*c).type_ {
            TEXT | CDATA => f(cstr((*c).content)),
            ELEMENT => {
                if !feed_text(c, f) {
                    return false;
                }
            }
            ENTITY_REF => return false,
            _ => {}
        }
        c = (*c).next;
    }
    true
}

unsafe fn walk_text(n: P, top: P, with_tail: bool, out: &mut Vec<String>) {
    if (*n).type_ == ELEMENT {
        if let Some(t) = raw::collect_text((*n).children) {
            out.push(t);
        }
        let mut c = (*n).children;
        while !c.is_null() {
            if raw::is_element(c) {
                walk_text(c, top, with_tail, out);
            }
            c = (*c).next;
        }
    }
    if with_tail && n != top {
        if let Some(t) = raw::collect_text((*n).next) {
            out.push(t);
        }
    }
}
