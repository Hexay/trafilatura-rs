//! Python `re` patterns (str, no MULTILINE/DOTALL) translated to `regex`, for search/match truthiness only.
//! Covers the syntax trafilatura's patterns use: literals, escapes, classes, groups, quantifiers, `^`/`$`.
use super::{SPACE_CLASS, WORD_CLASS};
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Python `re.I` letter classes: beyond ASCII case, `i` also matches U+0130/U+0131, `k` U+212A, `s` U+017F.
fn letter_class(c: char) -> String {
    let (lo, up) = (c.to_ascii_lowercase(), c.to_ascii_uppercase());
    let extra = match lo {
        'i' => r"\x{130}\x{131}",
        'k' => r"\x{212A}",
        's' => r"\x{17F}",
        _ => "",
    };
    format!("{lo}{up}{extra}")
}

fn escape(c: char, in_class: bool) -> String {
    match c {
        's' if in_class => SPACE_CLASS.to_owned(),
        's' => format!("[{SPACE_CLASS}]"),
        'S' => format!("[^{SPACE_CLASS}]"),
        'w' if in_class => WORD_CLASS.clone(),
        'w' => format!("[{}]", *WORD_CLASS),
        'W' => format!("[^{}]", *WORD_CLASS),
        'd' => r"\d".to_owned(),
        c => format!("\\{c}"),
    }
}

/// Translates `pat`; `$` becomes "end, or before a final newline" as in Python.
pub fn translate(pat: &str, ignorecase: bool) -> String {
    assert!(pat.is_ascii(), "non-ASCII pattern {pat:?}");
    let chars: Vec<char> = pat.chars().collect();
    let mut out = String::with_capacity(pat.len() * 2);
    let mut in_class = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                i += 1;
                let e = *chars.get(i).expect("dangling escape");
                assert!(!matches!(e, 'b' | 'B' | 'A' | 'Z') || in_class, "unsupported escape \\{e}");
                out.push_str(&escape(e, in_class));
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                if chars.get(i + 1) == Some(&'^') {
                    out.push('^');
                    i += 1;
                }
                if chars.get(i + 1) == Some(&']') {
                    out.push_str(r"\]");
                    i += 1;
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '$' if !in_class => out.push_str(r"(?:\n?\z)"),
            '{' if !in_class && chars.get(i + 1) == Some(&',') => out.push_str("{0"),
            c if c.is_ascii_alphabetic() && ignorecase => {
                assert!(!in_class || chars.get(i + 1) != Some(&'-'), "letter range under re.I in {pat:?}");
                let class = letter_class(c);
                if in_class {
                    out.push_str(&class);
                } else {
                    out.push('[');
                    out.push_str(&class);
                    out.push(']');
                }
            }
            // regex class set operators; a lone '-' stays a range or literal as in Python
            '&' | '~' if in_class => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

thread_local! {
    static CACHE: RefCell<HashMap<(String, bool), Rc<Regex>>> = RefCell::new(HashMap::new());
}

/// `re.compile(pat, re.I if ignorecase else 0)`, cached per thread.
pub fn compile(pat: &str, ignorecase: bool) -> Rc<Regex> {
    CACHE.with(|cache| {
        let key = (pat.to_owned(), ignorecase);
        if let Some(r) = cache.borrow().get(&key) {
            return r.clone();
        }
        let r = Rc::new(Regex::new(&translate(pat, ignorecase)).unwrap_or_else(|e| panic!("{pat:?}: {e}")));
        cache.borrow_mut().insert(key, r.clone());
        r
    })
}

thread_local! {
    static FANCY: RefCell<HashMap<(String, bool), Rc<fancy_regex::Regex>>> = RefCell::new(HashMap::new());
}

pub fn has_lookaround(pat: &str) -> bool {
    ["(?=", "(?!", "(?<=", "(?<!"].iter().any(|l| pat.contains(l))
}

/// `re.search(pat, s) is not None`; lookaround patterns (trafilatura 2.3's XPaths) go through fancy-regex.
pub fn search(pat: &str, ignorecase: bool, s: &str) -> bool {
    if !has_lookaround(pat) {
        return compile(pat, ignorecase).is_match(s);
    }
    compile_fancy(pat, ignorecase).is_match(s).expect("regex backtrack limit")
}

/// `compile` for patterns the `regex` crate cannot run (lookaround), cached per thread.
pub fn compile_fancy(pat: &str, ignorecase: bool) -> Rc<fancy_regex::Regex> {
    FANCY.with(|cache| {
        let key = (pat.to_owned(), ignorecase);
        if let Some(r) = cache.borrow().get(&key) {
            return r.clone();
        }
        let r =
            Rc::new(fancy_regex::Regex::new(&translate(pat, ignorecase)).unwrap_or_else(|e| panic!("{pat:?}: {e}")));
        cache.borrow_mut().insert(key, r.clone());
        r
    })
}

#[cfg(test)]
mod tests {
    use super::search;

    #[test]
    fn lookahead() {
        let pat = "^(?:[Cc]omment(?!ary)|comol)";
        assert!(search(pat, false, "comments"));
        assert!(!search(pat, false, "commentary"));
        assert!(search(pat, true, "COMOL\u{130}"));
        assert!(search("x(?!y)$", false, "ax\n"));
    }
}
