//! One location step of the recognised grammar and its native matcher:
//!   step  := .//name[preds]* | .//*[preds]* | .//*[tags][preds]*     (every predicate group must hold)
//!   tags  := self::a or self::b ...  (optionally parenthesised)
//!   preds := atom (or atom)*
//!   atom  := contains(V, "s") | starts-with(V, "s") | @a="v" | @a
//!          | contains(U, "s") | starts-with(U, "s") | re:test(U, 'pattern'[, 'flags'])
//!          | not(self::a or self::b ...) | not(.//name) | not(ancestor::name)
//!   V := @a | translate(@a, "ABC", "abc")      U := @a | @a|@b ...
//! A union U is converted to a string as libxml2 does for function arguments (sorted node-set): the value of the
//! first listed attribute in the element's attribute order, "" if none. `re:test` is Python `re.search` (lxml).
//! Values are compared as UTF-8 bytes, as libxml2's xmlStrstr / xmlStrncmp / xmlStrEqual do.
use super::super::raw::P;
use super::super::{cstr, ELEMENT};
pub(crate) use super::pred_parse::parse_step;
use super::resolve::{look, Resolved};
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
    /// Lookaround patterns, which only fancy-regex runs.
    Fancy(Rc<fancy_regex::Regex>),
}

/// A test on the string value of an attribute union (a single attribute for `re:test(@a, ...)`).
pub(crate) struct UnionTest {
    pub names: Vec<Vec<u8>>,
    pub kind: UnionKind,
}

/// One predicate: a disjunction of atoms.
#[derive(Default)]
pub(crate) struct Preds {
    pub tests: Vec<AttrTests>,
    pub unions: Vec<UnionTest>,
    /// `not(self::a or ...)` atoms (each holds when the element is none of its names).
    pub not_self: Vec<Vec<String>>,
    /// `not(.//name)` atoms.
    pub not_desc: Vec<String>,
    /// `not(ancestor::name)` atoms.
    pub not_anc: Vec<String>,
}

pub(crate) struct Step {
    pub(super) tags: Option<Vec<String>>,
    pub(super) groups: Vec<Preds>,
    /// Names resolved in the current `Ctx` dict (all names there are interned), so matching compares pointers.
    resolved: RefCell<Resolved>,
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

/// Whether some element ancestor of `n` satisfies `is_name`.
unsafe fn has_ancestor(n: P, is_name: impl Fn(*const ffi::xmlChar) -> bool) -> bool {
    let mut p = (*n).parent;
    while !p.is_null() {
        if (*p).type_ == ELEMENT && is_name((*p).name) {
            return true;
        }
        p = (*p).parent;
    }
    false
}

/// Whether some element strictly below `n` satisfies `is_name`.
unsafe fn has_descendant(n: P, is_name: impl Fn(*const ffi::xmlChar) -> bool) -> bool {
    let mut c = super::super::iter::traverse_next(n, n);
    while !c.is_null() {
        if (*c).type_ == ELEMENT && is_name((*c).name) {
            return true;
        }
        c = super::super::iter::traverse_next(n, c);
    }
    false
}

impl Step {
    pub(crate) fn new(tags: Option<Vec<String>>, groups: Vec<Preds>) -> Step {
        Step { tags, groups, resolved: RefCell::new(Resolved::empty()) }
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
        let name = (*n).name;
        if self.tags.is_some() && !r.tags.contains(&name) {
            return false;
        }
        self.groups.iter().zip(&r.groups).all(|(g, rg)| {
            g.tests
                .iter()
                .zip(&rg.attrs)
                .any(|(t, &attr)| !attr.is_null() && first_attr(n, |a| a == attr).is_some_and(|v| t.test(&v)))
                || g.unions
                    .iter()
                    .zip(&rg.unions)
                    .any(|(u, names)| u.test(&first_attr(n, |a| names.contains(&a)).unwrap_or(Cow::Borrowed(&[]))))
                || rg.not_self.iter().any(|tags| !tags.contains(&name))
                || rg.not_desc.iter().any(|&t| t.is_null() || !has_descendant(n, |d| d == t))
                || rg.not_anc.iter().any(|&t| t.is_null() || !has_ancestor(n, |d| d == t))
        })
    }

    unsafe fn matches_by_string(&self, n: P) -> bool {
        let name = cstr((*n).name);
        if self.tags.as_ref().is_some_and(|tags| !tags.iter().any(|t| t == name)) {
            return false;
        }
        fn eq(w: &[u8]) -> impl Fn(*const ffi::xmlChar) -> bool + '_ {
            move |a| unsafe { cstr(a) }.as_bytes() == w
        }
        self.groups.iter().all(|g| {
            g.tests.iter().any(|t| first_attr(n, eq(&t.name)).is_some_and(|v| t.test(&v)))
                || g.unions.iter().any(|u| {
                    let v = first_attr(n, |a| u.names.iter().any(|w| cstr(a).as_bytes() == &w[..]));
                    u.test(&v.unwrap_or(Cow::Borrowed(&[])))
                })
                || g.not_self.iter().any(|tags| !tags.iter().any(|t| t == name))
                || g.not_desc.iter().any(|t| !has_descendant(n, eq(t.as_bytes())))
                || g.not_anc.iter().any(|t| !has_ancestor(n, eq(t.as_bytes())))
        })
    }
}

/// Dict lookups for every name a step compares by pointer.
pub(super) unsafe fn resolve_names(dict: *mut ffi::xmlDict, step: &Step) -> Resolved {
    let mut r = Resolved::empty();
    r.dict = dict;
    r.tags = step.tags.iter().flatten().map(|t| look(dict, t.as_bytes())).collect();
    r.groups = step.groups.iter().map(|g| g.resolve(dict)).collect();
    r
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
        let text = || std::str::from_utf8(v).expect("libxml2 stores UTF-8");
        match &self.kind {
            UnionKind::Contains(s) => v.windows(s.len().max(1)).any(|w| w == &s[..]) || s.is_empty(),
            UnionKind::StartsWith(s) => v.starts_with(s),
            UnionKind::Regex(re) => re.is_match(text()),
            UnionKind::Fancy(re) => re.is_match(text()).expect("regex backtrack limit"),
        }
    }
}
