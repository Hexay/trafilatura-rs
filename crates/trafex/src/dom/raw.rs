//! Pointer-level helpers ported one-to-one from lxml's apihelpers.pxi (same names, snake_case).
use super::{cstr, Error, Result, CDATA, COMMENT, ELEMENT, ENTITY_REF, PI, TEXT};
use libxml2_sys as ffi;
use std::ffi::CString;
use std::ptr;

pub(crate) type P = *mut ffi::xmlNode;
const XINCLUDE_START: u32 = ffi::xmlElementType_XML_XINCLUDE_START;
const XINCLUDE_END: u32 = ffi::xmlElementType_XML_XINCLUDE_END;

pub(crate) unsafe fn is_element(n: P) -> bool {
    matches!((*n).type_, ELEMENT | COMMENT | ENTITY_REF | PI)
}

pub(crate) unsafe fn text_node_or_skip(mut n: P) -> P {
    while !n.is_null() {
        match (*n).type_ {
            TEXT | CDATA => return n,
            XINCLUDE_START | XINCLUDE_END => n = (*n).next,
            _ => return ptr::null_mut(),
        }
    }
    ptr::null_mut()
}

pub(crate) unsafe fn next_element(n: P) -> P {
    if n.is_null() {
        return n;
    }
    let mut c = (*n).next;
    while !c.is_null() && !is_element(c) {
        c = (*c).next;
    }
    c
}

pub(crate) unsafe fn previous_element(n: P) -> P {
    if n.is_null() {
        return n;
    }
    let mut c = (*n).prev;
    while !c.is_null() && !is_element(c) {
        c = (*c).prev;
    }
    c
}

pub(crate) unsafe fn parent_element(n: P) -> P {
    if n.is_null() || !is_element(n) {
        return ptr::null_mut();
    }
    let p = (*n).parent;
    if p.is_null() || !is_element(p) {
        return ptr::null_mut();
    }
    p
}

pub(crate) unsafe fn find_child_forwards(parent: P, index: usize) -> P {
    let mut c = (*parent).children;
    let mut i = 0;
    while !c.is_null() {
        if is_element(c) {
            if i == index {
                return c;
            }
            i += 1;
        }
        c = (*c).next;
    }
    ptr::null_mut()
}

pub(crate) unsafe fn find_child_backwards(parent: P, index: usize) -> P {
    let mut c = (*parent).last;
    let mut i = 0;
    while !c.is_null() {
        if is_element(c) {
            if i == index {
                return c;
            }
            i += 1;
        }
        c = (*c).prev;
    }
    ptr::null_mut()
}

/// Python-style index (negative counts from the end).
pub(crate) unsafe fn find_child(parent: P, index: isize) -> P {
    if index < 0 {
        find_child_backwards(parent, (-index - 1) as usize)
    } else {
        find_child_forwards(parent, index as usize)
    }
}

pub(crate) unsafe fn count_elements(mut n: P) -> usize {
    let mut count = 0;
    while !n.is_null() {
        if is_element(n) {
            count += 1;
        }
        n = (*n).next;
    }
    count
}

/// `_collectText`: None when there is no text node, "" when all are empty.
pub(crate) unsafe fn collect_text(n: P) -> Option<String> {
    let start = text_node_or_skip(n);
    let mut cur = start;
    let mut count = 0;
    let mut last_nonempty: *const ffi::xmlChar = ptr::null();
    while !cur.is_null() {
        if *(*cur).content != 0 {
            last_nonempty = (*cur).content;
        }
        count += 1;
        cur = text_node_or_skip((*cur).next);
    }
    if last_nonempty.is_null() {
        return if count > 0 { Some(String::new()) } else { None };
    }
    if count == 1 {
        return Some(cstr(last_nonempty).to_owned());
    }
    let mut out = String::new();
    let mut cur = start;
    while !cur.is_null() {
        out.push_str(cstr((*cur).content));
        cur = text_node_or_skip((*cur).next);
    }
    Some(out)
}

pub(crate) unsafe fn remove_text(n: P) {
    let mut cur = text_node_or_skip(n);
    while !cur.is_null() {
        let next = text_node_or_skip((*cur).next);
        ffi::xmlUnlinkNode(cur);
        ffi::xmlFreeNode(cur);
        cur = next;
    }
}

/// lxml `_utf8` + `_is_valid_xml_utf8`, including its byte-window quirk at the end of the string.
pub(crate) fn utf8(s: &str) -> Result<CString> {
    let b = s.as_bytes();
    let is_char = |c: u8| (0x9..=0xa).contains(&c) || c == 0xd || c >= 0x20;
    let invalid = || Error::Value("All strings must be XML compatible: Unicode or ASCII, no NULL bytes or control characters".into());
    let end = b.len();
    let mut i = 0;
    let mut next3: u32 = 0;
    if end >= 3 {
        next3 = ((b[0] as u32) << 8) | b[1] as u32;
    }
    while end >= 3 && i < end - 2 {
        next3 = 0x00ff_ffff & ((next3 << 8) | b[i + 2] as u32);
        if b[i] & 0x80 != 0 {
            if next3 == 0x00ef_bfbe || next3 == 0x00ef_bfbf || (0x00ed_a080..=0x00ed_bfbf).contains(&next3) {
                return Err(invalid());
            }
        } else if !is_char(b[i]) {
            return Err(invalid());
        }
        i += 1;
    }
    while i < end {
        if b[i] & 0x80 == 0 && !is_char(b[i]) {
            return Err(invalid());
        }
        i += 1;
    }
    Ok(CString::new(s).expect("NUL rejected above"))
}

pub(crate) unsafe fn create_text_node(doc: *mut ffi::xmlDoc, value: &str) -> Result<P> {
    let c = utf8(value)?;
    Ok(ffi::xmlNewDocText(doc, c.as_ptr() as *const ffi::xmlChar))
}

pub(crate) unsafe fn set_node_text(n: P, value: Option<&str>) -> Result<()> {
    // lxml removes the old text before validating the new one, so an invalid value leaves no text at all.
    remove_text((*n).children);
    let Some(v) = value else { return Ok(()) };
    let t = create_text_node((*n).doc, v)?;
    if (*n).children.is_null() {
        ffi::xmlAddChild(n, t);
    } else {
        ffi::xmlAddPrevSibling((*n).children, t);
    }
    Ok(())
}

pub(crate) unsafe fn set_tail_text(n: P, value: Option<&str>) -> Result<()> {
    remove_text((*n).next);
    let Some(v) = value else { return Ok(()) };
    let t = create_text_node((*n).doc, v)?;
    ffi::xmlAddNextSibling(n, t);
    Ok(())
}

pub(crate) unsafe fn move_tail(tail: P, mut target: P) {
    let mut cur = text_node_or_skip(tail);
    while !cur.is_null() {
        let next = text_node_or_skip((*cur).next);
        target = ffi::xmlAddNextSibling(target, cur);
        cur = next;
    }
}

pub(crate) unsafe fn copy_tail(tail: P, mut target: P) {
    let mut cur = text_node_or_skip(tail);
    while !cur.is_null() {
        let copy = if (*target).doc != (*cur).doc {
            ffi::xmlDocCopyNode(cur, (*target).doc, 0)
        } else {
            ffi::xmlCopyNode(cur, 0)
        };
        assert!(!copy.is_null(), "out of memory");
        target = ffi::xmlAddNextSibling(target, copy);
        cur = text_node_or_skip((*cur).next);
    }
}

pub(crate) unsafe fn is_ancestor_or_same(node: P, mut other: P) -> bool {
    while !other.is_null() {
        if other == node {
            return true;
        }
        other = (*other).parent;
    }
    false
}
