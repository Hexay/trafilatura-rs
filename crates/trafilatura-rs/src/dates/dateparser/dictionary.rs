//! `dateparser.languages.dictionary.Dictionary.split` and `are_tokens_valid` for one locale. None defers to Python.
use super::locales::{may_match, pattern, Dict, Locale};
use super::unicode::{is, py_isdigit, word_edge, Class};
use crate::py::pyre::{Caps, Pattern};

pub(super) const ALWAYS_KEEP: [&str; 6] = ["+", ":", ".", " ", "-", "/"];

/// `Dictionary._should_capture` without keep_formatting: KEEP_TOKEN_PATTERN `^.*[^\W_].*$`.
fn should_capture(token: &str) -> bool {
    ALWAYS_KEEP.contains(&token) || token.chars().any(|c| c != '_' && is(Class::Word, c))
}

/// `pattern.split(s)` of a pattern with one group: the pieces with the group text between them.
pub(super) fn split_with_group<'t>(p: &Pattern, s: &'t str) -> Vec<&'t str> {
    let mut out = Vec::new();
    let mut last = 0;
    for caps in p.finditer(s) {
        let m = caps.get(0).expect("group 0");
        out.push(&s[last..m.start()]);
        out.push(caps.get(1).map_or("", |g| g.as_str()));
        last = m.end();
    }
    out.push(&s[last..]);
    out
}

/// `_split_by_numerals`: NUMERAL_PATTERN `(\d+)` split, keeping what `_should_capture`.
fn split_by_numerals(s: &str, out: &mut Vec<String>) {
    let mut run = String::new();
    let mut digits = false;
    for c in s.chars() {
        let d = is(Class::Digit, c);
        if d != digits && !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
        digits = d;
        run.push(c);
    }
    out.push(run);
    out.retain(|t| should_capture(t));
}

/// `regex.match(string, pos)` of `(.*?(?:\G|\W|_|\d))(<words>)(?=\Z|\W|_|\d)`, or of `(.*?)(<words>)` without
/// word spacing: (start, end) of the word.
fn known_word(dict: &Dict, spaced: bool, chars: &[char], pos: usize) -> Option<(usize, usize)> {
    let trie = dict.trie();
    for p in pos..chars.len() {
        if spaced && p > pos && !word_edge(chars[p - 1]) {
            continue;
        }
        for len in trie.prefixes(&chars[p..]) {
            if !spaced || chars.get(p + len).is_none_or(|&c| word_edge(c)) {
                return Some((p, p + len));
            }
        }
    }
    None
}

/// `Dictionary._split_by_known_words(string, keep_formatting=False)`.
fn split_by_known_words(dict: &Dict, spaced: bool, string: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = string.chars().collect();
    let text = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mut pos = 0;
    while pos < chars.len() {
        let mut current = Vec::new();
        match known_word(dict, spaced, &chars, pos) {
            None => {
                let rest = text(pos, chars.len());
                if should_capture(&rest) {
                    split_by_numerals(&rest, &mut current);
                }
                pos = chars.len();
            }
            Some((start, end)) => {
                let (unparsed, known) = (text(pos, start), text(start, end));
                if !unparsed.is_empty() && should_capture(&unparsed) {
                    split_by_numerals(&unparsed, &mut current);
                }
                if should_capture(&known) {
                    current.push(known);
                }
                pos = if end > pos { end } else { chars.len() };
            }
        }
        for token in current {
            if out.last().is_some_and(|t| py_isdigit(t)) && ["st", "nd", "rd", "th"].contains(&token.as_str()) {
                continue;
            }
            out.push(token);
        }
    }
}

/// `Dictionary.split(string)` (keep_formatting False).
pub(super) fn split(locale: &Locale, dict: &Dict, s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    if s.is_empty() {
        return tokens;
    }
    let pieces =
        if may_match(locale.split_relative, s) { split_with_group(pattern(locale.split_relative), s) } else { vec![s] };
    for piece in pieces {
        if relative(locale, piece) {
            tokens.push(piece.to_owned());
        } else {
            let mut words = Vec::new();
            split_by_known_words(dict, !locale.no_word_spacing, piece, &mut words);
            tokens.extend(words);
        }
    }
    tokens.retain(|t| !t.is_empty());
    tokens
}

/// `Dictionary.are_tokens_valid(tokens)`.
pub(super) fn tokens_valid(locale: &Locale, dict: &Dict, tokens: &[String]) -> bool {
    if tokens.iter().all(|t| ALWAYS_KEEP.contains(&t.as_str())) {
        return false;
    }
    tokens.iter().all(|t| py_isdigit(t) || relative(locale, t) || dict.entries.contains_key(t.as_str()))
}

/// Whether the match-relative regex matches at the start of `token`.
fn relative(locale: &Locale, token: &str) -> bool {
    may_match(locale.match_relative, token) && pattern(locale.match_relative).match_(token).is_some()
}

/// Python's expansion of a `sub` template (`\<n>`, `\g<n>`); with `merged`, group 1 is whichever group matched.
/// None for any other escape.
pub(super) fn expand(template: &str, caps: &Caps, merged: bool) -> Option<String> {
    let group = |n: usize| -> &str {
        if merged && n == 1 {
            return (1..caps.len()).find_map(|i| caps.get(i)).map_or("", |m| m.as_str());
        }
        caps.get(n).map_or("", |m| m.as_str())
    };
    let mut out = String::new();
    let mut it = template.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next()? {
            d @ '1'..='9' => {
                let mut n = d.to_digit(10)? as usize;
                if let Some(e) = it.peek().and_then(|e| e.to_digit(10)) {
                    n = n * 10 + e as usize;
                    it.next();
                }
                (n < caps.len()).then_some(())?;
                out.push_str(group(n));
            }
            'g' => {
                (it.next()? == '<').then_some(())?;
                let name: String = it.by_ref().take_while(|&c| c != '>').collect();
                let n: usize = name.parse().ok()?;
                (n < caps.len()).then_some(())?;
                out.push_str(group(n));
            }
            _ => return None,
        }
    }
    Some(out)
}
