//! json_metadata.py `normalize_authors`, with the `encode().decode('unicode_escape')` it applies.
use super::meta_regexes::{
    AUTHOR_EMAIL, AUTHOR_EMOJI_REMOVE, AUTHOR_PREFIX, AUTHOR_REMOVE_NICKNAME, AUTHOR_REMOVE_NUMBERS,
    AUTHOR_REMOVE_PREPOSITION, AUTHOR_REMOVE_SPECIAL, AUTHOR_REPLACE_JOIN, AUTHOR_SPLIT, AUTHOR_TWITTER,
    HTML_STRIP_TAGS,
};
use super::text::trim;
use super::xml_unescape::unescape;
use crate::dom::{Error, Result};
use crate::py::case::title;

fn decode_error(what: &str) -> Error {
    Error::Value(format!("UnicodeDecodeError: {what}"))
}

fn hex_escape(bytes: &[u8], i: &mut usize, digits: usize) -> Result<u32> {
    let hex = bytes.get(*i..*i + digits).filter(|h| h.iter().all(u8::is_ascii_hexdigit));
    let hex = hex.ok_or_else(|| decode_error("truncated escape"))?;
    *i += digits;
    Ok(u32::from_str_radix(std::str::from_utf8(hex).expect("ascii"), 16).expect("hex"))
}

/// `s.encode().decode('unicode_escape')`: bytes outside escapes decode as Latin-1 (so UTF-8 becomes mojibake).
pub fn unicode_escape(s: &str) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b != b'\\' {
            out.push(b as char);
            continue;
        }
        let Some(&c) = bytes.get(i) else { return Err(decode_error("\\ at end of string")) };
        i += 1;
        let code = match c {
            b'\n' => continue,
            b'\\' | b'\'' | b'"' => c as u32,
            b'b' => 8,
            b'f' => 12,
            b't' => 9,
            b'n' => 10,
            b'r' => 13,
            b'v' => 11,
            b'a' => 7,
            b'0'..=b'7' => {
                let mut v = (c - b'0') as u32;
                for _ in 0..2 {
                    match bytes.get(i) {
                        Some(&d @ b'0'..=b'7') => {
                            v = v * 8 + (d - b'0') as u32;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                v
            }
            b'x' => hex_escape(bytes, &mut i, 2)?,
            b'u' => hex_escape(bytes, &mut i, 4)?,
            b'U' => hex_escape(bytes, &mut i, 8)?,
            b'N' => return Err(Error::Unsupported("\\N{...} escape".into())),
            other => {
                out.push('\\');
                out.push(other as char);
                continue;
            }
        };
        if code > 0x10FFFF {
            return Err(decode_error("illegal Unicode character"));
        }
        out.push(char::from_u32(code).ok_or_else(|| Error::Unsupported("lone surrogate".into()))?);
    }
    Ok(out)
}

fn is_upper(c: char) -> bool {
    c.is_uppercase()
}

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
        author_string = unicode_escape(&author_string)?;
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
        if !author.starts_with(is_upper) || !author.chars().any(is_upper) {
            author = title(&author).ok_or_else(|| Error::Unsupported("str.title() with a capital sigma".into()))?;
        }
        if !new_authors.contains(&author) && new_authors.iter().all(|a| !author.contains(a.as_str())) {
            new_authors.push(author);
        }
    }
    if new_authors.is_empty() {
        return keep();
    }
    Ok(Some(new_authors.join("; ").trim_matches(|c| c == ';' || c == ' ').to_owned()))
}
