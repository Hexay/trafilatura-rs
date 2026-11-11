//! lxml 6.1.3's element model over libxml2 2.14.6 nodes. Every mutating operation replays lxml's libxml2 call
//! sequence (lxml-6.1.3 src/lxml/apihelpers.pxi, etree.pyx), so trees, text-node merging and serialization stay
//! byte-identical to Python.
//!
//! Ownership: a `Ctx` owns one name dict shared by all its documents (lxml's per-thread global dict), every
//! document created in it, and every node detached from a tree. All of it lives in the thread's libxml2 arena
//! (see `alloc`) until the last `Ctx` on the thread drops, so a `Node` handle stays valid for the `Ctx`'s whole
//! life; using it after that is undefined behaviour. Create and drop a `Ctx` on the same thread.

mod alloc;
mod cleanup;
mod copy;
mod error;
mod exslt;
mod fastpath;
mod html;
mod iter;
mod mutate;
mod parse;
mod raw;
mod serialize;
mod tree;
mod xpath;

pub use error::{Error, Result};
pub use html::{document_fromstring, element as html_element, fragment_fromstring, fromstring, Input};
pub use iter::{DepthFirst, Stepping};
pub use parse::{ParserOpts, HTML_PARSER, LXML_HTML_PARSER};
pub use serialize::Method;
pub use xpath::{XItem, XValue};

use libxml2_sys as ffi;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::ptr::NonNull;

pub struct Ctx {
    dict: *mut ffi::xmlDict,
    /// Keeps `Ctx` !Send: its arena is thread-local.
    _thread_bound: PhantomData<*mut ()>,
}

impl Ctx {
    pub fn new() -> Ctx {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| unsafe {
            alloc::install();
            ffi::xmlInitParser();
        });
        alloc::enter();
        let dict = unsafe { ffi::xmlDictCreate() };
        assert!(!dict.is_null(), "xmlDictCreate failed");
        Ctx { dict, _thread_bound: PhantomData }
    }

    pub(crate) fn adopt_doc(&self, doc: *mut ffi::xmlDoc) {
        unsafe {
            if (*doc).dict.is_null() {
                (*doc).dict = self.dict;
                ffi::xmlDictReference(self.dict);
            }
        }
    }

    /// Marks a node as detached. Nothing to do: it stays in the arena until the `Ctx` drops.
    #[inline]
    pub(crate) fn orphan(&self, _node: *mut ffi::xmlNode) {}

    /// `etree.Element(tag)`: a new element as the root of a fresh XML document (`_newXMLDoc`).
    pub fn element(&self, tag: &str) -> Result<Node> {
        tree::make_element(self, tag, false)
    }
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx::new()
    }
}

impl Drop for Ctx {
    /// Documents, detached nodes and the dict are all arena memory: no per-node teardown.
    fn drop(&mut self) {
        alloc::leave();
    }
}

/// Handle to an lxml "element": an element, comment, PI or entity-reference node (lxml's `_isElement`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Node(NonNull<ffi::xmlNode>);

pub(crate) const ELEMENT: u32 = ffi::xmlElementType_XML_ELEMENT_NODE;
pub(crate) const TEXT: u32 = ffi::xmlElementType_XML_TEXT_NODE;
pub(crate) const CDATA: u32 = ffi::xmlElementType_XML_CDATA_SECTION_NODE;
pub(crate) const ENTITY_REF: u32 = ffi::xmlElementType_XML_ENTITY_REF_NODE;
pub(crate) const PI: u32 = ffi::xmlElementType_XML_PI_NODE;
pub(crate) const COMMENT: u32 = ffi::xmlElementType_XML_COMMENT_NODE;

/// Tag sentinels for non-element "elements"; lxml returns factory functions there, which never equal a str.
pub const COMMENT_TAG: &str = "<!--comment-->";
pub const PI_TAG: &str = "<?pi?>";
pub const ENTITY_TAG: &str = "&entity;";

impl Node {
    pub(crate) fn from_raw(p: *mut ffi::xmlNode) -> Option<Node> {
        NonNull::new(p).map(Node)
    }

    pub(crate) fn raw(self) -> *mut ffi::xmlNode {
        self.0.as_ptr()
    }

    pub(crate) fn node_type(self) -> u32 {
        unsafe { (*self.raw()).type_ }
    }

    /// `isinstance(el.tag, str)`: a real element, not a comment/PI/entity.
    pub fn is_element(self) -> bool {
        self.node_type() == ELEMENT
    }

    /// Element name. Names live in the `Ctx` dict (see `parse::intern_names`), so the slice outlives any rename.
    pub fn tag(self) -> &'static str {
        match self.node_type() {
            ELEMENT => unsafe { cstr((*self.raw()).name) },
            COMMENT => COMMENT_TAG,
            PI => PI_TAG,
            _ => ENTITY_TAG,
        }
    }

    pub(crate) fn doc(self) -> *mut ffi::xmlDoc {
        unsafe { (*self.raw()).doc }
    }
}

/// UTF-8 C string from libxml2 as `&str`; libxml2 only stores validated UTF-8.
pub(crate) unsafe fn cstr<'a>(p: *const ffi::xmlChar) -> &'a str {
    if p.is_null() {
        return "";
    }
    let bytes = CStr::from_ptr(p as *const std::os::raw::c_char).to_bytes();
    debug_assert!(std::str::from_utf8(bytes).is_ok(), "libxml2 stores UTF-8");
    std::str::from_utf8_unchecked(bytes)
}
