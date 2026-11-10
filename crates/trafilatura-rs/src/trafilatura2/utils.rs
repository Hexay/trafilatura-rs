//! trafilatura 2.2.0 utils.py: input repair and loading, image checks, textfilter and the list/cell position
//! helpers of the text serializer. Unchanged helpers (trim, sanitize, text_chars_test, ...) come from 1.11.
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

/// `repair_faulty_html`. Borrows the input when nothing changes (the common case): the control-character filter
/// only copies when a byte scan finds one, and the doctype rebuild only differs from the input when the first line
/// changed or there was no newline (Python appends one).
pub fn repair_faulty_html<'a>(html: &'a str, beginning: &str) -> Cow<'a, str> {
    // U+FFFE/U+FFFF are EF BF BE / EF BF BF in UTF-8.
    let bytes = html.as_bytes();
    let has_invalid = bytes.iter().any(|&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r'))
        || bytes.windows(3).any(|w| w[0] == 0xef && w[1] == 0xbf && w[2] >= 0xbe);
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
    for (i, line) in py::splitlines(&s).enumerate() {
        if line.contains("<html") && line.ends_with("/>") {
            return Cow::Owned(FAULTY_HTML.replacen(&s, 1, "$1>").into_owned());
        }
        if i > 2 {
            break;
        }
    }
    s
}

/// `load_html` for str input.
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

/// `is_image_element`.
pub fn is_image_element(el: Node) -> bool {
    ["data-src", "src"].iter().any(|a| is_image_file(Some(&el.get(a).unwrap_or_default())))
        || el.attrs().iter().any(|(k, v)| k.starts_with("data-src") && is_image_file(Some(v)))
}

/// `textfilter`: True when the element's text (or tail) should be discarded.
pub fn textfilter(el: Node) -> bool {
    let test = match el.text() {
        None => el.tail(),
        t => t,
    };
    let Some(t) = test.filter(|t| text_chars_test(Some(t))) else { return true };
    let re = py::re::compile(&format!("^(?:{RE_FILTER})"), true);
    let filtered = py::splitlines(&t).any(|l| re.is_match(l));
    filtered
}

/// `is_in_table_cell`.
pub fn is_in_table_cell(el: Node) -> bool {
    el.parent().is_some() && std::iter::successors(Some(el), |e| e.parent()).any(|e| e.tag() == "cell")
}

/// `is_last_element_in_cell`.
pub fn is_last_element_in_cell(el: Node) -> bool {
    if !is_in_table_cell(el) {
        return false;
    }
    let container = if el.tag() == "cell" { el } else { el.parent().expect("in a cell") };
    container.is_empty() || container.child(-1) == Some(el)
}

/// `is_element_in_item`.
pub fn is_element_in_item(el: Node) -> bool {
    std::iter::successors(Some(el), |e| e.parent()).any(|e| e.tag() == "item")
}

/// `item_if_first_element`: the enclosing list item when `el` carries its first content.
pub fn item_if_first_element(el: Node) -> Option<Node> {
    if el.tag() == "item" {
        return el.text().is_some_and(|t| !t.is_empty()).then_some(el);
    }
    let item = el.iter_ancestors(&["item"]).next()?;
    (item.text().is_none_or(|t| t.is_empty()) && item.iter_descendants(&["*"]).next() == Some(el)).then_some(item)
}

/// `is_last_element_in_item`.
pub fn is_last_element_in_item(el: Node) -> bool {
    if !is_element_in_item(el) {
        return false;
    }
    if el.tag() == "item" {
        return el.is_empty();
    }
    el.next().is_none_or(|n| n.tag() == "item")
}
