//! Python 3.12 str/re semantics the port depends on. Rust's notions differ at the edges (`char::is_whitespace`
//! lacks U+001C..U+001F; `regex`'s `\s`/`\w` use other Unicode properties), and those edges change output.

pub mod case;
#[rustfmt::skip]
mod case_tables;
pub mod datetime;
pub mod int;
pub mod json;
pub mod pyre;
pub mod re;
pub mod value;
#[rustfmt::skip]
mod tables;
#[rustfmt::skip]
mod tables_meta;

use std::sync::LazyLock;

fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// re `\w` (str patterns).
pub fn is_word(c: char) -> bool {
    in_table(&tables::WORD, c)
}

pub fn is_alnum(c: char) -> bool {
    in_table(&tables::ALNUM, c)
}

pub fn is_digit(c: char) -> bool {
    in_table(&tables::DIGIT, c)
}

pub fn is_printable(c: char) -> bool {
    in_table(&tables::PRINTABLE, c)
}

/// `str.isalpha()` of one character (Python 3.13, Unicode 15.1).
pub fn is_alpha(c: char) -> bool {
    in_table(&tables_meta::ALPHA, c)
}

/// `unicodedata.category(c)[0] == "P"` (Python 3.13, Unicode 15.1).
pub fn is_punctuation(c: char) -> bool {
    in_table(&tables_meta::PUNCT, c)
}

/// Python re `\w` as a `regex` character-class body: `format!("[{}]", *WORD_CLASS)` for `\w`, `[^...]` for `\W`.
/// Rust's own `\w` also matches combining marks, which Python's does not.
pub static WORD_CLASS: LazyLock<String> =
    LazyLock::new(|| tables::WORD.iter().map(|&(a, b)| format!(r"\x{{{a:x}}}-\x{{{b:x}}}")).collect());

/// `str.isspace()` / re `\s` (Unicode 15.0): bidi class WS, B or S, or category Zs.
pub fn is_space(c: char) -> bool {
    matches!(c,
        '\t'..='\r' | '\u{1c}'..='\u{1f}' | ' ' | '\u{85}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
        | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Python `\s` as a `regex` character-class body (use inside `[...]`).
pub const SPACE_CLASS: &str =
    r"\t\n\x0B\x0C\r\x1C-\x1F \x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}";

/// Streaming `' '.join(s.split())` over a sequence of pieces (whitespace runs spanning pieces collapse too).
#[derive(Default)]
pub struct Collapse {
    out: String,
    pending_space: bool,
}

impl Collapse {
    pub fn push(&mut self, s: &str) {
        if s.is_ascii() {
            for &b in s.as_bytes() {
                if matches!(b, b'\t'..=b'\r' | 0x1c..=0x1f | b' ') {
                    self.pending_space = !self.out.is_empty();
                } else {
                    if self.pending_space {
                        self.out.push(' ');
                        self.pending_space = false;
                    }
                    self.out.push(b as char);
                }
            }
            return;
        }
        for c in s.chars() {
            if is_space(c) {
                self.pending_space = !self.out.is_empty();
            } else {
                if self.pending_space {
                    self.out.push(' ');
                    self.pending_space = false;
                }
                self.out.push(c);
            }
        }
    }

    pub fn finish(self) -> String {
        self.out
    }

    pub fn of(s: &str) -> String {
        let mut c = Collapse::default();
        c.push(s);
        c.finish()
    }
}

/// `str.strip()` with no argument.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.split()` with no argument.
pub fn split(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_space).filter(|p| !p.is_empty())
}

/// `str.splitlines()` boundaries.
pub fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// `str.splitlines()` (no keepends): "\r\n" counts as one break, no trailing empty line.
pub fn splitlines(s: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    std::iter::from_fn(move || {
        while let Some((i, c)) = it.next() {
            if is_line_break(c) {
                let line = &s[start..i];
                start = i + c.len_utf8();
                if c == '\r' {
                    if let Some(&(j, '\n')) = it.peek() {
                        it.next();
                        start = j + 1;
                    }
                }
                return Some(line);
            }
        }
        (start < s.len()).then(|| std::mem::replace(&mut start, s.len())).map(|st| &s[st..])
    })
}

/// `len(s)` (code points).
pub fn len(s: &str) -> usize {
    s.chars().count()
}
