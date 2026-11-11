//! A step's names as pointers into the current `Ctx` dict, re-resolved when the dict or arena changes.
use super::super::alloc;
use super::pred::{resolve_names, Preds, Step};
use libxml2_sys as ffi;
use std::ptr;

type Name = *const ffi::xmlChar;

pub(super) unsafe fn look(dict: *mut ffi::xmlDict, s: &[u8]) -> Name {
    ffi::xmlDictExists(dict, s.as_ptr(), s.len() as i32)
}

pub(super) struct ResolvedGroup {
    pub attrs: Vec<Name>,
    pub unions: Vec<Vec<Name>>,
    pub not_self: Vec<Vec<Name>>,
    pub not_desc: Vec<Name>,
    pub not_anc: Vec<Name>,
}

pub(crate) struct Resolved {
    pub dict: *mut ffi::xmlDict,
    generation: u64,
    /// Dict size at resolution when some name was absent; renames (`set_tag`) can intern it later.
    missing_at: Option<i32>,
    pub tags: Vec<Name>,
    pub groups: Vec<ResolvedGroup>,
}

impl Preds {
    pub(super) unsafe fn resolve(&self, dict: *mut ffi::xmlDict) -> ResolvedGroup {
        let names = |v: &[String]| v.iter().map(|t| look(dict, t.as_bytes())).collect::<Vec<_>>();
        ResolvedGroup {
            attrs: self.tests.iter().map(|t| look(dict, &t.name)).collect(),
            unions: self.unions.iter().map(|u| u.names.iter().map(|n| look(dict, n)).collect()).collect(),
            not_self: self.not_self.iter().map(|t| names(t)).collect(),
            not_desc: names(&self.not_desc),
            not_anc: names(&self.not_anc),
        }
    }
}

impl Resolved {
    pub(super) fn empty() -> Resolved {
        Resolved { dict: ptr::null_mut(), generation: 0, missing_at: None, tags: vec![], groups: vec![] }
    }

    pub(super) unsafe fn new(dict: *mut ffi::xmlDict, step: &Step) -> Resolved {
        let mut r = resolve_names(dict, step);
        r.generation = alloc::generation();
        let groups = r.groups.iter().flat_map(|g| {
            let names = g.attrs.iter().chain(g.unions.iter().flatten()).chain(g.not_self.iter().flatten());
            names.chain(&g.not_desc).chain(&g.not_anc)
        });
        let missing = r.tags.iter().chain(groups).any(|p| p.is_null());
        r.missing_at = missing.then(|| ffi::xmlDictSize(dict));
        r
    }

    pub(super) unsafe fn stale(&self, dict: *mut ffi::xmlDict) -> bool {
        self.dict != dict
            || self.generation != alloc::generation()
            || self.missing_at.is_some_and(|size| size != ffi::xmlDictSize(dict))
    }
}
