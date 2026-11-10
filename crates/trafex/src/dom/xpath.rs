//! `el.xpath(expr)` / compiled `XPath(expr)(el)` through libxml2, unpacked like lxml's `_unwrapXPathObject`.
use super::fastpath::{plan, plan_first, Plan, VERIFY};
use super::raw;
use super::{cstr, Error, Node, Result};
use libxml2_sys as ffi;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

fn describe(nodes: &[Node]) -> String {
    let d = |n: &Node| format!("{}#{:?}.{:?}", n.tag(), n.get("id"), n.get("class"));
    nodes.iter().map(d).collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, PartialEq)]
pub enum XItem {
    Node(Node),
    /// Text, CDATA or attribute value (lxml's "smart string").
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum XValue {
    Nodes(Vec<XItem>),
    Bool(bool),
    Num(f64),
    Str(String),
}

struct Compiled(*mut ffi::xmlXPathCompExpr);

impl Drop for Compiled {
    fn drop(&mut self) {
        unsafe { ffi::xmlXPathFreeCompExpr(self.0) }
    }
}

/// Everything known about one expression string: libxml2's compiled form plus native plans for the full
/// node-set and for first-match use.
struct Entry {
    comp: Compiled,
    plan: Option<Plan>,
    first: Option<Plan>,
}

thread_local! {
    static CACHE: RefCell<HashMap<String, std::rc::Rc<Entry>>> = RefCell::new(HashMap::new());
}

fn entry(expr: &str) -> Result<std::rc::Rc<Entry>> {
    CACHE.with(|cache| {
        if let Some(e) = cache.borrow().get(expr) {
            return Ok(e.clone());
        }
        let c = CString::new(expr).map_err(|_| Error::Value("NUL in XPath".into()))?;
        // Cached across extractions, so it must not live in the per-Ctx arena.
        let comp = super::alloc::outside_arena(|| unsafe { ffi::xmlXPathCompile(c.as_ptr() as *const ffi::xmlChar) });
        if comp.is_null() {
            return Err(Error::Value(format!("Invalid expression: {expr}")));
        }
        let e = std::rc::Rc::new(Entry { comp: Compiled(comp), plan: plan(expr), first: plan_first(expr) });
        cache.borrow_mut().insert(expr.to_owned(), e.clone());
        Ok(e)
    })
}

impl Node {
    pub fn xpath(self, expr: &str) -> Result<XValue> {
        self.eval(expr, entry(expr)?.comp.0)
    }

    fn eval(self, expr: &str, comp: *mut ffi::xmlXPathCompExpr) -> Result<XValue> {
        unsafe {
            let ctxt = ffi::xmlXPathNewContext(self.doc());
            assert!(!ctxt.is_null(), "out of memory");
            (*ctxt).node = self.raw();
            if expr.contains("re:") {
                super::exslt::register(ctxt);
            }
            let obj = ffi::xmlXPathCompiledEval(comp, ctxt);
            ffi::xmlXPathFreeContext(ctxt);
            if obj.is_null() {
                return Err(Error::Value(format!("XPath evaluation failed: {expr}")));
            }
            let out = unwrap(obj);
            ffi::xmlXPathFreeObject(obj);
            Ok(out)
        }
    }

    /// Node-set result as elements, dropping strings (for expressions that only select elements).
    pub fn xpath_nodes(self, expr: &str) -> Result<Vec<Node>> {
        let e = entry(expr)?;
        let Some(plan) = &e.plan else { return self.lib_nodes(expr, &e) };
        let fast = plan.nodes(self);
        if *VERIFY {
            let lib = self.lib_nodes(expr, &e)?;
            assert!(fast == lib, "fast path differs from libxml2 for {expr}\nfast={}\nlib={}", describe(&fast), describe(&lib));
        }
        Ok(fast)
    }

    /// `next(iter(el.xpath(expr)), None)`, stopping at the first match when the expression has a native plan.
    pub fn xpath_first(self, expr: &str) -> Result<Option<Node>> {
        let e = entry(expr)?;
        let Some(plan) = &e.first else { return Ok(self.lib_nodes(expr, &e)?.into_iter().next()) };
        let fast = plan.first(self);
        if *VERIFY {
            assert_eq!(fast, self.lib_nodes(expr, &e)?.into_iter().next(), "fast first differs for {expr}");
        }
        Ok(fast)
    }

    fn lib_nodes(self, expr: &str, e: &Entry) -> Result<Vec<Node>> {
        Ok(match self.eval(expr, e.comp.0)? {
            XValue::Nodes(items) => items
                .into_iter()
                .filter_map(|i| match i {
                    XItem::Node(n) => Some(n),
                    XItem::Str(_) => None,
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    /// Node-set result as strings (text/attribute selections such as `//p//text()`).
    pub fn xpath_strings(self, expr: &str) -> Result<Vec<String>> {
        let e = entry(expr)?;
        if let Some(plan @ Plan::TextUnder { .. }) = &e.plan {
            let fast = plan.strings(self);
            if *VERIFY {
                assert_eq!(fast, self.lib_strings(expr, &e)?, "fast strings differ for {expr}");
            }
            return Ok(fast);
        }
        self.lib_strings(expr, &e)
    }

    fn lib_strings(self, expr: &str, e: &Entry) -> Result<Vec<String>> {
        Ok(match self.eval(expr, e.comp.0)? {
            XValue::Nodes(items) => items
                .into_iter()
                .filter_map(|i| match i {
                    XItem::Str(s) => Some(s),
                    XItem::Node(_) => None,
                })
                .collect(),
            _ => Vec::new(),
        })
    }
}

unsafe fn unwrap(obj: *mut ffi::xmlXPathObject) -> XValue {
    match (*obj).type_ {
        ffi::xmlXPathObjectType_XPATH_NODESET | ffi::xmlXPathObjectType_XPATH_XSLT_TREE => {
            let set = (*obj).nodesetval;
            let mut items = Vec::new();
            if !set.is_null() {
                for i in 0..(*set).nodeNr as usize {
                    let n = *(*set).nodeTab.add(i);
                    if raw::is_element(n) {
                        items.push(XItem::Node(Node::from_raw(n).expect("non-null")));
                    } else if matches!(
                        (*n).type_,
                        ffi::xmlElementType_XML_TEXT_NODE | ffi::xmlElementType_XML_CDATA_SECTION_NODE
                    ) {
                        items.push(XItem::Str(cstr((*n).content).to_owned()));
                    } else if (*n).type_ == ffi::xmlElementType_XML_ATTRIBUTE_NODE {
                        let s = ffi::xmlNodeGetContent(n);
                        items.push(XItem::Str(cstr(s).to_owned()));
                        ffi::xmlFree.expect("xmlFree")(s as *mut _);
                    }
                }
            }
            XValue::Nodes(items)
        }
        ffi::xmlXPathObjectType_XPATH_BOOLEAN => XValue::Bool((*obj).boolval != 0),
        ffi::xmlXPathObjectType_XPATH_NUMBER => XValue::Num((*obj).floatval),
        _ => XValue::Str(cstr((*obj).stringval).to_owned()),
    }
}
