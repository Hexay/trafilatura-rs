//! `Locale.is_applicable` and `Locale._translate` (dateparser.languages.locale). None defers to Python.
use super::dictionary::{expand, split, tokens_valid, ALWAYS_KEEP};
use super::locales::{may_match, pattern, Dict, Locale, DATA};
use super::unicode::{is, processed, Class};

const MAX_TRANSLATIONS: usize = 16;
const PUNCTUATION: &[char] = &['(', ')', '"', '\'', '{', '}', '[', ']', ',', '.', '،'];
const FRESHNESS: [&str; 10] =
    ["century", "decade", "fortnight", "day", "week", "month", "year", "hour", "minute", "second"];

fn dict(locale: &Locale) -> Option<&'static Dict> {
    locale.dict.map(|d| &DATA.dicts[d])
}

/// `_process_russian_compound_ordinals`' `\b(\d+)\s+(\d+)\b` rewrite, after the simplifications.
fn russian_compounds(s: &str) -> Option<String> {
    let c: Vec<char> = s.chars().collect();
    let w = |i: usize| c.get(i).is_some_and(|&x| is(Class::Word, x));
    let digits = |i: usize| c[i..].iter().take_while(|&&x| is(Class::Digit, x)).count();
    let (mut out, mut i) = (String::new(), 0);
    while i < c.len() {
        let d1 = if i == 0 || !w(i - 1) { digits(i) } else { 0 };
        let sp = c[i + d1..].iter().take_while(|&&x| is(Class::Space, x)).count();
        let d2 = if d1 > 0 && sp > 0 { digits(i + d1 + sp) } else { 0 };
        let end = i + d1 + sp + d2;
        if d2 == 0 || w(end) {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let (first, second): (String, String) = (c[i..i + d1].iter().collect(), c[end - d2..end].iter().collect());
        let whole: String = c[i..end].iter().collect();
        let sum = if (first == "20" || first == "30") && second.len() == 1 && second != "0" {
            Some(first.parse::<u32>().ok()? + second.parse::<u32>().ok()?).filter(|&n| n <= 31)
        } else {
            None
        };
        out.push_str(&sum.map_or(whole, |n| n.to_string()));
        i = end;
    }
    Some(out)
}

/// `Locale._simplify` of an already processed (lowercase) string.
fn simplify(locale: &Locale, s: &str) -> Option<String> {
    let mut s: String = if locale.letter_variants {
        s.chars().map(|c| *DATA.variants.get(&c).unwrap_or(&c)).collect()
    } else {
        s.to_owned()
    };
    for &(n, replacement) in &locale.simplifications {
        let p = pattern(n);
        if !may_match(n, &s) || p.search(&s).is_none() {
            continue;
        }
        let mut failed = false;
        let mut empty = false;
        let out = p.sub_with(&s, 0, |caps| {
            empty |= caps.get(0).is_some_and(|m| m.as_str().is_empty());
            expand(replacement, caps, false).unwrap_or_else(|| {
                failed = true;
                String::new()
            })
        });
        if failed || empty {
            return None;
        }
        // replacements are lowercase (checked by the generator), the rest was lowercase already
        s = out;
    }
    Some(if locale.name == "ru" { russian_compounds(&s)? } else { s })
}

/// The tokens a locale splits `date_string` into, and its dictionary.
fn tokens(locale: &Locale, date_string: &str) -> Option<(Vec<String>, &'static Dict)> {
    let dict = dict(locale)?;
    let s = processed(date_string)?;
    if s.contains('\n') {
        return None;
    }
    let s = simplify(locale, &s)?;
    Some((split(locale, dict, &s), dict))
}

/// `locale.is_applicable(date_string, strip_timezone=False)`.
pub(super) fn is_applicable(locale: &Locale, date_string: &str) -> Option<bool> {
    let (tokens, dict) = tokens(locale, date_string)?;
    Some(tokens_valid(locale, dict, &tokens))
}

fn clear_future_words(words: &mut [String]) {
    let bare = |w: &String| w.trim_matches(PUNCTUATION).to_owned();
    if !words.iter().filter(|w| !w.is_empty()).map(bare).any(|w| FRESHNESS.contains(&w.as_str())) {
        if let Some(i) = words.iter().position(|w| w == "in") {
            words[i].clear();
        }
    }
}

/// Drops the empty tokens; between spaces, keeps the longer run of spaces around one.
fn drop_empty(tokens: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].is_empty() {
            out.push(tokens[i].clone());
            i += 1;
            continue;
        }
        let prev = out.iter().rev().take_while(|t| *t == " ").count();
        let next = tokens[i + 1..].iter().take_while(|t| *t == " ").count();
        if prev > 0 && next > 0 {
            out.truncate(out.len() - prev);
            out.extend(std::iter::repeat_n(" ".to_owned(), prev.max(next)));
            i += next + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// `Locale._join(tokens, " ")`.
fn join(tokens: &[String]) -> String {
    let mut joined = tokens.first().cloned().unwrap_or_default();
    for pair in tokens.windows(2) {
        if !ALWAYS_KEEP.contains(&pair[0].as_str()) && !ALWAYS_KEEP.contains(&pair[1].as_str()) {
            joined.push(' ');
        }
        joined.push_str(&pair[1]);
    }
    joined
}

/// `locale._translate(date_string, keep_formatting=False, alternative=alternative)`: Some(None) where there is no
/// such combination of meanings.
pub(super) fn translate(locale: &Locale, date_string: &str, alternative: usize) -> Option<Option<String>> {
    if alternative >= MAX_TRANSLATIONS {
        return Some(None);
    }
    let (mut tokens, dict) = tokens(locale, date_string)?;
    let unambiguous_month = tokens.iter().any(|t| dict.unambiguous_month(t));
    let mut ambiguous: Vec<(usize, &Vec<&str>)> = Vec::new();
    'tokens: for (i, token) in tokens.iter_mut().enumerate() {
        let word = token.clone();
        for &(n, replacement, merged) in &locale.relative {
            if !may_match(n, &word) {
                continue;
            }
            if let Some(caps) = pattern(n).match_(&word) {
                let m = caps.get(0).expect("group 0");
                let sub = expand(replacement, &caps, merged)?;
                *token = format!("{}{sub}{}", &word[..m.start()], &word[m.end()..]);
                continue 'tokens;
            }
        }
        if unambiguous_month && dict.conflicts.contains_key(word.as_str()) {
            *token = dict.conflicts[word.as_str()].to_owned();
        } else if let Some(value) = dict.entries.get(word.as_str()) {
            *token = value.unwrap_or("").to_owned();
            if let Some(meanings) = dict.meanings.get(word.as_str()).filter(|_| value.is_some()) {
                ambiguous.push((i, meanings));
            }
        }
    }
    if alternative > 0 {
        // itertools.product order: the last token's meaning varies fastest
        let mut rest = alternative;
        let total: usize =
            ambiguous.iter().map(|(_, m)| m.len()).try_fold(1usize, |a, n| a.checked_mul(n)).unwrap_or(usize::MAX);
        if alternative >= total {
            return Some(None);
        }
        for (i, meanings) in ambiguous.iter().rev() {
            tokens[*i] = meanings[rest % meanings.len()].to_owned();
            rest /= meanings.len();
        }
    }
    if tokens.iter().any(|t| t == "in") {
        clear_future_words(&mut tokens);
    }
    Some(Some(join(&drop_empty(&tokens))))
}
