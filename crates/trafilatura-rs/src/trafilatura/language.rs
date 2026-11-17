//! `target_language` (utils.py and external.py, identical in 1.11 and 2.3.1): `check_html_lang`, `language_filter`
//! and the jusText stoplist `try_justext` picks. Language detection itself is a `Hooks` leaf (py3langid).
use super::document::Document;
use super::filters::py_lower;
use super::settings::Options;
use crate::dom::{Error, Node, Result};
use crate::hooks::Hooks;
use crate::justext::Stoplist;
use crate::py::value::Value;
use std::sync::Arc;

/// `TARGET_LANG_ATTRS` as the `findall` paths `check_html_lang` builds from them.
const TARGET_LANG_PATHS: [(&str, &str); 2] = [("http-equiv", "content-language"), ("property", "og:locale")];

/// `RE_HTML_LANG.split(s)` with `RE_HTML_LANG = re.compile(r"([a-z]{2})")`: the text between matches and the matches.
fn split_html_lang(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let (mut parts, mut last, mut i) = (Vec::new(), 0, 0);
    while i + 1 < b.len() {
        if b[i].is_ascii_lowercase() && b[i + 1].is_ascii_lowercase() {
            parts.extend([&s[last..i], &s[i..i + 2]]);
            last = i + 2;
            i += 2;
        } else {
            i += 1;
        }
    }
    parts.push(&s[last..]);
    parts
}

/// `check_html_lang(tree, target_language)` (strict=False): False when the first `<meta>` language attribute kind
/// present names other languages only.
pub fn check_html_lang(tree: Node, target: &str) -> Result<bool> {
    for (attr, value) in TARGET_LANG_PATHS {
        let elems: Vec<Node> = tree
            .iter_descendants(&["meta"])
            .filter(|e| e.get(attr).as_deref() == Some(value) && e.has_attr("content"))
            .collect();
        if elems.is_empty() {
            continue;
        }
        for elem in elems {
            if split_html_lang(&py_lower(&elem.get("content").unwrap_or_default())?).contains(&target) {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(true)
}

/// The quick check `bare_extraction` makes right after loading the tree: ValueError for a wrong `<meta>` language
/// when the detector is off (fast mode or py3langid missing).
pub fn check_lang_early(tree: Node, opts: &Options) -> Result<()> {
    match opts.lang.as_deref() {
        Some(lang) if (opts.fast || !opts.langid) && !check_html_lang(tree, lang)? => {
            Err(Error::Value("wrong HTML meta language".into()))
        }
        _ => Ok(()),
    }
}

/// `language_filter(temp_text, temp_comments, target_language, document)` and the ValueError `bare_extraction`
/// raises when the detected language is another.
pub fn language_filter(
    doc: &mut Document,
    text: &str,
    comments: &str,
    opts: &Options,
    hooks: &dyn Hooks,
) -> Result<()> {
    let Some(target) = opts.lang.as_deref() else { return Ok(()) };
    let detected = hooks.language_classifier(text, comments)?;
    doc.language = detected.clone().map_or(Value::None, Value::Str);
    match detected {
        Some(lang) if lang != target => Err(Error::Value("wrong language".into())),
        _ => Ok(()),
    }
}

/// The stoplist `try_justext(tree, url, target_language)` uses: the target language's, else JT_STOPLIST.
pub fn justext_stoplist(opts: &Options, hooks: &dyn Hooks) -> Result<StoplistRef> {
    Ok(StoplistRef(match opts.lang.as_deref() {
        Some(lang) => hooks.justext_stoplist(lang)?,
        None => None,
    }))
}

/// A language stoplist, or None for JT_STOPLIST.
pub struct StoplistRef(Option<Arc<Stoplist>>);

impl StoplistRef {
    pub fn get(&self) -> &Stoplist {
        self.0.as_deref().unwrap_or(Stoplist::jt())
    }
}

#[cfg(test)]
mod tests {
    use super::split_html_lang;

    #[test]
    fn splits_like_python() {
        assert_eq!(split_html_lang("en-us"), ["", "en", "-", "us", ""]);
        assert_eq!(split_html_lang("eng"), ["", "en", "g"]);
        assert_eq!(split_html_lang(""), [""]);
        assert_eq!(split_html_lang("fr_FR"), ["", "fr", "_FR"]);
    }
}
