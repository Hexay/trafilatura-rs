//! lxml cleanup.pxi `strip_tags` / `strip_elements` (element argument; the root itself is never removed) and the
//! ElementPath subset trafilatura uses for `find` / `findall`.
use super::iter::{traverse_next, Tags};
use super::mutate::remove_node;
use super::raw::{self, P};
use super::{Ctx, Node, ELEMENT};
use libxml2_sys as ffi;
use std::ptr;

/// `_replaceNodeByChildren`: splice the children in place with raw links (adjacent text nodes are *not* merged).
unsafe fn replace_node_by_children(n: P) {
    if (*n).children.is_null() {
        ffi::xmlUnlinkNode(n);
        return;
    }
    let parent = (*n).parent;
    let mut c = (*n).children;
    while !c.is_null() {
        (*c).parent = parent;
        c = (*c).next;
    }
    if (*n).prev.is_null() {
        (*parent).children = (*n).children;
    } else {
        (*(*n).prev).next = (*n).children;
        (*(*n).children).prev = (*n).prev;
    }
    if (*n).next.is_null() {
        (*parent).last = (*n).last;
    } else {
        (*(*n).next).prev = (*n).last;
        (*(*n).last).next = (*n).next;
    }
    (*n).children = ptr::null_mut();
    (*n).last = ptr::null_mut();
    (*n).parent = ptr::null_mut();
    (*n).next = ptr::null_mut();
    (*n).prev = ptr::null_mut();
}

/// Runs `body` on every element of `top`'s subtree (inclusive), traversing like BEGIN_FOR_EACH_ELEMENT_FROM.
unsafe fn for_each_element(top: P, mut body: impl FnMut(P)) {
    let mut n = top;
    while !n.is_null() {
        if (*n).type_ == ELEMENT {
            body(n);
        }
        n = traverse_next(top, n);
    }
}

impl Node {
    /// `strip_tags(el, *tags)`.
    pub fn strip_tags(self, ctx: &Ctx, tags: &[&str]) {
        if tags.is_empty() {
            return;
        }
        let m = Tags::for_node(tags, self.raw());
        if m.rejects_all() {
            return;
        }
        unsafe {
            for_each_element(self.raw(), |n| {
                let mut c = raw::find_child_forwards(n, 0);
                while !c.is_null() {
                    if !m.matches(c) {
                        c = raw::next_element(c);
                        continue;
                    }
                    if (*c).type_ == ELEMENT {
                        let first = raw::find_child_forwards(c, 0);
                        let next = if first.is_null() { raw::next_element(c) } else { first };
                        replace_node_by_children(c);
                        ctx.orphan(c);
                        c = next;
                    } else {
                        let next = raw::next_element(c);
                        ffi::xmlUnlinkNode(c);
                        ctx.orphan(c);
                        c = next;
                    }
                }
            });
        }
    }

    /// `strip_elements(el, *tags, with_tail=...)`.
    pub fn strip_elements(self, ctx: &Ctx, tags: &[&str], with_tail: bool) {
        if tags.is_empty() {
            return;
        }
        let m = Tags::for_node(tags, self.raw());
        if m.rejects_all() {
            return;
        }
        unsafe {
            for_each_element(self.raw(), |n| {
                let mut c = raw::find_child_forwards(n, 0);
                while !c.is_null() {
                    let next = raw::next_element(c);
                    if m.matches(c) {
                        if (*c).type_ == ELEMENT {
                            if !with_tail {
                                ffi::xmlUnlinkNode(c);
                            }
                            remove_node(ctx, c);
                        } else {
                            if with_tail {
                                raw::remove_text((*c).next);
                            }
                            ffi::xmlUnlinkNode(c);
                            ctx.orphan(c);
                        }
                    }
                    c = next;
                }
            });
        }
    }

    /// `el.findall(path)` for `tag`, `*`, `./tag`, `.//tag`, `.//*` and `.//tag[@attr="value"]`.
    pub fn findall(self, path: &str) -> Vec<Node> {
        self.find_iter(path).collect()
    }

    pub fn find(self, path: &str) -> Option<Node> {
        self.find_iter(path).next()
    }

    fn find_iter(self, path: &str) -> impl Iterator<Item = Node> {
        let (descendants, rest) = match path.strip_prefix(".//") {
            Some(r) => (true, r),
            None => (false, path.strip_prefix("./").unwrap_or(path)),
        };
        let (tag, pred) = match rest.split_once('[') {
            Some((t, p)) => (t, Some(parse_attr_pred(p))),
            None => (rest, None),
        };
        assert!(!tag.contains('/') && !tag.contains('|'), "unsupported ElementPath {path:?}");
        let candidates: Box<dyn Iterator<Item = Node>> =
            if descendants { Box::new(self.iter_descendants(&[tag])) } else { Box::new(self.child_iter(tag)) };
        candidates.filter(move |n| pred.as_ref().is_none_or(|(k, v)| n.get(k).as_deref() == Some(v.as_str())))
    }

    fn child_iter(self, tag: &str) -> impl Iterator<Item = Node> {
        let m = Tags::for_node(&[tag], self.raw());
        self.children().into_iter().filter(move |c| unsafe { m.matches(c.raw()) })
    }
}

fn parse_attr_pred(p: &str) -> (String, String) {
    let body = p.strip_suffix(']').and_then(|b| b.strip_prefix('@')).expect("unsupported ElementPath predicate");
    let (k, v) = body.split_once('=').expect("only [@attr=\"value\"] predicates are supported");
    (k.to_owned(), v.trim_matches(|c| c == '"' || c == '\'').to_owned())
}
