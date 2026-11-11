//! Python `re` pattern objects over fancy-regex. Patterns come pre-translated by harness/gen_py_regexes.py, so
//! classes, `re.I`, `\b` and `$` already behave as in Python; this layer adds Python's search/match/findall/sub/split
//! API. Offsets are byte offsets into the haystack.
use fancy_regex::{Regex, RegexBuilder, RegexInput};
use std::sync::OnceLock;

pub type Caps<'t> = fancy_regex::Captures<'t, str>;

pub struct Pattern {
    src: &'static str,
    re: OnceLock<Regex>,
    full: OnceLock<Regex>,
}

fn build(src: &str) -> Regex {
    RegexBuilder::new(src)
        // Python never gives up on backtracking; neither may we.
        .backtrack_limit(usize::MAX)
        .delegate_size_limit(1 << 28)
        .delegate_dfa_size_limit(1 << 28)
        .build()
        .unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

impl Pattern {
    pub const fn new(src: &'static str) -> Pattern {
        Pattern { src, re: OnceLock::new(), full: OnceLock::new() }
    }

    fn re(&self) -> &Regex {
        self.re.get_or_init(|| build(self.src))
    }

    fn run<'t>(re: &Regex, input: RegexInput<'t, str>) -> Option<Caps<'t>> {
        re.captures_input(input).expect("regex runtime error")
    }

    /// `pattern.search(s, pos)`; lookbehinds see text before `pos`, `^` only matches at 0.
    pub fn search_from<'t>(&self, s: &'t str, pos: usize) -> Option<Caps<'t>> {
        Self::run(self.re(), RegexInput::new(s).from_pos(pos))
    }

    pub fn search<'t>(&self, s: &'t str) -> Option<Caps<'t>> {
        self.search_from(s, 0)
    }

    pub fn is_match(&self, s: &str) -> bool {
        self.re().is_match(s).expect("regex runtime error")
    }

    /// `pattern.match(s, pos)`.
    pub fn match_at<'t>(&self, s: &'t str, pos: usize) -> Option<Caps<'t>> {
        Self::run(self.re(), RegexInput::new(s).from_pos(pos).anchored(true))
    }

    pub fn match_<'t>(&self, s: &'t str) -> Option<Caps<'t>> {
        self.match_at(s, 0)
    }

    /// `pattern.fullmatch(s)`.
    pub fn fullmatch<'t>(&self, s: &'t str) -> Option<Caps<'t>> {
        let re = self.full.get_or_init(|| build(&format!(r"(?:{})\z", self.src)));
        Self::run(re, RegexInput::new(s).anchored(true))
    }

    /// `pattern.finditer(s)`. An empty match is never followed by another empty match at the same position.
    pub fn finditer<'t>(&self, s: &'t str) -> Vec<Caps<'t>> {
        let mut out = Vec::new();
        let mut pos = 0;
        let mut after_empty = false;
        while pos <= s.len() {
            let Some(mut caps) = self.search_from(s, pos) else { break };
            let m = caps.get(0).expect("group 0");
            if after_empty && m.start() == pos && m.end() == pos {
                let Some(c) = s[pos..].chars().next() else { break };
                match self.search_from(s, pos + c.len_utf8()) {
                    Some(c) => caps = c,
                    None => break,
                }
            }
            let m = caps.get(0).expect("group 0");
            after_empty = m.start() == m.end();
            pos = m.end();
            out.push(caps);
        }
        out
    }

    /// `pattern.findall(s)` for patterns with at most one group (a non-participating group gives "").
    pub fn findall<'t>(&self, s: &'t str) -> Vec<&'t str> {
        let group = usize::from(self.re().captures_len() > 1);
        assert!(self.re().captures_len() <= 2, "findall with several groups returns tuples");
        self.finditer(s).iter().map(|c| c.get(group).map_or("", |m| m.as_str())).collect()
    }

    /// `pattern.sub(repl, s, count)` with a function replacement; `count` 0 replaces all.
    pub fn sub_with(&self, s: &str, count: usize, mut repl: impl FnMut(&Caps) -> String) -> String {
        let mut out = String::with_capacity(s.len());
        let mut last = 0;
        for (n, caps) in self.finditer(s).iter().enumerate() {
            if count != 0 && n == count {
                break;
            }
            let m = caps.get(0).expect("group 0");
            out.push_str(&s[last..m.start()]);
            out.push_str(&repl(caps));
            last = m.end();
        }
        out.push_str(&s[last..]);
        out
    }

    /// `pattern.sub(literal, s)`; `literal` must not contain backslashes.
    pub fn sub(&self, s: &str, literal: &str) -> String {
        debug_assert!(!literal.contains('\\'));
        self.sub_with(s, 0, |_| literal.to_owned())
    }

    /// `pattern.split(s)` for patterns without groups.
    pub fn split<'t>(&self, s: &'t str) -> Vec<&'t str> {
        assert_eq!(self.re().captures_len(), 1, "split with groups also returns the groups");
        let mut out = Vec::new();
        let mut last = 0;
        for caps in self.finditer(s) {
            let m = caps.get(0).expect("group 0");
            out.push(&s[last..m.start()]);
            last = m.end();
        }
        out.push(&s[last..]);
        out
    }
}

/// Group `i` of a match as Python's `m[i]` (None when the group did not participate).
pub fn group<'t>(caps: &Caps<'t>, i: usize) -> Option<&'t str> {
    caps.get(i).map(|m| m.as_str())
}

/// Named group as Python's `m["name"]`.
pub fn named<'t>(caps: &Caps<'t>, name: &str) -> Option<&'t str> {
    caps.name(name).map(|m| m.as_str())
}

/// Code point offset of byte offset `byte` in `s` (Python string indices).
pub fn char_index(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

/// Byte offset of code point index `index` in `s`, clamped to the end.
pub fn byte_index(s: &str, index: usize) -> usize {
    s.char_indices().nth(index).map_or(s.len(), |(b, _)| b)
}
