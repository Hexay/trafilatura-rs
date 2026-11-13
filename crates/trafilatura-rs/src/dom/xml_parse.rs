//! `etree.fromstring(text, XMLParser(remove_blank_text=True))` (trafilatura's CONTROL_PARSER) for str input.
use super::parse::{ignore_error, init_sax_document, intern_names};
use super::{Ctx, Error, Node, Result};
use libxml2_sys as ffi;
use std::os::raw::{c_char, c_int};
use std::ptr;

/// lxml's `_XML_DEFAULT_PARSE_OPTIONS` plus `remove_blank_text`.
const OPTIONS: c_int = (ffi::xmlParserOption_XML_PARSE_NOENT
    | ffi::xmlParserOption_XML_PARSE_NOCDATA
    | ffi::xmlParserOption_XML_PARSE_NONET
    | ffi::xmlParserOption_XML_PARSE_COMPACT
    | ffi::xmlParserOption_XML_PARSE_BIG_LINES
    | ffi::xmlParserOption_XML_PARSE_NOBLANKS) as c_int;

impl Ctx {
    /// The root element; `Err(Error::Syntax)` where lxml raises XMLSyntaxError. The text carries no encoding
    /// declaration, so its UTF-8 bytes parse as lxml's PEP-393 buffer does.
    pub fn parse_xml_noblanks(&self, text: &str) -> Result<Node> {
        unsafe {
            let ctxt = ffi::xmlNewParserCtxt();
            assert!(!ctxt.is_null(), "out of memory");
            ffi::xmlCtxtSetErrorHandler(ctxt, Some(ignore_error), ptr::null_mut());
            (*(*ctxt).sax).startDocument = Some(init_sax_document);
            if (*ctxt).dict != self.dict {
                ffi::xmlDictFree((*ctxt).dict);
                (*ctxt).dict = self.dict;
                ffi::xmlDictReference(self.dict);
            }
            (*ctxt).dictNames = 1;
            let doc = ffi::xmlCtxtReadMemory(
                ctxt,
                text.as_ptr() as *const c_char,
                text.len() as c_int,
                ptr::null(),
                c"UTF-8".as_ptr(),
                OPTIONS,
            );
            let well_formed = (*ctxt).wellFormed != 0;
            if !(*ctxt).myDoc.is_null() && (*ctxt).myDoc != doc {
                ffi::xmlFreeDoc((*ctxt).myDoc);
            }
            (*ctxt).myDoc = ptr::null_mut();
            ffi::xmlFreeParserCtxt(ctxt);
            if doc.is_null() || !well_formed {
                return Err(Error::Syntax("XMLSyntaxError".into()));
            }
            self.adopt_doc(doc);
            intern_names(self.dict, doc);
            Node::from_raw(ffi::xmlDocGetRootElement(doc)).ok_or_else(|| Error::Syntax("no root element".into()))
        }
    }
}
