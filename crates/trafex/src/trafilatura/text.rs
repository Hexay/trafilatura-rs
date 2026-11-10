//! Text helpers from trafilatura/utils.py (trim, sanitize, line_processing, textfilter, ...).
use crate::dom::Node;
use crate::py;
use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

/// `trim`: `' '.join(s.split()).strip()`.
pub fn trim(s: &str) -> String {
    if is_trimmed(s) {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    if s.is_ascii() {
        let mut buf = Vec::with_capacity(s.len());
        for &b in s.as_bytes() {
            if matches!(b, b'\t'..=b'\r' | 0x1c..=0x1f | b' ') {
                pending_space = !buf.is_empty();
            } else {
                if pending_space {
                    buf.push(b' ');
                    pending_space = false;
                }
                buf.push(b);
            }
        }
        return String::from_utf8(buf).expect("ascii");
    }
    for c in s.chars() {
        if py::is_space(c) {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

/// `len(trim(s))` without building the string: non-space characters plus one separator between tokens.
pub fn trim_len(s: &str) -> usize {
    let (mut n, mut tokens, mut in_token) = (0usize, 0usize, false);
    for c in s.chars() {
        if py::is_space(c) {
            in_token = false;
        } else {
            if !in_token {
                tokens += 1;
                in_token = true;
            }
            n += 1;
        }
    }
    n + tokens.saturating_sub(1)
}

/// Already in `trim` form: whitespace only as single ' ' between non-space characters.
fn is_trimmed(s: &str) -> bool {
    let mut prev_space = true;
    for c in s.chars() {
        if c == ' ' {
            if prev_space {
                return false;
            }
            prev_space = true;
        } else if py::is_space(c) {
            return false;
        } else {
            prev_space = false;
        }
    }
    !prev_space || s.is_empty()
}

/// `text_chars_test`: non-empty and not all whitespace.
pub fn text_chars_test(s: Option<&str>) -> bool {
    s.is_some_and(|s| !s.is_empty() && !s.chars().all(py::is_space))
}

/// `remove_control_characters`: keep printable or whitespace characters.
pub fn remove_control_characters(s: &str) -> String {
    s.chars().filter(|&c| py::is_printable(c) || py::is_space(c)).collect()
}

pub fn normalize_unicode(s: &str) -> String {
    s.nfc().collect()
}

/// `LINES_TRIMMING.sub(" ", s)` for `(?<![p{P}>])\n`: Python reads `[p{P}>]` as the literal set `p { P } >`.
fn lines_trimming(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev: Option<char> = None;
    for c in s.chars() {
        let keep = c != '\n' || matches!(prev, Some('p' | '{' | 'P' | '}' | '>'));
        out.push(if keep { c } else { ' ' });
        prev = Some(c);
    }
    out
}

/// `line_processing`; `None` for lines that are empty or all whitespace.
pub fn line_processing(line: &str, preserve_space: bool, trailing_space: bool) -> Option<String> {
    let replaced = line.replace("&#13;", "\r").replace("&#10;", "\n").replace("&nbsp;", "\u{a0}");
    let mut new_line = remove_control_characters(&replaced);
    if !preserve_space {
        new_line = trim(&lines_trimming(&new_line));
        if new_line.chars().all(py::is_space) {
            return None;
        }
        if trailing_space {
            let before = if line.chars().next().is_some_and(py::is_space) { " " } else { "" };
            let after = if line.chars().next_back().is_some_and(py::is_space) { " " } else { "" };
            new_line = format!("{before}{new_line}{after}");
        }
    }
    Some(new_line)
}

/// `sanitize`.
pub fn sanitize(text: &str, preserve_space: bool, trailing_space: bool) -> Option<String> {
    if trailing_space {
        return line_processing(text, preserve_space, true);
    }
    let lines: Vec<String> = py::splitlines(text)
        .into_iter()
        .filter_map(|l| line_processing(l, preserve_space, false))
        .filter(|l| !l.is_empty())
        .collect();
    Some(lines.join("\n").replace('\u{2424}', ""))
}

const IMAGE_EXTENSIONS: &[&str] = &["avif", "bmp", "gif", "heic", "heif", "jpeg", "jpg", "png", "webp"];

/// `is_image_file`: `[^\s]+\.(avif|bmp|gif|hei[cf]|jpe?g|png|webp)(\b|$)` found anywhere.
pub fn is_image_file(src: Option<&str>) -> bool {
    let Some(s) = src else { return false };
    let chars: Vec<char> = s.chars().collect();
    (1..chars.len()).any(|k| {
        chars[k] == '.'
            && !py::is_space(chars[k - 1])
            && IMAGE_EXTENSIONS.iter().any(|ext| {
                let end = k + 1 + ext.len();
                end <= chars.len()
                    && chars[k + 1..end].iter().copied().eq(ext.chars())
                    && chars.get(end).is_none_or(|&c| !py::is_word(c))
            })
    })
}

/// utils.RE_FILTER, used with `.match` (anchored at the start). Built through `py::re` for Python's `re.I`, under
/// which `i` also matches U+0130/U+0131 (Rust's `(?i)` does not).
static RE_FILTER: LazyLock<Regex> = LazyLock::new(|| {
    let pat = r"\W*(Drucken|E-?Mail|Facebook|Flipboard|Google|Instagram|Linkedin|Mail|PDF|Pinterest|Pocket|Print|QQ|Reddit|Twitter|WeChat|WeiBo|Whatsapp|Xing|Mehr zum Thema:?|More on this.{,8}$)$";
    Regex::new(&format!("^(?:{})", py::re::translate(pat, true))).unwrap()
});

fn re_filter_match(line: &str) -> bool {
    RE_FILTER.is_match(line)
}

/// `textfilter`: True when the element's text (or tail) should be discarded.
pub fn textfilter(el: Node) -> bool {
    let test = match el.text() {
        None => el.tail(),
        t => t,
    };
    if !text_chars_test(test.as_deref()) {
        return true;
    }
    let filtered = py::splitlines(test.as_deref().unwrap_or("")).any(re_filter_match);
    filtered
}

#[cfg(test)]
mod tests {
    use super::re_filter_match;

    /// Expected values from `trafilatura.utils.RE_FILTER.match` on Python 3.12.
    #[test]
    fn re_filter_follows_python_ignorecase() {
        for (line, want) in [
            ("Lınkedin", true),
            ("Lİnkedin", true),
            ("PRİNT", true),
            ("More on this story", true),
            ("  Twitter", true),
            ("Twitter!", false),
            ("Facebook\n", true),
        ] {
            assert_eq!(re_filter_match(line), want, "{line:?}");
        }
    }
}
