//! HTML parsing exactly as lxml 6.1.3 drives libxml2 (parser.pxi `_parseUnicodeDoc` / `_parseDoc`).
use super::iter::traverse_next;
use super::raw::{self, P};
use super::{Ctx, Error, Node, Result, ELEMENT};
use crate::py::SPACE_CLASS;
use libxml2_sys as ffi;
use regex::Regex;
use std::borrow::Cow;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::sync::LazyLock;

/// An lxml `HTMLParser` configuration.
#[derive(Clone, Copy, Debug)]
pub struct ParserOpts {
    pub options: c_int,
    pub remove_comments: bool,
    pub remove_pis: bool,
    /// `encoding=` of the parser: only applies to bytes input.
    pub default_encoding: Option<&'static str>,
}

const BASE: c_int = (ffi::htmlParserOption_HTML_PARSE_RECOVER
    | ffi::htmlParserOption_HTML_PARSE_NONET
    | ffi::htmlParserOption_HTML_PARSE_COMPACT) as c_int;

/// trafilatura.utils.HTML_PARSER: `HTMLParser(collect_ids=False, default_doctype=False, encoding='utf-8',
/// remove_comments=True, remove_pis=True)`.
pub const HTML_PARSER: ParserOpts = ParserOpts {
    options: BASE | ffi::htmlParserOption_HTML_PARSE_NODEFDTD as c_int,
    remove_comments: true,
    remove_pis: true,
    default_encoding: Some("utf-8"),
};

/// htmldate.utils.HTML_PARSER: trafilatura's without `remove_comments`.
pub const HTMLDATE_PARSER: ParserOpts = ParserOpts { remove_comments: false, ..HTML_PARSER };

/// `lxml.html.html_parser` (defaults), used by fragment_fromstring without a parser.
pub const LXML_HTML_PARSER: ParserOpts =
    ParserOpts { options: BASE, remove_comments: false, remove_pis: false, default_encoding: None };

static XML_ENCODING_DECL: LazyLock<Regex> = LazyLock::new(|| {
    let s = format!("[{SPACE_CLASS}]");
    Regex::new(&format!(r#"^(<\?xml[^>]+){s}+encoding{s}*={s}*["'][^"']*["']({s}*\?>|)"#)).unwrap()
});

/// lxml's startDocument hook (`_initSaxDocument`): share the parser dict with the new document.
unsafe extern "C" fn init_sax_document(ctxt: *mut c_void) {
    ffi::xmlSAX2StartDocument(ctxt);
    let c = ctxt as *mut ffi::xmlParserCtxt;
    let doc = (*c).myDoc;
    if !doc.is_null() && !(*c).dict.is_null() && (*doc).dict.is_null() {
        (*c).dictNames = 1;
        (*doc).dict = (*c).dict;
        ffi::xmlDictReference((*c).dict);
    }
}

/// lxml collects parse errors into an error log that trafilatura never reads.
unsafe extern "C" fn ignore_error(_: *mut c_void, _: *const ffi::xmlError) {}

/// CPython's PEP-393 buffer for `s` and the encoding lxml announces for it.
fn pep393_buffer(s: &str) -> (Cow<'_, [u8]>, &'static str) {
    // The widest code point follows from UTF-8 lead bytes: C2/C3 start U+0080..U+00FF, C4..EF start
    // U+0100..U+FFFF, F0..F4 start supplementary-plane code points.
    // libxml2 decodes a UTF-16/32 or Latin-1 buffer back to exactly these UTF-8 bytes, so feeding them directly
    // parses identically. Two exceptions keep the PEP-393 buffer: a leading U+FEFF (BOM handling depends on the
    // announced encoding) and C1 controls in a Latin-1-kind string (HTML maps ISO-8859-1 to windows-1252).
    let bytes = s.as_bytes();
    if s.is_ascii() {
        return (Cow::Borrowed(bytes), "UTF-8");
    }
    let wide = bytes.iter().any(|&b| b >= 0xc4);
    let c1 = || bytes.windows(2).any(|w| w[0] == 0xc2 && w[1] <= 0x9f);
    if !s.starts_with('\u{feff}') && (wide || !c1()) {
        return (Cow::Borrowed(bytes), "UTF-8");
    }
    let width = if bytes.iter().any(|&b| b >= 0xf0) {
        4
    } else if wide {
        2
    } else {
        1
    };
    match width {
        1 => {
            let mut out = Vec::with_capacity(s.len());
            out.extend(s.chars().map(|c| c as u8));
            (Cow::Owned(out), "ISO-8859-1")
        }
        2 => {
            let mut out = Vec::with_capacity(s.len() * 2);
            for u in s.encode_utf16() {
                out.extend_from_slice(&u.to_le_bytes());
            }
            (Cow::Owned(out), "UTF-16LE")
        }
        _ => {
            let mut out = Vec::with_capacity(s.len() * 4);
            for c in s.chars() {
                out.extend_from_slice(&(c as u32).to_le_bytes());
            }
            (Cow::Owned(out), "UTF-32LE")
        }
    }
}

impl Ctx {
    /// `etree.fromstring(text, parser)` for str input; returns the root element (None if the document is empty).
    pub fn parse_str(&self, text: &str, opts: ParserOpts) -> Result<Option<Node>> {
        if XML_ENCODING_DECL.is_match(text) {
            return Err(Error::Value(
                "Unicode strings with encoding declaration are not supported. Please use bytes input or XML \
                 fragments without declaration."
                    .into(),
            ));
        }
        let (buf, enc) = pep393_buffer(text);
        self.parse_buffer(&buf, Some(enc), opts)
    }

    /// `etree.fromstring(data, parser)` for bytes input (UTF-32 BOM sniffing as lxml does).
    pub fn parse_bytes(&self, data: &[u8], opts: ParserOpts) -> Result<Option<Node>> {
        let mut buf = data;
        let enc = match opts.default_encoding {
            Some(e) => Some(e),
            None if buf.starts_with(&[0xff, 0xfe, 0, 0]) => {
                buf = &buf[4..];
                Some("UTF-32LE")
            }
            None if buf.starts_with(&[0, 0, 0xfe, 0xff]) => {
                buf = &buf[4..];
                Some("UTF-32BE")
            }
            None => match unsafe { ffi::xmlDetectCharEncoding(buf.as_ptr(), buf.len() as c_int) } {
                ffi::xmlCharEncoding_XML_CHAR_ENCODING_UCS4LE => Some("UTF-32LE"),
                ffi::xmlCharEncoding_XML_CHAR_ENCODING_UCS4BE => Some("UTF-32BE"),
                _ => None,
            },
        };
        self.parse_buffer(buf, enc, opts)
    }

    fn parse_buffer(&self, buf: &[u8], enc: Option<&str>, opts: ParserOpts) -> Result<Option<Node>> {
        let enc_c = enc.map(|e| CString::new(e).unwrap());
        unsafe {
            let ctxt = ffi::htmlCreateMemoryParserCtxt(c"dummy".as_ptr(), 5);
            assert!(!ctxt.is_null(), "out of memory");
            ffi::xmlCtxtSetErrorHandler(ctxt, Some(ignore_error), ptr::null_mut());
            let sax = (*ctxt).sax;
            (*sax).startDocument = Some(init_sax_document);
            if opts.remove_comments {
                (*sax).comment = None;
            }
            if opts.remove_pis {
                (*sax).processingInstruction = None;
            }
            (*sax).cdataBlock = None;
            if (*ctxt).dict != self.dict {
                ffi::xmlDictFree((*ctxt).dict);
                (*ctxt).dict = self.dict;
                ffi::xmlDictReference(self.dict);
            }
            (*ctxt).dictNames = 1;
            let orig_options = (*ctxt).options;
            let doc = ffi::htmlCtxtReadMemory(
                ctxt,
                buf.as_ptr() as *const c_char,
                buf.len() as c_int,
                ptr::null(),
                enc_c.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                opts.options,
            );
            (*ctxt).options = orig_options;
            if !(*ctxt).myDoc.is_null() && (*ctxt).myDoc != doc {
                ffi::xmlFreeDoc((*ctxt).myDoc);
            }
            (*ctxt).myDoc = ptr::null_mut();
            ffi::htmlFreeParserCtxt(ctxt);
            if doc.is_null() {
                return Err(Error::Syntax("libxml2 returned no document".into()));
            }
            self.adopt_doc(doc);
            intern_names(self.dict, doc);
            if (*doc).encoding.is_null() {
                (*doc).encoding = ffi::xmlStrdup(c"UTF-8".as_ptr().cast());
            }
            Ok(Node::from_raw(ffi::xmlDocGetRootElement(doc)))
        }
    }
}

/// `_fixHtmlDictNames`: move element and attribute names into the dict (so `Node::tag` slices stay valid).
unsafe fn intern_names(dict: *mut ffi::xmlDict, doc: *mut ffi::xmlDoc) {
    let top = doc as P;
    let mut n = (*doc).children;
    while !n.is_null() && !raw::is_element(n) {
        n = (*n).next;
    }
    while !n.is_null() {
        if (*n).type_ == ELEMENT {
            intern(dict, &mut (*n).name);
            let mut a = (*n).properties;
            while !a.is_null() {
                intern(dict, &mut (*a).name);
                a = (*a).next;
            }
        }
        n = traverse_next(top, n);
    }
}

unsafe fn intern(dict: *mut ffi::xmlDict, name: &mut *const ffi::xmlChar) {
    // The parser already interns most names (dictNames=1); skip the hash for those.
    if ffi::xmlDictOwns(dict, *name) == 1 {
        return;
    }
    let interned = ffi::xmlDictLookup(dict, *name, -1);
    assert!(!interned.is_null(), "out of memory");
    if interned != *name {
        ffi::xmlFree.expect("xmlFree")(*name as *mut c_void);
        *name = interned;
    }
}
