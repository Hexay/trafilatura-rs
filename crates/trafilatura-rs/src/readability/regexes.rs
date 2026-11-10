//! readability_lxml.py's `REGEXES` and `DOT_SPACE` with Python `re.I` semantics.
use regex::Regex;
use std::sync::LazyLock;

/// Expands ASCII letters of `pat` into Python `re.I` (str pattern) classes. Beyond Rust's `(?i)` simple folding,
/// Python also matches U+0130 and U+0131 against `i`.
fn ignorecase(pat: &str) -> Regex {
    let mut out = String::with_capacity(pat.len() * 4);
    for c in pat.chars() {
        if c.is_ascii_alphabetic() {
            let (lo, up) = (c.to_ascii_lowercase(), c.to_ascii_uppercase());
            let extra = match lo {
                'i' => r"\x{130}\x{131}",
                'k' => r"\x{212A}",
                's' => r"\x{17F}",
                _ => "",
            };
            out.push_str(&format!("[{lo}{up}{extra}]"));
        } else {
            out.push(c);
        }
    }
    Regex::new(&out).expect("valid pattern")
}

macro_rules! re_i {
    ($name:ident, $pat:expr) => {
        pub static $name: LazyLock<Regex> = LazyLock::new(|| ignorecase($pat));
    };
}

re_i!(UNLIKELY_CANDIDATES, "combx|comment|community|disqus|extra|foot|header|menu|remark|rss|shoutbox|sidebar|sponsor|ad-break|agegate|pagination|pager|popup|tweet|twitter");
re_i!(OK_MAYBE_CANDIDATE, "and|article|body|column|main|shadow");
re_i!(POSITIVE, "article|body|content|entry|hentry|main|page|pagination|post|text|blog|story");
re_i!(NEGATIVE, "button|combx|comment|com-|contact|figure|foot|footer|footnote|form|input|masthead|media|meta|outbrain|promo|related|scroll|shoutbox|sidebar|sponsor|shopping|tags|tool|widget");
re_i!(DIV_TO_P_ELEMENTS, "<(?:a|blockquote|dl|div|img|ol|p|pre|table|ul)");
re_i!(VIDEO, r"https?://(?:www\.)?(?:youtube|vimeo)\.com");

/// `DOT_SPACE.search(s)` for `\.( |$)`; Python's `$` also matches before a final "\n".
pub fn dot_space(s: &str) -> bool {
    let b = s.as_bytes();
    b.iter().enumerate().any(|(i, &c)| c == b'.' && matches!(&b[i + 1..], [] | [b' ', ..] | [b'\n']))
}
