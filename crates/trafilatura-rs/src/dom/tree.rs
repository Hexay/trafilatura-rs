//! `_Element` accessors and mutations (etree.pyx), in lxml's libxml2 call order.
use super::raw::{self, P};
use super::{cstr, Ctx, Error, Node, Result, ELEMENT};
use libxml2_sys as ffi;
use std::ffi::CString;
use std::ptr;

fn opt(p: P) -> Option<Node> {
    Node::from_raw(p)
}

fn name_cstring(name: &str) -> Result<CString> {
    CString::new(name).map_err(|_| Error::Value(format!("Invalid tag name {name:?}")))
}

pub(crate) fn is_html_doc(doc: *mut ffi::xmlDoc) -> bool {
    unsafe { !doc.is_null() && (*doc).type_ == ffi::xmlElementType_XML_HTML_DOCUMENT_NODE }
}

/// `_htmlTagValidOrRaise` / `_tagValidOrRaise`, chosen by the owning document's parser kind as lxml does.
pub(crate) fn check_tag(html: bool, name: &str) -> Result<CString> {
    let c = name_cstring(name)?;
    let valid = if html {
        !name.is_empty() && !name.bytes().any(|b| b"&<>/\"'\t\n\x0B\x0C\r ".contains(&b))
    } else {
        // _pyXmlNameIsValid: a valid XML Name without ':' (prefixes go through {ns} notation in lxml)
        !name.contains(':') && unsafe { ffi::xmlValidateNameValue(c.as_ptr() as *const ffi::xmlChar) != 0 }
    };
    if valid {
        Ok(c)
    } else {
        Err(Error::Value(format!("Invalid tag name {name:?}")))
    }
}

/// `_makeElement` with doc=None, parser None (etree.Element) or an HTML parser (lxml.html.Element).
pub(crate) fn make_element(ctx: &Ctx, tag: &str, html: bool) -> Result<Node> {
    let name = check_tag(html, tag)?;
    unsafe {
        let doc = if html { ffi::htmlNewDoc(ptr::null(), ptr::null()) } else { new_xml_doc() };
        ctx.adopt_doc(doc);
        let n = ffi::xmlNewDocNode(doc, ptr::null_mut(), name.as_ptr() as *const ffi::xmlChar, ptr::null());
        ffi::xmlDocSetRootElement(doc, n);
        Ok(Node::from_raw(n).expect("out of memory"))
    }
}

/// `_newXMLDoc`.
pub(crate) unsafe fn new_xml_doc() -> *mut ffi::xmlDoc {
    let doc = ffi::xmlNewDoc(ptr::null());
    if (*doc).encoding.is_null() {
        (*doc).encoding = ffi::xmlStrdup(c"UTF-8".as_ptr().cast());
    }
    doc
}

impl Node {
    pub fn text(self) -> Option<String> {
        unsafe { raw::collect_text((*self.raw()).children) }
    }

    pub fn tail(self) -> Option<String> {
        unsafe { raw::collect_text((*self.raw()).next) }
    }

    pub fn set_text(self, value: Option<&str>) -> Result<()> {
        unsafe { raw::set_node_text(self.raw(), value) }
    }

    pub fn set_tail(self, value: Option<&str>) -> Result<()> {
        unsafe { raw::set_tail_text(self.raw(), value) }
    }

    /// `el.tag = name`.
    pub fn set_tag(self, name: &str) -> Result<()> {
        let c = check_tag(is_html_doc(self.doc()), name)?;
        unsafe {
            ffi::xmlNodeSetName(self.raw(), c.as_ptr() as *const ffi::xmlChar);
            (*self.raw()).ns = ptr::null_mut();
        }
        Ok(())
    }

    pub fn get(self, key: &str) -> Option<String> {
        if self.node_type() != ELEMENT {
            return None;
        }
        let k = CString::new(key).ok()?;
        unsafe {
            let v = ffi::xmlGetNsProp(self.raw(), k.as_ptr() as *const ffi::xmlChar, ptr::null());
            if v.is_null() {
                return None;
            }
            let s = cstr(v).to_owned();
            ffi::xmlFree.expect("xmlFree")(v as *mut _);
            Some(s)
        }
    }

    pub fn has_attr(self, key: &str) -> bool {
        let Ok(k) = CString::new(key) else { return false };
        unsafe { !ffi::xmlHasNsProp(self.raw(), k.as_ptr() as *const ffi::xmlChar, ptr::null()).is_null() }
    }

    /// `el.set(key, value)`; `None` (valueless attribute) is only accepted in HTML documents.
    pub fn set(self, key: &str, value: Option<&str>) -> Result<()> {
        let html = is_html_doc(self.doc());
        let k = if html { name_cstring(key)? } else { check_tag(false, key)? };
        let v = match value {
            Some(v) => Some(raw::utf8(v)?),
            None if html => None,
            None => return Err(Error::Type("Argument must be bytes or unicode, got 'NoneType'".into())),
        };
        unsafe {
            let vp = v.as_ref().map_or(ptr::null(), |c| c.as_ptr() as *const ffi::xmlChar);
            ffi::xmlSetNsProp(self.raw(), ptr::null_mut(), k.as_ptr() as *const ffi::xmlChar, vp);
        }
        Ok(())
    }

    /// `el.attrib.pop(key, None)`.
    pub fn pop_attr(self, key: &str) -> Option<String> {
        let value = self.get(key)?;
        let k = CString::new(key).ok()?;
        unsafe {
            let a = ffi::xmlHasNsProp(self.raw(), k.as_ptr() as *const ffi::xmlChar, ptr::null());
            if !a.is_null() {
                ffi::xmlRemoveProp(a);
            }
        }
        Some(value)
    }

    pub fn clear_attrs(self) {
        unsafe {
            let a = (*self.raw()).properties;
            if !a.is_null() {
                (*self.raw()).properties = ptr::null_mut();
                ffi::xmlFreePropList(a);
            }
        }
    }

    /// `el.attrib.items()` in document order (`keys()` is the first field).
    pub fn attrs(self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if self.node_type() != ELEMENT {
            return out;
        }
        unsafe {
            let mut a = (*self.raw()).properties;
            while !a.is_null() {
                if (*a).type_ == ffi::xmlElementType_XML_ATTRIBUTE_NODE {
                    let name = cstr((*a).name).to_owned();
                    out.push((name.clone(), self.get(&name).unwrap_or_default()));
                }
                a = (*a).next;
            }
        }
        out
    }

    pub fn has_attrs(self) -> bool {
        unsafe { self.node_type() == ELEMENT && !(*self.raw()).properties.is_null() }
    }

    pub fn parent(self) -> Option<Node> {
        unsafe { opt(raw::parent_element(self.raw())) }
    }

    pub fn next(self) -> Option<Node> {
        unsafe { opt(raw::next_element(self.raw())) }
    }

    pub fn prev(self) -> Option<Node> {
        unsafe { opt(raw::previous_element(self.raw())) }
    }

    /// `el.getroottree().getroot()`: the root of the node's document, which moves with the node on append and
    /// stays when it is removed.
    pub fn getroot(self) -> Option<Node> {
        unsafe { opt(ffi::xmlDocGetRootElement(self.doc())) }
    }

    /// `len(el)`: element-like children, comments included.
    pub fn len(self) -> usize {
        unsafe { raw::count_elements((*self.raw()).children) }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// `el[index]` with Python negative indexing.
    pub fn child(self, index: isize) -> Option<Node> {
        unsafe { opt(raw::find_child(self.raw(), index)) }
    }

    pub fn children(self) -> Vec<Node> {
        let mut out = Vec::new();
        let mut c = self.child(0);
        while let Some(n) = c {
            out.push(n);
            c = n.next();
        }
        out
    }

    /// `parent.index(child)`.
    pub fn index(self, child: Node) -> Result<usize> {
        self.children().iter().position(|&c| c == child).ok_or_else(|| Error::Value("Element is not in list".into()))
    }
}
