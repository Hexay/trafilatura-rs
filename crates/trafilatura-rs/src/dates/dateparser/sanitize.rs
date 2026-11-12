//! `dateparser.date.sanitize_date` (its patterns are `regex`-module ones), as far as it changes letters and word
//! boundaries; None where a short-cut is unsafe (`, в`, `г.`, a Roman numeral year or a calendar quarter).
use super::chars::{is_digit, is_ghe, is_space, look_alike, push_letters};

/// RE_SANITIZE_SKIP: these become a space; None for its `,\sв\b` alternative.
fn skip(s: &[char]) -> Option<Vec<char>> {
    if s.windows(3).any(|w| w[0] == ',' && is_space(w[1]) && w[2] == 'в') {
        return None;
    }
    let to_space =
        |c: char| matches!(c, '\t' | '\n' | '\r' | '»' | '\u{200e}' | '·' | '\u{200f}' | '\u{64e}' | '\u{64f}');
    Some(s.iter().map(|&c| if to_space(c) { ' ' } else { c }).collect())
}

fn digits_end(s: &[char], i: usize) -> usize {
    i + s[i..].iter().take_while(|&&c| is_digit(c)).count()
}

/// RE_SANITIZE_CROATIAN: `(?<!\d)(\d+)\.\s?(\d+)\.\s?(\d+)\.( u)?` → `\1.\2.\3 ` (re.I).
fn croatian(s: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if !is_digit(s[i]) || (i > 0 && is_digit(s[i - 1])) {
            out.push(s[i]);
            i += 1;
            continue;
        }
        let mut groups = Vec::new();
        let mut j = i;
        for k in 0..3 {
            if k > 0 && s.get(j).copied().is_some_and(is_space) && s.get(j + 1).copied().is_some_and(is_digit) {
                j += 1;
            }
            let end = digits_end(s, j);
            if end == j || s.get(end) != Some(&'.') {
                break;
            }
            groups.push(j..end);
            j = end + 1;
        }
        if groups.len() < 3 {
            out.push(s[i]);
            i += 1;
            continue;
        }
        if s.get(j) == Some(&' ') && matches!(s.get(j + 1), Some('u' | 'U')) {
            j += 2;
        }
        for (k, g) in groups.into_iter().enumerate() {
            out.extend_from_slice(&s[g]);
            out.push(if k < 2 { '.' } else { ' ' });
        }
        i = j;
    }
    out
}

/// `sanitize_spaces`: runs of \s (NBSP included) become one space; RE_TRIM_SPACES trims only when both ends are.
fn spaces(s: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(s.len());
    for &c in s {
        let c = if is_space(c) { ' ' } else { c };
        if !(c == ' ' && out.last() == Some(&' ')) {
            out.push(c);
        }
    }
    if out.first() == Some(&' ') && out.last() == Some(&' ') && out.iter().any(|&c| c != ' ') {
        out.retain_trimmed(' ');
    }
    out
}

trait Trim {
    fn retain_trimmed(&mut self, c: char);
}

impl Trim for Vec<char> {
    fn retain_trimmed(&mut self, c: char) {
        let start = self.iter().take_while(|&&x| x == c).count();
        let end = self.len() - self.iter().rev().take_while(|&&x| x == c).count();
        self.truncate(end);
        self.drain(..start);
    }
}

/// RE_SANITIZE_PERIOD `(?<=[^0-9\s])\.`; after `spaces` the only \s left is the space.
fn periods(s: &[char]) -> Vec<char> {
    let keep = |i: usize| s[i] != '.' || i == 0 || s[i - 1].is_ascii_digit() || s[i - 1] == ' ';
    (0..s.len()).filter(|&i| keep(i)).map(|i| s[i]).collect()
}

/// Whether `_RE_ROMAN_YEAR` may match: a word of Roman numeral letters starting with M (its `\b`s fall at least
/// between any characters this splits on).
fn has_roman_year(s: &[char]) -> bool {
    s.split(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
        .any(|w| w.first() == Some(&'M') && w.iter().all(|c| "MDCLXVI".contains(*c)))
}

/// RE_SANITIZE_ON: `^.*?on:\s+(.*)` → `\1`.
fn after_on(s: &[char]) -> &[char] {
    for i in 0..s.len().saturating_sub(3) {
        if s[i..i + 3] == ['o', 'n', ':'] && s[i + 3] == ' ' {
            let rest = &s[i + 3..];
            return &rest[rest.iter().take_while(|&&c| c == ' ').count()..];
        }
    }
    s
}

/// Whether `_RE_QUARTER` may fullmatch (its only letter is Q).
fn may_be_quarter(s: &[char]) -> bool {
    s.iter().all(|&c| matches!(c, 'q' | 'Q') || push_letters(c, &mut String::new()) == Some(false))
}

/// `sanitize_date(s)` then `_replace_quarter`; None where the date string would be rewritten beyond that.
pub(super) fn sanitize(s: &str) -> Option<String> {
    let s: Vec<char> = s.chars().collect();
    let s = skip(&s)?;
    if s.windows(2).any(|w| is_ghe(w[0]) && w[1] == '.') {
        return None;
    }
    let s = periods(&spaces(&croatian(&s)));
    if has_roman_year(&s) {
        return None;
    }
    let mut s = after_on(&s).to_vec();
    while s.last() == Some(&':') {
        s.pop();
    }
    for c in s.iter_mut() {
        *c = look_alike(*c).unwrap_or(*c);
    }
    s.retain_trimmed(' ');
    (!may_be_quarter(&s)).then(|| s.into_iter().collect())
}
