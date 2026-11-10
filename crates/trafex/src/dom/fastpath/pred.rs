//! One location step of the recognised grammar and its native matcher:
//!   step  := .//name | .//name[preds] | .//*[preds] | .//*[tags][preds]
//!   tags  := self::a or self::b ...  (optionally parenthesised)
//!   preds := atom (or atom)*
//!   atom  := contains(V, "s") | starts-with(V, "s") | @a="v" | @a
//!          | contains(U, "s") | starts-with(U, "s") | re:test(U, 'pattern'[, 'flags'])
//!   V := @a | translate(@a, "ABC", "abc")      U := @a | @a|@b ...
//! A union U is converted to a string as libxml2 does for function arguments (sorted node-set): the value of the
//! first listed attribute in the element's attribute order, "" if none. `re:test` is Python `re.search` (lxml).
//! Values are compared as UTF-8 bytes, as libxml2's xmlStrstr / xmlStrncmp / xmlStrEqual do.
use super::super::alloc;
use super::super::raw::P;
use super::super::{cstr, ELEMENT};
pub(crate) use super::pred_parse::parse_step;
use aho_corasick::AhoCorasick;
use libxml2_sys as ffi;
use regex::Regex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;

/// Tests on one (attribute, translate map) pair.
pub(crate) struct AttrTests {
    pub name: Vec<u8>,
    pub map: Option<[u8; 256]>,
    pub contains: Option<AhoCorasick>,
    pub starts: Vec<Vec<u8>>,
    pub equals: Vec<Vec<u8>>,
    pub present: bool,
}

pub(crate) enum UnionKind {
    Contains(Vec<u8>),
    StartsWith(Vec<u8>),
    Regex(Rc<Regex>),
}

/// A test on the string value of an attribute union (a single attribute for `re:test(@a, ...)`).
pub(crate) struct UnionTest {
    pub names: Vec<Vec<u8>>,
    pub kind: UnionKind,
}

pub(crate) struct Step {
    tags: Option<Vec<String>>,
    tests: Vec<AttrTests>,
    unions: Vec<UnionTest>,
    tag_only: bool,
    /// Names resolved in the current `Ctx` dict (all names there are interned), so matching compares pointers.
    resolved: RefCell<Resolved>,
}

struct Resolved {
    dict: *mut ffi::xmlDict,
    generation: u64,
    /// Dict size at resolution when some name was absent; renames (`set_tag`) can intern it later.
    missing_at: Option<i32>,
    tags: Vec<*const ffi::xmlChar>,
    attrs: Vec<*const ffi::xmlChar>,
    unions: Vec<Vec<*const ffi::xmlChar>>,
}

unsafe fn look(dict: *mut ffi::xmlDict, s: &[u8]) -> *const ffi::xmlChar {
    ffi::xmlDictExists(dict, s.as_ptr(), s.len() as i32)
}

impl Resolved {
    unsafe fn new(dict: *mut ffi::xmlDict, step: &Step) -> Resolved {
        let mut r = Resolved {
            dict,
            generation: alloc::generation(),
            missing_at: None,
            tags: step.tags.iter().flatten().map(|t| look(dict, t.as_bytes())).collect(),
            attrs: step.tests.iter().map(|t| look(dict, &t.name)).collect(),
            unions: step.unions.iter().map(|u| u.names.iter().map(|n| look(dict, n)).collect()).collect(),
        };
        let missing = r.tags.iter().chain(&r.attrs).chain(r.unions.iter().flatten()).any(|p| p.is_null());
        r.missing_at = missing.then(|| ffi::xmlDictSize(dict));
        r
    }

    unsafe fn stale(&self, dict: *mut ffi::xmlDict) -> bool {
        self.dict != dict
            || self.generation != alloc::generation()
            || self.missing_at.is_some_and(|size| size != ffi::xmlDictSize(dict))
    }
}

/// Attribute value bytes as XPath sees them (borrowed when the value is a single text node).
unsafe fn value_of<'a>(n: P, a: *mut ffi::xmlAttr) -> Cow<'a, [u8]> {
    let c = (*a).children;
    if c.is_null() {
        return Cow::Borrowed(&[]);
    }
    if (*c).next.is_null() && (*c).type_ == ffi::xmlElementType_XML_TEXT_NODE {
        return Cow::Borrowed(cstr((*c).content).as_bytes());
    }
    let s = ffi::xmlNodeListGetString((*n).doc, c, 1);
    let owned = cstr(s).as_bytes().to_vec();
    ffi::xmlFree.expect("xmlFree")(s as *mut _);
    Cow::Owned(owned)
}

/// Value of the first attribute (in the element's attribute order) whose name satisfies `is_name`.
unsafe fn first_attr<'a>(n: P, is_name: impl Fn(*const ffi::xmlChar) -> bool) -> Option<Cow<'a, [u8]>> {
    let mut a = (*n).properties;
    while !a.is_null() {
        if (*a).ns.is_null() && is_name((*a).name) {
            return Some(value_of(n, a));
        }
        a = (*a).next;
    }
    None
}

impl Step {
    pub(crate) fn new(
        tags: Option<Vec<String>>,
        tests: Vec<AttrTests>,
        unions: Vec<UnionTest>,
        tag_only: bool,
    ) -> Step {
        let resolved = Resolved {
            dict: ptr::null_mut(),
            generation: 0,
            missing_at: None,
            tags: vec![],
            attrs: vec![],
            unions: vec![],
        };
        Step { tags, tests, unions, tag_only, resolved: RefCell::new(resolved) }
    }

    pub(crate) unsafe fn matches(&self, n: P) -> bool {
        if (*n).type_ != ELEMENT {
            return false;
        }
        let dict = if (*n).doc.is_null() { ptr::null_mut() } else { (*(*n).doc).dict };
        if dict.is_null() {
            return self.matches_by_string(n);
        }
        let mut r = self.resolved.borrow_mut();
        if r.stale(dict) {
            *r = Resolved::new(dict, self);
        }
        if self.tags.is_some() && !r.tags.contains(&(*n).name) {
            return false;
        }
        if self.tag_only {
            return true;
        }
        self.tests
            .iter()
            .zip(&r.attrs)
            .any(|(t, &attr)| !attr.is_null() && first_attr(n, |name| name == attr).is_some_and(|v| t.test(&v)))
            || self
                .unions
                .iter()
                .zip(&r.unions)
                .any(|(u, names)| u.test(&first_attr(n, |name| names.contains(&name)).unwrap_or(Cow::Borrowed(&[]))))
    }

    unsafe fn matches_by_string(&self, n: P) -> bool {
        if let Some(tags) = &self.tags {
            let name = cstr((*n).name);
            if !tags.iter().any(|t| t == name) {
                return false;
            }
        }
        self.tag_only
            || self
                .tests
                .iter()
                .any(|t| first_attr(n, |name| cstr(name).as_bytes() == &t.name[..]).is_some_and(|v| t.test(&v)))
            || self.unions.iter().any(|u| {
                let v = first_attr(n, |name| u.names.iter().any(|w| cstr(name).as_bytes() == &w[..]));
                u.test(&v.unwrap_or(Cow::Borrowed(&[])))
            })
    }
}

impl AttrTests {
    /// Whether an attribute value satisfies any atom of this group.
    fn test(&self, v: &[u8]) -> bool {
        if self.present {
            return true;
        }
        let mapped: Vec<u8>;
        let v = match &self.map {
            Some(m) => {
                mapped = v.iter().map(|&b| m[b as usize]).collect();
                &mapped[..]
            }
            None => v,
        };
        self.contains.as_ref().is_some_and(|ac| ac.is_match(v))
            || self.starts.iter().any(|s| v.starts_with(s))
            || self.equals.iter().any(|s| v == &s[..])
    }
}

impl UnionTest {
    fn test(&self, v: &[u8]) -> bool {
        match &self.kind {
            UnionKind::Contains(s) => v.windows(s.len().max(1)).any(|w| w == &s[..]) || s.is_empty(),
            UnionKind::StartsWith(s) => v.starts_with(s),
            UnionKind::Regex(re) => re.is_match(std::str::from_utf8(v).expect("libxml2 stores UTF-8")),
        }
    }
}
