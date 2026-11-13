//! `dateparser.date.sanitize_date` (its patterns are `regex`-module ones); None where it is not ported (`, в`, `г.`,
//! a Roman numeral year, a calendar quarter, a space only one of `str.strip` and `\s` knows).
use super::chars::{is_digit, is_ghe, is_space, look_alike};
use super::unicode::{is, Class};

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
        let end = (self.len() - self.iter().rev().take_while(|&&x| x == c).count()).max(start);
        self.truncate(end);
        self.drain(..start);
    }
}

/// RE_SANITIZE_PERIOD `(?<=[^0-9\s])\.`; after `spaces` the only \s left is the space.
fn periods(s: &[char]) -> Vec<char> {
    let keep = |i: usize| s[i] != '.' || i == 0 || s[i - 1].is_ascii_digit() || s[i - 1] == ' ';
    (0..s.len()).filter(|&i| keep(i)).map(|i| s[i]).collect()
}

/// Whether `_RE_ROMAN_YEAR` may match a numeral of 3 letters or more (shorter ones neither change nor make a short
/// Roman year): a word of Roman numeral letters starting with M (its `\b`s fall at least between any characters this
/// splits on).
fn has_roman_year(s: &[char]) -> bool {
    s.split(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
        .any(|w| w.len() >= 3 && w[0] == 'M' && w.iter().all(|c| "MDCLXVI".contains(*c)))
}

/// RE_SANITIZE_DECIMAL_COMMA `(?<=\d:\d{2}:\d{2}),(?=(\d{3,})(\s*[ap]\.?m\b)?)` (re.I) with `_decimal_comma_to_point`.
fn decimal_commas(s: &mut [char]) {
    let d = |s: &[char], i: usize| s.get(i).copied().is_some_and(is_digit);
    let year_end = (0..s.len()).find(|&i| (i..i + 4).all(|k| d(s, k))).map_or(s.len(), |i| i + 4);
    for i in 7..s.len() {
        let behind = [0, 2, 3, 5, 6].iter().all(|&k| d(s, i - 7 + k)) && s[i - 6] == ':' && s[i - 3] == ':';
        if s[i] != ',' || !behind {
            continue;
        }
        let run = s[i + 1..].iter().take_while(|&&c| is_digit(c)).count();
        if run < 3 {
            continue;
        }
        let mut j = i + 1 + run;
        j += s[j..].iter().take_while(|&&c| is_space(c)).count();
        let meridiem = matches!(s.get(j), Some('a' | 'A' | 'p' | 'P')) && {
            j += 1 + usize::from(s.get(j + 1) == Some(&'.'));
            matches!(s.get(j), Some('m' | 'M')) && !s.get(j + 1).is_some_and(|&c| is(Class::Word, c))
        };
        s[i] = if run == 4 && !meridiem && year_end > i { ',' } else { '.' };
    }
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

/// Whether `_RE_QUARTER` fullmatches (`\s*(?:(19|20)\d\d[-\s]?Q0?[1-4]|Q0?[1-4][-\s]?(19|20)\d\d)\s*`, re.I).
fn quarter(s: &[char]) -> bool {
    let start = s.iter().take_while(|&&c| is_space(c)).count();
    let end = s.len() - s[start..].iter().rev().take_while(|&&c| is_space(c)).count();
    let m = &s[start..end];
    let year =
        |y: &[char]| y.len() == 4 && matches!(y[..2], ['1', '9'] | ['2', '0']) && is_digit(y[2]) && is_digit(y[3]);
    let sep = |c: Option<&char>| c.is_some_and(|&c| c == '-' || is_space(c));
    let q = |q: &[char]| matches!(q, ['q' | 'Q', '0', '1'..='4'] | ['q' | 'Q', '1'..='4']);
    let n = m.len();
    n >= 6
        && ((year(&m[..4]) && (q(&m[4..]) || (sep(m.get(4)) && q(&m[5..]))))
            || (year(&m[n - 4..]) && (q(&m[..n - 4]) || (sep(m.get(n - 5)) && q(&m[..n - 5])))))
}

/// `sanitize_date(s)` then `_replace_quarter`; None where the date string would be rewritten beyond that.
pub(super) fn sanitize(s: &str) -> Option<String> {
    let s: Vec<char> = s.chars().collect();
    // str.strip() and \s disagree on these
    if s.iter().any(|&c| is(Class::PySpace, c) != is_space(c)) {
        return None;
    }
    let s = skip(&s)?;
    if s.windows(2).any(|w| is_ghe(w[0]) && w[1] == '.') {
        return None;
    }
    let mut s = periods(&spaces(&croatian(&s)));
    if has_roman_year(&s) {
        return None;
    }
    decimal_commas(&mut s);
    let mut s = after_on(&s).to_vec();
    while s.last() == Some(&':') {
        s.pop();
    }
    for c in s.iter_mut() {
        *c = look_alike(*c).unwrap_or(*c);
    }
    s.retain_trimmed(' ');
    (!quarter(&s)).then(|| s.into_iter().collect())
}
