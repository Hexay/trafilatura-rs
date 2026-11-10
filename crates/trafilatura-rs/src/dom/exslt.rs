//! lxml's `_ExsltRegExp.test` (extensions.pxi): `re:test(s, rexp[, flags])` backed by Python `re` semantics.
//! lxml registers it for `XPath(..., namespaces={"re": EXSLT_RE})`; it is registered here when the expression
//! uses the `re:` prefix.
use super::cstr;
use libxml2_sys as ffi;
use std::os::raw::c_int;

const EXSLT_RE: &[u8] = b"http://exslt.org/regular-expressions\0";

pub(crate) unsafe fn register(ctxt: *mut ffi::xmlXPathContext) {
    ffi::xmlXPathRegisterNs(ctxt, c"re".as_ptr().cast(), EXSLT_RE.as_ptr());
    ffi::xmlXPathRegisterFuncNS(ctxt, c"test".as_ptr().cast(), EXSLT_RE.as_ptr(), Some(re_test));
}

/// `_make_string`: a node-set argument is its *first* node in libxml2's node table order (not sorted), an
/// attribute/text node its value, an element its text content.
unsafe fn make_string(obj: *mut ffi::xmlXPathObject) -> String {
    match (*obj).type_ {
        ffi::xmlXPathObjectType_XPATH_NODESET | ffi::xmlXPathObjectType_XPATH_XSLT_TREE => {
            let set = (*obj).nodesetval;
            if set.is_null() || (*set).nodeNr == 0 {
                return String::new();
            }
            let n = *(*set).nodeTab;
            let s = ffi::xmlNodeGetContent(n);
            let out = cstr(s).to_owned();
            ffi::xmlFree.expect("xmlFree")(s as *mut _);
            out
        }
        ffi::xmlXPathObjectType_XPATH_BOOLEAN => (if (*obj).boolval != 0 { "True" } else { "False" }).to_owned(),
        ffi::xmlXPathObjectType_XPATH_STRING => cstr((*obj).stringval).to_owned(),
        _ => unimplemented!("re:test with a number argument"),
    }
}

unsafe fn pop_string(ctxt: ffi::xmlXPathParserContextPtr) -> String {
    let obj = ffi::xmlXPathValuePop(ctxt);
    let s = make_string(obj);
    ffi::xmlXPathFreeObject(obj);
    s
}

unsafe extern "C" fn re_test(ctxt: ffi::xmlXPathParserContextPtr, nargs: c_int) {
    if !(2..=3).contains(&nargs) {
        ffi::xmlXPathErr(ctxt, ffi::xmlXPathError_XPATH_INVALID_ARITY as c_int);
        return;
    }
    let flags = if nargs == 3 { pop_string(ctxt) } else { String::new() };
    let rexp = pop_string(ctxt);
    let s = pop_string(ctxt);
    let found = crate::py::re::search(&rexp, flags.contains('i'), &s);
    ffi::xmlXPathValuePush(ctxt, ffi::xmlXPathNewBoolean(found as c_int));
}
