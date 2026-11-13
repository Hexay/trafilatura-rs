//! Character properties of the native dateparser port, from unicode.txt (harness/gen_dateparser_unicode.py).
use std::collections::HashMap;
use std::sync::LazyLock;

const SOURCE: &str = include_str!("unicode.txt");

struct Table {
    versions: (&'static str, &'static str),
    mapped: HashMap<u32, Vec<char>>,
    unknown: Vec<(u32, u32)>,
    hangul: (u32, u32),
    fold: HashMap<char, char>,
    classes: [Vec<(u32, u32)>; 6],
}

/// Property ranges, in the order of `Class`.
const TAGS: [&str; 6] = ["W", "D", "S", "A", "N", "B"];

#[derive(Clone, Copy)]
pub(super) enum Class {
    /// `regex` \w
    Word,
    /// `regex` \d
    Digit,
    /// `regex` \s
    Space,
    /// str.isalpha
    Alpha,
    /// str.isdigit
    PyDigit,
    /// str.isspace
    PySpace,
}

fn hex(field: &str) -> u32 {
    u32::from_str_radix(field, 16).unwrap_or_else(|_| panic!("bad code point {field:?} in unicode.txt"))
}

fn ch(field: &str) -> char {
    char::from_u32(hex(field)).expect("a scalar value")
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    let mut t = Table {
        versions: ("", ""),
        mapped: HashMap::new(),
        unknown: Vec::new(),
        hangul: (1, 0),
        fold: HashMap::new(),
        classes: Default::default(),
    };
    for line in SOURCE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&'static str> = line.split(' ').collect();
        match f[0] {
            "V" => t.versions = (f[1], f[2]),
            "P" => _ = t.mapped.insert(hex(f[1]), f[2..].iter().map(|x| ch(x)).collect()),
            "U" => t.unknown.push((hex(f[1]), hex(f[2]))),
            "H" => t.hangul = (hex(f[1]), hex(f[2])),
            "F" => _ = t.fold.insert(ch(f[1]), ch(f[2])),
            // str.isdecimal: _translate_numerals' digits are already in the P lines
            "E" => {}
            tag => {
                let i = TAGS.iter().position(|&x| x == tag).unwrap_or_else(|| panic!("bad unicode.txt line {line:?}"));
                t.classes[i].push((hex(f[1]), hex(f[2])));
            }
        }
    }
    t
});

fn in_ranges(ranges: &[(u32, u32)], c: char) -> bool {
    let i = ranges.partition_point(|r| r.0 <= c as u32);
    i > 0 && ranges[i - 1].1 >= c as u32
}

/// (`unicodedata.unidata_version`, `regex.__version__`) the table holds for.
pub(super) fn versions() -> (&'static str, &'static str) {
    TABLE.versions
}

pub(super) fn is(class: Class, c: char) -> bool {
    in_ranges(&TABLE.classes[class as usize], c)
}

/// The representative of `c`'s `regex` IGNORECASE class.
pub(super) fn fold(c: char) -> char {
    *TABLE.fold.get(&c).unwrap_or(&c)
}

/// `_translate_numerals`, `normalize_unicode` and `str.lower()` of `s`, as a locale reads it; None when a character's
/// result is not known per character.
pub(super) fn processed(s: &str) -> Option<String> {
    let t = &*TABLE;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        if let Some(m) = t.mapped.get(&cp) {
            out.extend(m);
        } else if (t.hangul.0..=t.hangul.1).contains(&cp) {
            let s = cp - t.hangul.0;
            let jamo = [0x1100 + s / 588, 0x1161 + s % 588 / 28, 0x11A7 + s % 28];
            out.extend(jamo[..if s.is_multiple_of(28) { 2 } else { 3 }].iter().filter_map(|&j| char::from_u32(j)));
        } else if in_ranges(&t.unknown, c) {
            return None;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Python's `str.strip()`.
pub(super) fn py_strip(s: &str) -> &str {
    s.trim_matches(|c| is(Class::PySpace, c))
}

/// Python's `str.isdigit()`.
pub(super) fn py_isdigit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| is(Class::PyDigit, c))
}

/// `regex` `\W|_|\d` (the boundary of a dictionary word).
pub(super) fn word_edge(c: char) -> bool {
    !is(Class::Word, c) || c == '_' || is(Class::Digit, c)
}
