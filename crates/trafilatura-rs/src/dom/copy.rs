//! Native `xmlStaticCopyNode(node, doc, NULL, 1)` for the node kinds HTML trees hold (elements without namespaces,
//! text, CDATA, comments, PIs), observably identical to libxml2's copy:
//! - names stay the dict-interned pointers (the source's when the target dict owns them, as PATCHES.md's patch);
//! - text content is shared, not duplicated: nothing frees inside a `Ctx` arena and libxml2 never writes into a
//!   content buffer in place (appends go through `xmlRealloc`, which the arena always moves; see `alloc`);
//!   COMPACT-parsed short texts stored inline in the node are copied inline;
//! - attribute IDs are not registered in the target document (nothing reads its ID table).
//!
//! `FOREIGN` copies come from another libxml2 instance's tree (lxml's, see `copy_foreign`): content is duplicated
//! into the arena and the static text/comment names map to this libxml2's, as `xmlStaticCopyNode` maps its own.
//!
//! Returns null on anything else (namespaces, entity references, ...) so the caller can use libxml2 instead.
use super::alloc::x_malloc;
use super::raw::{self, P};
use super::{Ctx, Error, Node, Result, CDATA, COMMENT, ELEMENT, PI, TEXT};
use libxml2_sys as ffi;
use std::ffi::CStr;
use std::ptr;

unsafe fn new_node<T>() -> *mut T {
    let n = x_malloc(std::mem::size_of::<T>()) as *mut T;
    assert!(!n.is_null(), "out of memory");
    ptr::write_bytes(n, 0, 1);
    n
}

/// Interned name in the target dict: the same pointer when that dict owns it.
unsafe fn dict_name(dict: *mut ffi::xmlDict, name: *const ffi::xmlChar) -> *const ffi::xmlChar {
    if name.is_null() || dict.is_null() || ffi::xmlDictOwns(dict, name) == 1 {
        return name;
    }
    let n = ffi::xmlDictLookup(dict, name, -1);
    assert!(!n.is_null(), "out of memory");
    n
}

/// This libxml2's static name for a foreign text, CDATA or comment node's name.
unsafe fn static_name(dict: *mut ffi::xmlDict, name: *const ffi::xmlChar) -> *const ffi::xmlChar {
    if name.is_null() {
        return name;
    }
    match CStr::from_ptr(name.cast()).to_bytes() {
        b"text" => ffi::xmlStringText.as_ptr(),
        b"textnoenc" => ffi::xmlStringTextNoenc.as_ptr(),
        b"comment" => ffi::xmlStringComment.as_ptr(),
        _ => dict_name(dict, name),
    }
}

/// A childless copy of `src` (text-like or element without its attributes).
unsafe fn copy_shallow<const FOREIGN: bool>(src: P, doc: *mut ffi::xmlDoc, parent: P) -> P {
    let t = (*src).type_;
    let n: P = new_node();
    (*n).type_ = t;
    (*n).doc = doc;
    (*n).parent = parent;
    let dict = if doc.is_null() { ptr::null_mut() } else { (*doc).dict };
    // text/CDATA/comment names are libxml2's static strings, kept as is
    (*n).name = if matches!(t, ELEMENT | PI) {
        dict_name(dict, (*src).name)
    } else if FOREIGN {
        static_name(dict, (*src).name)
    } else {
        (*src).name
    };
    if t == ELEMENT {
        (*n).line = (*src).line;
    } else if !(*src).content.is_null() {
        let inline = ptr::addr_of_mut!((*src).properties) as *mut ffi::xmlChar;
        if (*src).content == inline {
            (*n).properties = (*src).properties;
            (*n).nsDef = (*src).nsDef;
            (*n).content = ptr::addr_of_mut!((*n).properties) as *mut ffi::xmlChar;
        } else if FOREIGN {
            (*n).content = ffi::xmlStrdup((*src).content);
            assert!(!(*n).content.is_null(), "out of memory");
        } else {
            (*n).content = (*src).content;
        }
    }
    n
}

unsafe fn supported(n: P) -> bool {
    match (*n).type_ {
        ELEMENT => (*n).ns.is_null() && (*n).nsDef.is_null(),
        TEXT | CDATA | COMMENT | PI => true,
        _ => false,
    }
}

/// Attribute list of `src` copied onto `target`; false on an unsupported attribute.
unsafe fn copy_attrs<const FOREIGN: bool>(src: P, target: P) -> bool {
    let doc = (*target).doc;
    let dict = if doc.is_null() { ptr::null_mut() } else { (*doc).dict };
    let mut a = (*src).properties;
    let mut prev: *mut ffi::xmlAttr = ptr::null_mut();
    while !a.is_null() {
        if !(*a).ns.is_null() {
            return false;
        }
        let c: *mut ffi::xmlAttr = new_node();
        (*c).type_ = ffi::xmlElementType_XML_ATTRIBUTE_NODE;
        (*c).name = dict_name(dict, (*a).name);
        (*c).parent = target;
        (*c).doc = doc;
        let mut v = (*a).children;
        let mut last: P = ptr::null_mut();
        while !v.is_null() {
            if !matches!((*v).type_, TEXT) {
                return false;
            }
            let t = copy_shallow::<FOREIGN>(v, doc, c as P);
            if last.is_null() {
                (*c).children = t;
            } else {
                (*last).next = t;
                (*t).prev = last;
            }
            last = t;
            v = (*v).next;
        }
        (*c).last = last;
        if prev.is_null() {
            (*target).properties = c;
        } else {
            (*prev).next = c;
            (*c).prev = prev;
        }
        prev = c;
        a = (*a).next;
    }
    true
}

/// Deep copy of `node` into `doc` (no parent, no siblings), or null when a node needs libxml2's copy.
pub(crate) unsafe fn copy_tree(node: P, doc: *mut ffi::xmlDoc) -> P {
    copy_subtree::<false>(node, doc)
}

unsafe fn copy_subtree<const FOREIGN: bool>(node: P, doc: *mut ffi::xmlDoc) -> P {
    if !supported(node) {
        return ptr::null_mut();
    }
    let root = copy_shallow::<FOREIGN>(node, doc, ptr::null_mut());
    if (*node).type_ != ELEMENT {
        return root;
    }
    if !copy_attrs::<FOREIGN>(node, root) {
        return ptr::null_mut();
    }
    let (mut cur, mut insert) = ((*node).children, root);
    while !cur.is_null() {
        if !supported(cur) {
            return ptr::null_mut();
        }
        let copy = copy_shallow::<FOREIGN>(cur, doc, insert);
        if (*cur).type_ == ELEMENT && !copy_attrs::<FOREIGN>(cur, copy) {
            return ptr::null_mut();
        }
        if (*insert).last.is_null() {
            (*insert).children = copy;
        } else {
            (*copy).prev = (*insert).last;
            (*(*insert).last).next = copy;
        }
        (*insert).last = copy;
        if (*cur).type_ == ELEMENT && !(*cur).children.is_null() {
            cur = (*cur).children;
            insert = copy;
            continue;
        }
        loop {
            if !(*cur).next.is_null() {
                cur = (*cur).next;
                break;
            }
            cur = (*cur).parent;
            insert = (*insert).parent;
            if cur == node {
                cur = ptr::null_mut();
                break;
            }
        }
    }
    root
}

/// `copy(el)` (`_copyDocRoot`, as `Node::deep_copy`) of an element of an HTML document owned by another libxml2
/// 2.14 instance, such as lxml's: the copy shares no memory with the source, which is only read. Unsupported for
/// anything `copy_tree` declines (namespaces, entity references) and for non-HTML documents.
///
/// # Safety
/// `src` must point to a live `xmlNode` whose tree nobody mutates during the call (hold the GIL for lxml's).
pub unsafe fn copy_foreign(ctx: &Ctx, src: *const std::ffi::c_void) -> Result<Node> {
    let src = src as P;
    let unsupported = |why: &str| Err(Error::Unsupported(format!("lxml tree: {why}")));
    if src.is_null() || (*src).type_ != ELEMENT {
        return unsupported("not an element");
    }
    let src_doc = (*src).doc;
    if src_doc.is_null() || (*src_doc).type_ != ffi::xmlElementType_XML_HTML_DOCUMENT_NODE {
        return unsupported("not an HTML document");
    }
    let doc = ffi::xmlCopyDoc(src_doc, 0);
    assert!(!doc.is_null(), "out of memory");
    ctx.adopt_doc(doc);
    let root = copy_subtree::<true>(src, doc);
    if root.is_null() {
        return unsupported("namespaces or entity references");
    }
    ffi::xmlDocSetRootElement(doc, root);
    let (mut tail, mut target) = (raw::text_node_or_skip((*src).next), root);
    while !tail.is_null() {
        target = ffi::xmlAddNextSibling(target, copy_shallow::<true>(tail, doc, ptr::null_mut()));
        tail = raw::text_node_or_skip((*tail).next);
    }
    Ok(Node::from_raw(root).expect("copied root"))
}
