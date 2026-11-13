//! Tree mutations from etree.pyx / apihelpers.pxi. lxml's moveNodeToDocument is a no-op here: HTML trees carry
//! no namespaces and every document shares the Ctx dict. Removed nodes become Ctx orphans instead of being freed.
use super::raw::{self, P};
use super::tree::{check_tag, is_html_doc, make_element};
use super::{Ctx, Error, Node, Result};
use libxml2_sys as ffi;
use std::ptr;

unsafe fn link_child(parent: P, n: P) {
    (*n).parent = parent;
    if (*parent).children.is_null() {
        (*parent).children = n;
        (*parent).last = n;
    } else {
        (*n).prev = (*parent).last;
        (*(*parent).last).next = n;
        (*parent).last = n;
    }
    if (*n).doc != (*parent).doc {
        ffi::xmlSetTreeDoc(n, (*parent).doc);
    }
}

fn cycle(what: &str) -> Error {
    Error::Value(what.into())
}

impl Node {
    /// `_appendChild`.
    pub fn append(self, child: Node) -> Result<()> {
        unsafe {
            let n = child.raw();
            if raw::is_ancestor_or_same(n, self.raw()) {
                return Err(cycle("cannot append parent to itself"));
            }
            let next = (*n).next;
            ffi::xmlUnlinkNode(n);
            link_child(self.raw(), n);
            raw::move_tail(next, n);
        }
        Ok(())
    }

    pub fn extend(self, children: &[Node]) -> Result<()> {
        children.iter().try_for_each(|&c| self.append(c))
    }

    /// `el.insert(index, child)`.
    pub fn insert(self, index: isize, child: Node) -> Result<()> {
        unsafe {
            let at = raw::find_child(self.raw(), index);
            if at.is_null() {
                return self.append(child);
            }
            let n = child.raw();
            if raw::is_ancestor_or_same(n, self.raw()) {
                return Err(cycle("cannot append parent to itself"));
            }
            let next = (*n).next;
            ffi::xmlAddPrevSibling(at, n);
            raw::move_tail(next, n);
        }
        Ok(())
    }

    /// `parent.remove(child)`: the child leaves with its tail.
    pub fn remove(self, ctx: &Ctx, child: Node) -> Result<()> {
        unsafe {
            let n = child.raw();
            if (*n).parent != self.raw() {
                return Err(Error::Value("Element is not a child of this node.".into()));
            }
            let next = (*n).next;
            ffi::xmlUnlinkNode(n);
            raw::move_tail(next, n);
            ctx.orphan(n);
        }
        Ok(())
    }

    /// `parent.replace(old, new)`.
    pub fn replace(self, ctx: &Ctx, old: Node, new: Node) -> Result<()> {
        unsafe {
            let (o, n) = (old.raw(), new.raw());
            if (*o).parent != self.raw() {
                return Err(Error::Value("Element is not a child of this node.".into()));
            }
            if raw::is_ancestor_or_same(n, self.raw()) {
                return Err(cycle("cannot append parent to itself"));
            }
            let (old_next, new_next) = ((*o).next, (*n).next);
            ffi::xmlReplaceNode(o, n);
            raw::move_tail(new_next, n);
            raw::move_tail(old_next, o);
            ctx.orphan(o);
        }
        Ok(())
    }

    /// `el.clear(keep_tail)`.
    pub fn clear(self, ctx: &Ctx, keep_tail: bool) {
        unsafe {
            let n = self.raw();
            raw::remove_text((*n).children);
            if !keep_tail {
                raw::remove_text((*n).next);
            }
            self.clear_attrs();
            let mut c = (*n).children;
            if !c.is_null() && !raw::is_element(c) {
                c = raw::next_element(c);
            }
            while !c.is_null() {
                let next = raw::next_element(c);
                remove_node(ctx, c);
                c = next;
            }
        }
    }

    /// `el.addnext(other)` for non-root elements (`_appendSibling`).
    pub fn addnext(self, other: Node) -> Result<()> {
        self.add_sibling(other, true)
    }

    pub fn addprevious(self, other: Node) -> Result<()> {
        self.add_sibling(other, false)
    }

    fn add_sibling(self, other: Node, as_next: bool) -> Result<()> {
        unsafe {
            let (e, n) = (self.raw(), other.raw());
            let parent = (*e).parent;
            if !parent.is_null() && (*parent).type_ != ffi::xmlElementType_XML_ELEMENT_NODE {
                if !matches!((*n).type_, ffi::xmlElementType_XML_PI_NODE | ffi::xmlElementType_XML_COMMENT_NODE) {
                    return Err(Error::Type(
                        "Only processing instructions and comments can be siblings of the root element".into(),
                    ));
                }
                return Err(Error::Unsupported("PI/comment sibling of the root element".into()));
            }
            if raw::is_ancestor_or_same(n, e) {
                if e == n {
                    return Ok(());
                }
                return Err(cycle("cannot add ancestor as sibling, please break cycle first"));
            }
            let next = (*n).next;
            if as_next {
                let after = raw::next_element(e);
                if after.is_null() {
                    let mut last = e;
                    while !(*last).next.is_null() {
                        last = (*last).next;
                    }
                    ffi::xmlAddNextSibling(last, n);
                } else {
                    ffi::xmlAddPrevSibling(after, n);
                }
            } else {
                ffi::xmlAddPrevSibling(e, n);
            }
            raw::move_tail(next, n);
        }
        Ok(())
    }

    /// `copy(el)` / `deepcopy(el)` (`_copyDocRoot`): a new document holding a deep copy plus the copied tail.
    pub fn deep_copy(self, ctx: &Ctx) -> Node {
        unsafe {
            let doc = ffi::xmlCopyDoc(self.doc(), 0);
            ctx.adopt_doc(doc);
            let mut c = super::copy::copy_tree(self.raw(), doc);
            if c.is_null() {
                c = ffi::xmlDocCopyNode(self.raw(), doc, 1);
            }
            assert!(!c.is_null(), "out of memory");
            ffi::xmlDocSetRootElement(doc, c);
            raw::copy_tail((*self.raw()).next, c);
            Node::from_raw(c).expect("copied node")
        }
    }

    /// `SubElement(self, tag)` (`_makeSubElement`).
    pub fn sub_element(self, tag: &str) -> Result<Node> {
        let name = check_tag(is_html_doc(self.doc()), tag)?;
        unsafe {
            let n = ffi::xmlNewDocNode(self.doc(), ptr::null_mut(), name.as_ptr() as *const ffi::xmlChar, ptr::null());
            ffi::xmlAddChild(self.raw(), n);
            Ok(Node::from_raw(n).expect("out of memory"))
        }
    }
}

/// `_removeNode`.
pub(crate) unsafe fn remove_node(ctx: &Ctx, n: P) {
    let next = (*n).next;
    ffi::xmlUnlinkNode(n);
    raw::move_tail(next, n);
    ctx.orphan(n);
}

/// `lxml.html.Element(tag)`: new element in a fresh HTML document.
pub fn html_element(ctx: &Ctx, tag: &str) -> Result<Node> {
    make_element(ctx, tag, true)
}
