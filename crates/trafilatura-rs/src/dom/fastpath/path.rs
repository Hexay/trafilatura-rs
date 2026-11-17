//! Location paths of several steps joined by `//` or `/` (`//div[...]//a[@href]`, `.//head/meta[...]`,
//! `//*[...]|//author`). Each step is a `Step`, so its predicates never depend on position: the result is every
//! element matching the last step whose ancestors match the earlier steps along each axis, in document order.
//! Absolute paths start at the context node's document, as libxml2's `/` does (a detached subtree is not in it).
use super::super::raw::P;
use super::super::ELEMENT;
use super::pred::{parse_step, Step};
use super::pred_parse::split_top;

pub(crate) struct Path {
    absolute: bool,
    /// Each step with whether it is reached from the previous one by the child axis (else descendant).
    steps: Vec<(bool, Step)>,
}

/// One union branch, or None for anything else (attributes, text(), functions, positions...).
fn parse_path(b: &str) -> Option<Path> {
    let (absolute, rest) = match b.strip_prefix(".//") {
        Some(rest) => (false, rest),
        None => (true, b.strip_prefix("//")?),
    };
    let mut steps = Vec::new();
    for desc in split_top(rest, "//") {
        for (j, seg) in split_top(desc, "/").into_iter().enumerate() {
            // parse_step takes a name or `*` head only: `@a`, `text()`, `..` and axes stay on libxml2
            steps.push((j > 0, parse_step(&format!(".//{}", seg.trim()))?));
        }
    }
    Some(Path { absolute, steps })
}

/// A union of paths that all start the same way (all absolute or all relative).
pub(crate) fn parse_union(branches: &[&str]) -> Option<(bool, Vec<Path>)> {
    let paths: Vec<Path> = branches.iter().map(|b| parse_path(b.trim())).collect::<Option<_>>()?;
    let absolute = paths.first()?.absolute;
    paths.iter().all(|p| p.absolute == absolute).then_some((absolute, paths))
}

impl Path {
    /// Whether `n` matches the last step with its ancestors (strictly below `top`) matching the steps before it.
    pub(crate) unsafe fn matches(&self, n: P, top: P) -> bool {
        self.matches_at(n, self.steps.len() - 1, top)
    }

    unsafe fn matches_at(&self, n: P, k: usize, top: P) -> bool {
        if (*n).type_ != ELEMENT || !self.steps[k].1.matches(n) {
            return false;
        }
        if k == 0 {
            return true;
        }
        let mut p = (*n).parent;
        while !p.is_null() && p != top && (*p).type_ == ELEMENT {
            if self.matches_at(p, k - 1, top) {
                return true;
            }
            if self.steps[k].0 {
                return false;
            }
            p = (*p).parent;
        }
        false
    }
}
