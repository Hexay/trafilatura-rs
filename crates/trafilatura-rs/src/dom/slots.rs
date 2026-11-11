//! Text of a subtree grouped by lxml slot (an element's `.text`, an element's or comment's `.tail`), read in place.
use super::raw::{self, P};
use super::{cstr, Node, CDATA, ELEMENT, TEXT};
use std::collections::HashSet;

impl Node {
    /// Calls `f(owner, pieces)` for every text slot under `self` in document order (`self`'s text, then each
    /// descendant's text and tail; not `self`'s tail), as the tree would read after deleting every element in
    /// `removed` with `trafilatura.xml.delete_element(keep_tail=True)`: a removed element's subtree is skipped and
    /// its tail joins the slot before it. Every slot is reported, empty ones with no pieces.
    pub fn text_slots(self, removed: &HashSet<Node>, f: &mut dyn FnMut(Node, &[&str])) {
        let mut pieces = Vec::new();
        unsafe { walk(self.raw(), removed, &mut pieces, f) }
    }
}

unsafe fn walk<'a>(e: P, removed: &HashSet<Node>, pieces: &mut Vec<&'a str>, f: &mut dyn FnMut(Node, &[&str])) {
    let start = pieces.len();
    let mut owner = e;
    let mut c = (*e).children;
    while !c.is_null() {
        match (*c).type_ {
            TEXT | CDATA => pieces.push(cstr((*c).content)),
            ELEMENT if removed.contains(&Node::from_raw(c).expect("non-null")) => {}
            _ if raw::is_element(c) => {
                f(Node::from_raw(owner).expect("non-null"), &pieces[start..]);
                pieces.truncate(start);
                if (*c).type_ == ELEMENT {
                    walk(c, removed, pieces, f);
                }
                owner = c;
            }
            _ => {}
        }
        c = (*c).next;
    }
    f(Node::from_raw(owner).expect("non-null"), &pieces[start..]);
    pieces.truncate(start);
}
