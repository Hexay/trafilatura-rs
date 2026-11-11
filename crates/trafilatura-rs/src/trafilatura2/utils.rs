//! trafilatura 2.3.1 utils.py: input repair and loading, image checks and textfilter. Unchanged helpers (trim,
//! sanitize, text_chars_test, ...) come from 1.11.
use crate::dom::{fromstring, Ctx, Error, Input, Node, HTML_PARSER};
use crate::py;
use crate::trafilatura::text::text_chars_test;
use crate::trafilatura::utils::fromstring_bytes;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

static DOCTYPE_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^< ?! ?DOCTYPE[^>]*/[^<>]*>").unwrap());
static FAULTY_HTML: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)(<html.*?)[{}]*/>", py::SPACE_CLASS)).unwrap());
const RE_FILTER: &str = r"\W*(Drucken|E-?Mail|Facebook|Flipboard|Google|Instagram|Linkedin|Mail|PDF|Pinterest|Pocket|Print|QQ|Reddit|Twitter|WeChat|WeiBo|Whatsapp|Xing|Mehr zum Thema:?|More on this.{,8}$)$";

fn invalid_xml_char(c: char) -> bool {
    matches!(c, '\0'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f' | '\u{fffe}' | '\u{ffff}')
}

/// `htmlstring[:n]` in code points.
fn prefix_chars(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// `repair_faulty_html`. Borrows the input when nothing changes (the common case): the control-character filter
/// only copies when a byte scan finds one, and the doctype rebuild only differs from the input when the first line
/// changed or there was no newline (Python appends one).
pub fn repair_faulty_html<'a>(html: &'a str, beginning: &str) -> Cow<'a, str> {
    // U+FFFE/U+FFFF are EF BF BE / EF BF BF in UTF-8.
    let bytes = html.as_bytes();
    // branch-free per chunk so the scan vectorizes
    let control = |c: &[u8]| c.iter().fold(false, |acc, &b| acc | (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')));
    let has_invalid = bytes.chunks(64).any(control)
        || memchr::memmem::find_iter(bytes, b"\xef\xbf").any(|i| bytes.get(i + 2).is_some_and(|&c| c >= 0xbe));
    let mut s: Cow<str> = if has_invalid {
        Cow::Owned(html.chars().filter(|&c| !invalid_xml_char(c)).collect())
    } else {
        Cow::Borrowed(html)
    };
    if beginning.contains("doctype") {
        let (first, rest) = s.split_once('\n').unwrap_or((&s, ""));
        let fixed = DOCTYPE_TAG.replacen(first, 1, "");
        if matches!(fixed, Cow::Owned(_)) || !s.contains('\n') {
            s = Cow::Owned(format!("{fixed}\n{rest}"));
        }
    }
    let faulty = py::splitlines(prefix_chars(&s, 4096)).take(4).any(|l| l.contains("<html") && l.ends_with("/>"));
    if faulty {
        return Cow::Owned(FAULTY_HTML.replacen(&s, 1, "$1>").into_owned());
    }
    s
}

/// `load_html` for str input (decode_file and its MAX_FILE_SIZE cap only apply to bytes).
pub fn load_html(ctx: &Ctx, html: &str) -> Option<Node> {
    let beginning = html.chars().take(50).collect::<String>().to_lowercase();
    let check_flag = !beginning.contains("html");
    let html = repair_faulty_html(html, &beginning);
    let mut fallback_parse = false;
    let mut tree = match fromstring(ctx, &Input::Str(&html), HTML_PARSER) {
        Ok(t) => Some(t),
        Err(Error::Value(_)) => {
            fallback_parse = true;
            fromstring_bytes(ctx, &html)
        }
        Err(_) => None,
    };
    if tree.is_none_or(|t| t.is_empty()) && !fallback_parse {
        tree = fromstring_bytes(ctx, &html);
    }
    tree.filter(|t| !(check_flag && t.len() < 2))
}

/// `trim(s) or None`.
pub fn trim_or_none(s: Option<String>) -> Option<String> {
    s.map(|s| crate::trafilatura::text::trim(&s)).filter(|t| !t.is_empty())
}

/// Python `re.I` equality of one input char with a lowercase ASCII pattern letter.
fn eq_ignorecase(c: char, letter: char) -> bool {
    c.to_ascii_lowercase() == letter || (letter == 'i' && matches!(c, '\u{130}' | '\u{131}'))
}

const IMAGE_EXTENSIONS: &[&str] = &["avif", "bmp", "gif", "heic", "heif", "jpeg", "jpg", "png", "webp"];

/// `is_image_file`: at most 8192 code points, `[^\s]+\.(avif|bmp|gif|hei[cf]|jpe?g|png|webp)(\b|$)` under re.I.
pub fn is_image_file(src: Option<&str>) -> bool {
    let Some(s) = src else { return false };
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > 8192 {
        return false;
    }
    (1..chars.len()).any(|k| {
        chars[k] == '.'
            && !py::is_space(chars[k - 1])
            && IMAGE_EXTENSIONS.iter().any(|ext| {
                let end = k + 1 + ext.len();
                end <= chars.len()
                    && chars[k + 1..end].iter().zip(ext.chars()).all(|(&c, l)| eq_ignorecase(c, l))
                    && chars.get(end).is_none_or(|&c| !py::is_word(c))
            })
    })
}

/// `image_src`: src, data-src, or the first data-src* attribute pointing to an image file.
pub fn image_src(el: Node) -> Option<String> {
    ["data-src", "src"].iter().map(|a| el.get(a).unwrap_or_default()).find(|s| is_image_file(Some(s))).or_else(|| {
        el.attrs().into_iter().find(|(k, v)| k.starts_with("data-src") && is_image_file(Some(v))).map(|kv| kv.1)
    })
}

/// `textfilter`: True when the element's text (or tail) should be discarded.
pub fn textfilter(el: Node) -> bool {
    let test = match el.text() {
        None => el.tail(),
        t => t,
    };
    let Some(t) = test.filter(|t| text_chars_test(Some(t))) else { return true };
    thread_local! {
        static FILTER: std::rc::Rc<Regex> = py::re::compile(&format!("^(?:{RE_FILTER})"), true);
    }
    FILTER.with(|re| py::splitlines(&t).any(|l| re.is_match(l)))
}
