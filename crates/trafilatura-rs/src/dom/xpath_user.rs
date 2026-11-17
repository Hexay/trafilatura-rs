//! `XPath(expr)` / `XPath(expr)(el)` for a caller's expression (trafilatura's `prune_xpath`), as lxml compiles,
//! evaluates and unwraps it: its XPathSyntaxError / XPathEvalError, and what a Python loop over the result sees.
use super::raw;
use super::{cstr, Error, Node, Result};
use libxml2_sys as ffi;
use std::ffi::CString;

/// One item of a node-set result, by the Python type lxml gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserItem {
    /// An element (or comment, PI, entity reference: lxml's `_isElement`).
    Node(Node),
    /// A text, CDATA or attribute node: `lxml.etree._ElementUnicodeResult`.
    SmartString,
    /// A namespace node: a (prefix, uri) tuple.
    Tuple,
}

/// The result of `XPath(expr)(el)` by Python type.
#[derive(Debug, Clone, PartialEq)]
pub enum UserValue {
    /// A list (document and XInclude nodes are left out, as lxml does).
    Items(Vec<UserItem>),
    Float,
    Bool,
    Str(String),
}

/// A compiled caller expression; `compile` fails as lxml's `XPath(expr)` does.
pub struct UserXPath(*mut ffi::xmlXPathCompExpr);

impl Drop for UserXPath {
    fn drop(&mut self) {
        unsafe { ffi::xmlXPathFreeCompExpr(self.0) }
    }
}

/// Keeps libxml2 from printing XPath errors to stderr (lxml collects them in its error log).
unsafe extern "C" fn ignore_error(_: *mut std::os::raw::c_void, _: *const ffi::xmlError) {}

unsafe fn new_context(doc: ffi::xmlDocPtr) -> ffi::xmlXPathContextPtr {
    let ctxt = ffi::xmlXPathNewContext(doc);
    assert!(!ctxt.is_null(), "out of memory");
    (*ctxt).error = Some(ignore_error);
    ctxt
}

/// libxml2's message for the context's last XPath error, as lxml's error log words it.
unsafe fn last_error(ctxt: ffi::xmlXPathContextPtr) -> String {
    const MESSAGES: [&str; 27] = [
        "Ok",
        "Number encoding",
        "Unfinished literal",
        "Start of literal",
        "Expected $ for variable reference",
        "Undefined variable",
        "Invalid predicate",
        "Invalid expression",
        "Missing closing curly brace",
        "Unregistered function",
        "Invalid operand",
        "Invalid type",
        "Invalid number of arguments",
        "Invalid context size",
        "Invalid context position",
        "Memory allocation error",
        "Syntax error",
        "Resource error",
        "Sub resource error",
        "Undefined namespace prefix",
        "Encoding error",
        "Char out of XML range",
        "Invalid or incomplete context",
        "Stack usage error",
        "Forbidden variable",
        "Operation limit exceeded",
        "Recursion limit exceeded",
    ];
    let code = (*ctxt).lastError.code - ffi::xmlParserErrors_XML_XPATH_EXPRESSION_OK as i32;
    match usize::try_from(code).ok().and_then(|c| MESSAGES.get(c)) {
        Some(m) if code != 0 => (*m).to_owned(),
        _ => "Error in xpath expression".to_owned(),
    }
}

impl UserXPath {
    pub fn compile(expr: &str) -> Result<UserXPath> {
        let c = CString::new(expr).map_err(|_| Error::Unsupported("NUL in an XPath expression".into()))?;
        unsafe {
            let ctxt = new_context(std::ptr::null_mut());
            let comp = ffi::xmlXPathCtxtCompile(ctxt, c.as_ptr() as *const ffi::xmlChar);
            let err = comp.is_null().then(|| last_error(ctxt));
            ffi::xmlXPathFreeContext(ctxt);
            match err {
                Some(msg) => Err(Error::Runtime(format!("XPathSyntaxError: {msg}"))),
                None => Ok(UserXPath(comp)),
            }
        }
    }

    pub fn eval(&self, node: Node) -> Result<UserValue> {
        unsafe {
            let ctxt = new_context(node.doc());
            (*ctxt).node = node.raw();
            let obj = ffi::xmlXPathCompiledEval(self.0, ctxt);
            let err = obj.is_null().then(|| last_error(ctxt));
            ffi::xmlXPathFreeContext(ctxt);
            if let Some(msg) = err {
                return Err(Error::Runtime(format!("XPathEvalError: {msg}")));
            }
            let out = unwrap(obj);
            ffi::xmlXPathFreeObject(obj);
            out
        }
    }
}

/// lxml's `_unwrapXPathObject` / `_createNodeSetResult`, keeping only each item's Python type.
unsafe fn unwrap(obj: *mut ffi::xmlXPathObject) -> Result<UserValue> {
    Ok(match (*obj).type_ {
        ffi::xmlXPathObjectType_XPATH_NODESET => {
            let set = (*obj).nodesetval;
            let mut items = Vec::new();
            for i in 0..if set.is_null() { 0 } else { (*set).nodeNr as usize } {
                let n = *(*set).nodeTab.add(i);
                items.push(match (*n).type_ {
                    _ if raw::is_element(n) => UserItem::Node(Node::from_raw(n).expect("non-null")),
                    ffi::xmlElementType_XML_TEXT_NODE
                    | ffi::xmlElementType_XML_CDATA_SECTION_NODE
                    | ffi::xmlElementType_XML_ATTRIBUTE_NODE => UserItem::SmartString,
                    ffi::xmlElementType_XML_NAMESPACE_DECL => UserItem::Tuple,
                    ffi::xmlElementType_XML_DOCUMENT_NODE
                    | ffi::xmlElementType_XML_HTML_DOCUMENT_NODE
                    | ffi::xmlElementType_XML_XINCLUDE_START
                    | ffi::xmlElementType_XML_XINCLUDE_END => continue,
                    other => return Err(Error::Unsupported(format!("XPath result node type {other}"))),
                });
            }
            UserValue::Items(items)
        }
        ffi::xmlXPathObjectType_XPATH_BOOLEAN => UserValue::Bool,
        ffi::xmlXPathObjectType_XPATH_NUMBER => UserValue::Float,
        ffi::xmlXPathObjectType_XPATH_STRING => UserValue::Str(cstr((*obj).stringval).to_owned()),
        other => return Err(Error::Unsupported(format!("XPath result type {other}"))),
    })
}
