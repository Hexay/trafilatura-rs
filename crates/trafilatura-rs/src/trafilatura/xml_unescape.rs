//! Python 3.12 `html.unescape` (HTML5 named and numeric character references).
use super::tables::{HTML5, INVALID_CHARREFS, INVALID_CODEPOINTS};
use regex::{Captures, Regex};
use std::sync::LazyLock;

static CHARREF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(#[0-9]+;?|#[xX][0-9a-fA-F]+;?|[^\t\n\x0C <&#;]{1,32};?)").unwrap());

fn html5(name: &str) -> Option<&'static str> {
    HTML5.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| HTML5[i].1)
}

fn numeric(digits: &str, radix: u32) -> String {
    // Saturation is safe: anything above 0x10FFFF maps to U+FFFD like Python's bigint path.
    let num = digits.chars().fold(0u64, |acc, c| {
        acc.saturating_mul(radix as u64).saturating_add(c.to_digit(radix).expect("regex digit") as u64)
    });
    if let Some(&(_, s)) = INVALID_CHARREFS.iter().find(|(k, _)| *k as u64 == num) {
        return s.to_owned();
    }
    if (0xD800..=0xDFFF).contains(&num) || num > 0x10FFFF {
        return "\u{FFFD}".to_owned();
    }
    if INVALID_CODEPOINTS.contains(&(num as u32)) {
        return String::new();
    }
    char::from_u32(num as u32).expect("valid scalar").to_string()
}

fn replace_charref(caps: &Captures) -> String {
    let s = &caps[1];
    if let Some(num) = s.strip_prefix('#') {
        let num = num.trim_end_matches(';');
        return match num.strip_prefix(['x', 'X']) {
            Some(hex) => numeric(hex, 16),
            None => numeric(num, 10),
        };
    }
    if let Some(v) = html5(s) {
        return v.to_owned();
    }
    let bounds: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    for x in (2..bounds.len()).rev() {
        if let Some(v) = html5(&s[..bounds[x]]) {
            return format!("{v}{}", &s[bounds[x]..]);
        }
    }
    format!("&{s}")
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    CHARREF.replace_all(s, replace_charref).into_owned()
}
