//! `etree.tostring(el, encoding=str|'utf-8', method=...)` for elements (serializer.pxi `_tostring` with no XML
//! declaration, no complete-document output).
use super::raw;
use super::Node;
use libxml2_sys as ffi;
use std::os::raw::c_int;
use std::ptr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Xml,
    Html,
    Text,
}

impl Node {
    pub fn tostring(self, method: Method, with_tail: bool) -> String {
        match method {
            Method::Text => self.text_to_string(with_tail),
            _ => unsafe { self.dump(method == Method::Html, with_tail, false) },
        }
    }

    /// `etree.tostring(el, pretty_print=True, encoding=str)` (XML method, with tail).
    pub fn tostring_pretty(self) -> String {
        unsafe { self.dump(false, true, true) }
    }

    /// `_textToString`.
    fn text_to_string(self, with_tail: bool) -> String {
        unsafe {
            let buf = ffi::xmlBufferCreate();
            assert!(!buf.is_null(), "out of memory");
            ffi::xmlNodeBufGetContent(buf, self.raw());
            if with_tail {
                let mut t = raw::text_node_or_skip((*self.raw()).next);
                while !t.is_null() {
                    ffi::xmlBufferWriteChar(buf, (*t).content as *const std::os::raw::c_char);
                    t = raw::text_node_or_skip((*t).next);
                }
            }
            let bytes = std::slice::from_raw_parts(ffi::xmlBufferContent(buf), ffi::xmlBufferLength(buf) as usize);
            let out = String::from_utf8_lossy(bytes).into_owned();
            ffi::xmlBufferFree(buf);
            out
        }
    }

    /// `_writeNodeToBuffer` + `_writeTail`.
    unsafe fn dump(self, html: bool, with_tail: bool, pretty: bool) -> String {
        let format = c_int::from(pretty);
        let n = self.raw();
        let out = ffi::xmlAllocOutputBuffer(ptr::null_mut());
        assert!(!out.is_null(), "out of memory");
        let doc = (*n).doc;
        let parent = (*n).parent;
        let shallow = parent.is_null() || (*parent).type_ != ffi::xmlElementType_XML_DOCUMENT_NODE;
        let target = if shallow {
            let c = ffi::xmlCopyNode(n, 2);
            (*c).parent = parent;
            (*c).children = (*n).children;
            (*c).last = (*n).last;
            c
        } else {
            n
        };
        if html {
            ffi::htmlNodeDumpFormatOutput(out, doc, target, ptr::null(), format);
        } else {
            ffi::xmlNodeDumpOutput(out, doc, target, 0, format, ptr::null());
        }
        if shallow {
            (*target).children = ptr::null_mut();
            (*target).last = ptr::null_mut();
            ffi::xmlFreeNode(target);
        }
        if with_tail {
            let mut t = (*n).next;
            while !t.is_null()
                && matches!((*t).type_, ffi::xmlElementType_XML_TEXT_NODE | ffi::xmlElementType_XML_CDATA_SECTION_NODE)
            {
                if html {
                    ffi::htmlNodeDumpFormatOutput(out, (*t).doc, t, ptr::null(), format);
                } else {
                    ffi::xmlNodeDumpOutput(out, (*t).doc, t, 0, format, ptr::null());
                }
                t = (*t).next;
            }
        }
        if pretty {
            ffi::xmlOutputBufferWrite(out, 1, c"\n".as_ptr());
        }
        ffi::xmlOutputBufferFlush(out);
        let buf = if (*out).conv.is_null() { (*out).buffer } else { (*out).conv };
        let s = cstr_len(ffi::xmlBufContent(buf), ffi::xmlBufUse(buf));
        ffi::xmlOutputBufferClose(out);
        s
    }
}

unsafe fn cstr_len(p: *const ffi::xmlChar, len: usize) -> String {
    if p.is_null() {
        return String::new();
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(p, len)).into_owned()
}
