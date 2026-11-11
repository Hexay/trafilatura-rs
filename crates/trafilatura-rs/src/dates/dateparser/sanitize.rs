//! `dateparser.date.sanitize_date` for printable ASCII strings, as far as it changes letters and spaces; None where
//! a short-cut is unsafe (a Roman numeral year or a calendar quarter rewrites the string).

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn digits_end(s: &[u8], i: usize) -> usize {
    i + s[i..].iter().take_while(|&&b| is_digit(b)).count()
}

/// RE_SANITIZE_CROATIAN: `(?<!\d)(\d+)\.\s?(\d+)\.\s?(\d+)\.( u)?` → `\1.\2.\3 ` (re.I).
fn croatian(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let (mut i, mut copied) = (0, 0);
    while i < b.len() {
        if !is_digit(b[i]) || (i > 0 && is_digit(b[i - 1])) {
            i += 1;
            continue;
        }
        let mut groups = Vec::new();
        let mut j = i;
        for k in 0..3 {
            if k > 0 && b.get(j) == Some(&b' ') && b.get(j + 1).copied().is_some_and(is_digit) {
                j += 1;
            }
            let end = digits_end(b, j);
            if end == j || b.get(end) != Some(&b'.') {
                break;
            }
            groups.push(&s[j..end]);
            j = end + 1;
        }
        if groups.len() < 3 {
            i += 1;
            continue;
        }
        if b.get(j) == Some(&b' ') && matches!(b.get(j + 1), Some(b'u' | b'U')) {
            j += 2;
        }
        out.push_str(&s[copied..i]);
        out.push_str(&format!("{}.{}.{} ", groups[0], groups[1], groups[2]));
        copied = j;
        i = j;
    }
    out.push_str(&s[copied..]);
    out
}

/// `sanitize_spaces`: collapse runs of spaces; RE_TRIM_SPACES only trims when both ends are spaces.
fn spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if !(c == ' ' && out.ends_with(' ')) {
            out.push(c);
        }
    }
    if out.starts_with(' ') && out.ends_with(' ') && !out.trim().is_empty() {
        return out.trim_matches(' ').to_owned();
    }
    out
}

/// RE_SANITIZE_PERIOD: drop each '.' preceded by neither a digit nor whitespace.
fn periods(s: &str) -> String {
    let b = s.as_bytes();
    let keep = |i: usize| b[i] != b'.' || i == 0 || is_digit(b[i - 1]) || b[i - 1] == b' ';
    (0..b.len()).filter(|&i| keep(i)).map(|i| b[i] as char).collect()
}

/// Whether `_RE_ROMAN_YEAR` may match: a word of Roman numeral letters starting with M.
fn has_roman_year(s: &str) -> bool {
    s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| w.starts_with('M') && w.bytes().all(|b| b"MDCLXVI".contains(&b)))
}

/// RE_SANITIZE_ON: `^.*?on:\s+(.*)` → `\1`.
fn after_on(s: &str) -> &str {
    let mut from = 0;
    while let Some(p) = s[from..].find("on:") {
        let rest = &s[from + p + 3..];
        if rest.starts_with(' ') {
            return rest.trim_start_matches(' ');
        }
        from += p + 1;
    }
    s
}

/// Whether `_RE_QUARTER` may fullmatch (its only letter is Q).
fn may_be_quarter(s: &str) -> bool {
    s.bytes().filter(u8::is_ascii_alphabetic).all(|b| b == b'q' || b == b'Q')
}

/// `sanitize_date(s)` then `_replace_quarter`; None where the date string would be rewritten beyond that.
pub(super) fn sanitize(s: &str) -> Option<String> {
    let s = croatian(s);
    let s = spaces(&s);
    let s = periods(&s);
    if has_roman_year(&s) {
        return None;
    }
    let s = after_on(&s).trim_end_matches(':').trim_matches(' ').to_owned();
    (!may_be_quarter(&s)).then_some(s)
}
