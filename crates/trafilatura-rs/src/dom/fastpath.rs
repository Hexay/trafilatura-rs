//! Native evaluation of the XPath shapes trafilatura runs millions of times, returning exactly libxml2's node-set.
//! Expressions are classified once per string (see `Plan`); anything unrecognised stays on libxml2.
//! `TRAFILATURA_RS_VERIFY_XPATH=1` evaluates both ways on every call and panics on any difference.
use super::raw::P;
use super::{cstr, Node, ELEMENT};
use libxml2_sys as ffi;
use std::sync::LazyLock;

mod pred;
mod pred_parse;
mod resolve;
pub(crate) use pred::{parse_step, Step};

pub(crate) static VERIFY: LazyLock<bool> = LazyLock::new(|| std::env::var_os("TRAFILATURA_RS_VERIFY_XPATH").is_some());

pub(crate) enum Plan {
    /// `.//*[(self::a or ...)][atom or ...]...` (and `.//tag`, unions of `.//tag`): descendants in document order.
    Descendants(Vec<Step>),
    /// `ancestor::tag`: ancestors, document order (outermost first).
    Ancestor(String),
    /// `[.]//tag//text()` / `//tag//text()`: text nodes below any `tag`, document order.
    TextUnder { absolute: bool, tag: String },
    /// Union of `.//outer//inner` (outer: a name or `*[self::a or ...]`): `inner` elements with an ancestor named
    /// in `outer` strictly inside the context node.
    NestedUnder(Vec<(Vec<String>, String)>),
    /// `[.]//processing-instruction()|[.]//*[not(node())]`: PIs and childless elements of the whole document
    /// (absolute) or of the context node's descendants.
    EmptyOrPi { absolute: bool },
}

/// `a` or `*[self::a or self::b ...]` -> tag names.
fn outer_tags(s: &str) -> Option<Vec<String>> {
    if is_name(s) {
        return Some(vec![s.to_owned()]);
    }
    let inner = s.strip_prefix("*[")?.strip_suffix(']')?;
    inner.split(" or ").map(|t| t.trim().strip_prefix("self::").filter(|n| is_name(n)).map(str::to_owned)).collect()
}

/// Recognise an expression; `None` means "let libxml2 do it".
pub(crate) fn plan(expr: &str) -> Option<Plan> {
    let e: String = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(tag) = e.strip_prefix("ancestor::") {
        return is_name(tag).then(|| Plan::Ancestor(tag.to_owned()));
    }
    for (prefix, absolute) in [(".//", false), ("//", true)] {
        if let Some(tag) = e.strip_prefix(prefix).and_then(|r| r.strip_suffix("//text()")) {
            if is_name(tag) {
                return Some(Plan::TextUnder { absolute, tag: tag.to_owned() });
            }
        }
    }
    match e.as_str() {
        "//processing-instruction()|//*[not(node())]" => return Some(Plan::EmptyOrPi { absolute: true }),
        ".//processing-instruction()|.//*[not(node())]" => return Some(Plan::EmptyOrPi { absolute: false }),
        _ => {}
    }
    let nested: Option<Vec<(Vec<String>, String)>> = split_union(&e)
        .iter()
        .map(|b| {
            let (outer, inner) = b.trim().strip_prefix(".//")?.split_once("//")?;
            is_name(inner).then_some(())?;
            Some((outer_tags(outer)?, inner.to_owned()))
        })
        .collect();
    if let Some(pairs) = nested {
        return Some(Plan::NestedUnder(pairs));
    }
    let branches: Option<Vec<Step>> = split_union(&e).iter().map(|b| parse_step(b.trim())).collect();
    branches.map(Plan::Descendants)
}

/// Whether `n` has an element ancestor named in `tags` strictly below `top`.
unsafe fn has_ancestor_below(top: P, n: P, tags: &[String]) -> bool {
    let mut p = (*n).parent;
    while !p.is_null() && p != top {
        if (*p).type_ == ELEMENT && tags.iter().any(|t| t == cstr((*p).name)) {
            return true;
        }
        p = (*p).parent;
    }
    false
}

/// Plan for `xpath_first` only: branches may be `(step)[1]` or `step[1]`. The document-order first element of
/// `.//*[p][1]` (first match per parent) is the first match overall, and a union's first is the earliest branch
/// first, so the first element matching any branch is exactly libxml2's first result.
pub(crate) fn plan_first(expr: &str) -> Option<Plan> {
    let e: String = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    let steps: Option<Vec<Step>> = split_union(&e)
        .iter()
        .map(|b| {
            let b = b.trim();
            let b =
                b.strip_suffix("[1]").map_or(b, |s| s.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(s));
            parse_step(b.trim())
        })
        .collect();
    steps.map(Plan::Descendants)
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn split_union(e: &str) -> Vec<&str> {
    let (mut depth, mut quote, mut start, mut parts) = (0i32, None, 0, Vec::new());
    for (i, c) in e.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth -= 1,
            (None, '|') if depth == 0 => {
                parts.push(&e[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&e[start..]);
    parts
}

unsafe fn matches_any(steps: &[Step], n: P) -> bool {
    (*n).type_ == ELEMENT && steps.iter().any(|s| s.matches(n))
}

/// Next node in document order below `top` (elements only), like libxml2's descendant axis.
unsafe fn next_below(top: P, n: P) -> P {
    super::iter::traverse_next(top, n)
}

impl Plan {
    pub(crate) fn nodes(&self, ctx_node: Node) -> Vec<Node> {
        let mut out = Vec::new();
        unsafe {
            match self {
                Plan::Descendants(steps) => {
                    let top = ctx_node.raw();
                    let mut n = next_below(top, top);
                    while !n.is_null() {
                        if matches_any(steps, n) {
                            out.push(Node::from_raw(n).expect("non-null"));
                        }
                        n = next_below(top, n);
                    }
                }
                Plan::Ancestor(tag) => {
                    let mut p = (*ctx_node.raw()).parent;
                    while !p.is_null() {
                        if (*p).type_ == ELEMENT && cstr((*p).name) == tag {
                            out.push(Node::from_raw(p).expect("non-null"));
                        }
                        p = (*p).parent;
                    }
                    out.reverse();
                }
                Plan::NestedUnder(pairs) => {
                    let top = ctx_node.raw();
                    let mut n = next_below(top, top);
                    while !n.is_null() {
                        if (*n).type_ == ELEMENT {
                            let name = cstr((*n).name);
                            if pairs.iter().any(|(o, i)| i == name && has_ancestor_below(top, n, o)) {
                                out.push(Node::from_raw(n).expect("non-null"));
                            }
                        }
                        n = next_below(top, n);
                    }
                }
                Plan::EmptyOrPi { absolute } => {
                    let top = if *absolute { ctx_node.doc() as P } else { ctx_node.raw() };
                    let mut n = next_below(top, top);
                    while !n.is_null() {
                        let t = (*n).type_;
                        if t == super::PI || (t == ELEMENT && (*n).children.is_null()) {
                            out.push(Node::from_raw(n).expect("non-null"));
                        }
                        n = next_below(top, n);
                    }
                }
                Plan::TextUnder { .. } => {}
            }
        }
        out
    }

    /// First node of `nodes()` without materialising the rest.
    pub(crate) fn first(&self, ctx_node: Node) -> Option<Node> {
        match self {
            Plan::Descendants(steps) => unsafe {
                let top = ctx_node.raw();
                let mut n = next_below(top, top);
                while !n.is_null() {
                    if matches_any(steps, n) {
                        return Node::from_raw(n);
                    }
                    n = next_below(top, n);
                }
                None
            },
            _ => self.nodes(ctx_node).into_iter().next(),
        }
    }

    pub(crate) fn strings(&self, ctx_node: Node) -> Vec<String> {
        let Plan::TextUnder { absolute, tag } = self else { return Vec::new() };
        let mut out = Vec::new();
        unsafe {
            let start = if *absolute { ctx_node.doc() as P } else { ctx_node.raw() };
            collect_text_below(start, false, tag, &mut out);
        }
        out
    }
}

/// Text/CDATA children of `n` (recursively through elements), kept once any ancestor below the start is `tag`.
unsafe fn collect_text_below(n: P, below: bool, tag: &str, out: &mut Vec<String>) {
    let mut c = (*n).children;
    while !c.is_null() {
        match (*c).type_ {
            ffi::xmlElementType_XML_TEXT_NODE | ffi::xmlElementType_XML_CDATA_SECTION_NODE if below => {
                out.push(cstr((*c).content).to_owned())
            }
            ELEMENT => collect_text_below(c, below || cstr((*c).name) == tag, tag, out),
            _ => {}
        }
        c = (*c).next;
    }
}
