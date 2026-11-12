//! trafilatura/utils.py.
use crate::dom::{fromstring, Ctx, Error, Input, Node, HTML_PARSER};
use crate::py;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

pub(crate) static DOCTYPE_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^< ?! ?DOCTYPE.+?/ ?>").unwrap());
pub(crate) static FAULTY_HTML: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)(<html.*?)[{}]*/>", py::SPACE_CLASS)).unwrap());

/// `repair_faulty_html`. Python rebuilds the string after the doctype fix; that rebuild only differs from the
/// input when the first line changed or there was no newline (it appends one), so otherwise borrow.
pub fn repair_faulty_html<'a>(html: &'a str, beginning: &str) -> Cow<'a, str> {
    let mut s = Cow::Borrowed(html);
    if beginning.contains("doctype") {
        let (first, rest) = html.split_once('\n').unwrap_or((html, ""));
        let fixed = DOCTYPE_TAG.replacen(first, 1, "");
        if matches!(fixed, Cow::Owned(_)) || !html.contains('\n') {
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

/// `fromstring_bytes`: UTF-8 bytes through the parser's default encoding; any failure is None.
pub fn fromstring_bytes(ctx: &Ctx, html: &str) -> Option<Node> {
    fromstring(ctx, &Input::Bytes(html.as_bytes()), HTML_PARSER).ok()
}

/// `load_html` for str input (the only form datatrove passes).
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
