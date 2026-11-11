//! trafilatura 2.3.1 json_metadata.py `normalize_authors`.
use super::meta_regexes::{AUTHOR_EMAIL, AUTHOR_SPLIT};
use crate::dom::{Error, Result};
use crate::py::case::title;
use crate::trafilatura::json_metadata_authors::unicode_escape;
use crate::trafilatura::meta_regexes::{
    AUTHOR_EMOJI_REMOVE, AUTHOR_PREFIX, AUTHOR_REMOVE_NICKNAME, AUTHOR_REMOVE_NUMBERS, AUTHOR_REMOVE_PREPOSITION,
    AUTHOR_REMOVE_SPECIAL, AUTHOR_REPLACE_JOIN, AUTHOR_TWITTER, HTML_STRIP_TAGS,
};
use crate::trafilatura::text::trim;
use crate::trafilatura::xml_unescape::unescape;

/// `normalize_authors`: the updated author string ("; "-separated), or `current_authors` unchanged.
pub fn normalize_authors(current_authors: Option<&str>, author_string: &str) -> Result<Option<String>> {
    let keep = || Ok(current_authors.map(str::to_owned));
    if author_string.to_lowercase().starts_with("http") || AUTHOR_EMAIL.match_(author_string).is_some() {
        return keep();
    }
    let mut new_authors: Vec<String> =
        current_authors.map(|c| c.split("; ").map(str::to_owned).collect()).unwrap_or_default();
    let mut author_string = author_string.to_owned();
    if author_string.contains("\\u") {
        // UnicodeDecodeError is caught and the string kept
        match unicode_escape(&author_string) {
            Ok(decoded) => author_string = decoded,
            Err(Error::Value(_)) => {}
            Err(e) => return Err(e),
        }
    }
    if author_string.contains("&#") || author_string.contains("&amp;") {
        author_string = unescape(&author_string);
    }
    let author_string = HTML_STRIP_TAGS.sub(&author_string, "");
    for author in AUTHOR_SPLIT.split(&author_string) {
        let mut author = trim(author);
        author = AUTHOR_EMOJI_REMOVE.sub(&author, "");
        author = AUTHOR_TWITTER.sub(&author, "");
        author = trim(&AUTHOR_REPLACE_JOIN.sub(&author, " "));
        author = AUTHOR_REMOVE_NICKNAME.sub(&author, "");
        author = AUTHOR_REMOVE_SPECIAL.sub(&author, "");
        author = AUTHOR_PREFIX.sub(&author, "");
        author = AUTHOR_REMOVE_NUMBERS.sub(&author, "");
        author = AUTHOR_REMOVE_PREPOSITION.sub(&author, "");
        if author.is_empty() || (crate::py::len(&author) >= 50 && !author.contains(' ') && !author.contains('-')) {
            continue;
        }
        if !author.starts_with(char::is_uppercase) {
            author = title(&author).ok_or_else(|| Error::Unsupported("str.title() with a capital sigma".into()))?;
        }
        if !new_authors.contains(&author) {
            new_authors.push(author);
        }
    }
    let kept: Vec<&String> =
        new_authors.iter().filter(|n| !new_authors.iter().any(|m| n != &m && m.contains(n.as_str()))).collect();
    if kept.is_empty() {
        return keep();
    }
    let joined = kept.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; ");
    Ok(Some(joined.trim_matches(|c| c == ';' || c == ' ').to_owned()))
}
