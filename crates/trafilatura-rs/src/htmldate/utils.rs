//! htmldate/utils.py `load_html` for str input. Unlike trafilatura's, it keeps comments and has its own repair and
//! fallback steps.
use crate::dom::{fromstring, Ctx, Input, Node, HTMLDATE_PARSER};
use crate::trafilatura::utils::{DOCTYPE_TAG, FAULTY_HTML};
use std::borrow::Cow;

/// `repair_faulty_html`.
fn repair_faulty_html<'a>(html: &'a str, beginning: &str) -> Cow<'a, str> {
    let mut s = Cow::Borrowed(html);
    if beginning.contains("doctype") {
        let (first, rest) = html.split_once('\n').unwrap_or((html, ""));
        let fixed = DOCTYPE_TAG.replacen(first, 1, "");
        if matches!(fixed, Cow::Owned(_)) || !html.contains('\n') {
            s = Cow::Owned(format!("{fixed}\n{rest}"));
        }
    }
    let faulty =
        s.splitn(5, '\n').take(4).any(|line| line.contains("<html") && line.trim_end_matches('\r').ends_with("/>"));
    if faulty {
        return Cow::Owned(FAULTY_HTML.replacen(&s, 1, "$1>").into_owned());
    }
    s
}

/// `load_html(htmlobject)` for a str that is not a URL (htmldate downloads those).
pub fn load_html(ctx: &Ctx, html: &str) -> Option<Node> {
    let beginning = html.chars().take(50).collect::<String>().to_lowercase();
    let html = repair_faulty_html(html, &beginning);
    let mut tree = fromstring(ctx, &Input::Str(&html), HTMLDATE_PARSER).ok();
    if tree.is_none_or(|t| t.is_empty()) {
        tree = fromstring(ctx, &Input::Bytes(html.as_bytes()), HTMLDATE_PARSER).ok();
    }
    tree.filter(|t| !(!beginning.contains("html") && t.len() < 2))
}
