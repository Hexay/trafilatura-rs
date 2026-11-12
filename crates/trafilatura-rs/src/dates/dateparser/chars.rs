//! Character classes for the dateparser shortcut, from chars.txt (harness/gen_dateparser_chars.py): what a locale
//! sees of each character after `_translate_numerals`, `normalize_unicode` and `str.lower()`, folded case-insensitively.
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const CHARS: &str = include_str!("chars.txt");

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Identity,
    Mark,
    Other,
}

struct Chars {
    /// (unidata_version, regex version) the table holds for
    versions: (&'static str, &'static str),
    ranges: Vec<(u32, u32, Kind)>,
    mapped: HashMap<u32, Vec<char>>,
    hangul: (u32, u32),
    digits: Vec<(u32, u32)>,
    spaces: HashSet<char>,
    tz_fold: HashMap<char, char>,
    ghe: HashSet<char>,
    apostrophes: HashSet<char>,
    dashes: HashSet<char>,
}

fn hex(field: &str) -> u32 {
    u32::from_str_radix(field, 16).unwrap_or_else(|_| panic!("bad code point {field:?} in chars.txt"))
}

fn ch(field: &str) -> char {
    char::from_u32(hex(field)).expect("a scalar value")
}

static TABLE: LazyLock<Chars> = LazyLock::new(|| {
    let mut t = Chars {
        versions: ("", ""),
        ranges: Vec::new(),
        mapped: HashMap::new(),
        hangul: (1, 0),
        digits: Vec::new(),
        spaces: HashSet::new(),
        tz_fold: HashMap::new(),
        ghe: HashSet::new(),
        apostrophes: HashSet::new(),
        dashes: HashSet::new(),
    };
    for line in CHARS.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "V" => t.versions = (f[1], f[2]),
            "R" => {
                let kind = match f[3] {
                    "I" => Kind::Identity,
                    "M" => Kind::Mark,
                    _ => Kind::Other,
                };
                t.ranges.push((hex(f[1]), hex(f[2]), kind));
            }
            "P" => _ = t.mapped.insert(hex(f[1]), f[2..].iter().map(|x| ch(x)).collect()),
            "H" => t.hangul = (hex(f[1]), hex(f[2])),
            "D" => t.digits.push((hex(f[1]), hex(f[2]))),
            "S" => t.spaces.extend(f[1..].iter().map(|x| ch(x))),
            "A" => _ = t.tz_fold.insert(ch(f[1]), ch(f[2])),
            "G" => t.ghe.extend(f[1..].iter().map(|x| ch(x))),
            "Q" => t.apostrophes.extend(f[1..].iter().map(|x| ch(x))),
            "Z" => t.dashes.extend(f[1..].iter().map(|x| ch(x))),
            _ => panic!("malformed chars.txt line {line:?}"),
        }
    }
    t.ranges.sort_unstable_by_key(|r| r.0);
    t
});

fn in_ranges<T>(ranges: &[(u32, u32, T)], c: u32) -> Option<&T> {
    let i = ranges.partition_point(|r| r.0 <= c).checked_sub(1)?;
    (ranges[i].1 >= c).then(|| &ranges[i].2)
}

/// (`unicodedata.unidata_version`, `regex.__version__`) the table was generated with.
pub(crate) fn versions() -> (&'static str, &'static str) {
    TABLE.versions
}

/// Appends what a locale sees of `c` (folded letters) to `out`: Some(true) for a letter (`[^\W\d_]`), which may
/// add nothing, Some(false) for anything else, which adds nothing; None when unknown.
pub(super) fn push_letters(c: char, out: &mut String) -> Option<bool> {
    let t = &*TABLE;
    let cp = c as u32;
    if let Some(letters) = t.mapped.get(&cp) {
        out.extend(letters);
        return Some(true);
    }
    if (t.hangul.0..=t.hangul.1).contains(&cp) {
        let s = cp - t.hangul.0;
        let jamo = [0x1100 + s / 588, 0x1161 + s % 588 / 28, 0x11A7 + s % 28];
        out.extend(jamo[..if s.is_multiple_of(28) { 2 } else { 3 }].iter().filter_map(|&j| char::from_u32(j)));
        return Some(true);
    }
    match in_ranges(&t.ranges, cp)? {
        Kind::Identity => out.push(c),
        Kind::Mark => {}
        Kind::Other => return Some(false),
    }
    Some(true)
}

/// `regex` \d.
pub(super) fn is_digit(c: char) -> bool {
    let t = &TABLE.digits;
    let i = t.partition_point(|r| r.0 <= c as u32);
    i > 0 && t[i - 1].1 >= c as u32
}

/// `regex` \s.
pub(super) fn is_space(c: char) -> bool {
    TABLE.spaces.contains(&c)
}

/// `c` folded where it may be a letter of a timezone name, matched case-insensitively.
pub(super) fn tz_fold(c: char) -> Option<char> {
    if c.is_ascii_alphabetic() {
        return Some(c.to_ascii_lowercase());
    }
    TABLE.tz_fold.get(&c).copied()
}

/// IGNORECASE-equal to `г`.
pub(super) fn is_ghe(c: char) -> bool {
    TABLE.ghe.contains(&c)
}

/// The character sanitize_date writes for `c` (apostrophe and dash look-alikes), if it rewrites it.
pub(super) fn look_alike(c: char) -> Option<char> {
    if TABLE.apostrophes.contains(&c) {
        Some('\'')
    } else if TABLE.dashes.contains(&c) {
        Some('-')
    } else {
        None
    }
}
