//! Per-locale vocabularies from tables.txt (harness/gen_dateparser_tables.py).
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const SOURCE: &str = include_str!("tables.txt");

pub(super) enum Vocabulary {
    /// The locale splits on word boundaries; its words are the bits `index` of `Tables::words`.
    Words,
    /// no_word_spacing: a word must be a concatenation of these.
    Pieces(HashSet<&'static str>),
}

pub(super) struct Locale {
    /// The letter-variant twin of ar/fa, which reads `Tables::variants` letters as their replacements.
    pub variant: bool,
    pub vocabulary: Vocabulary,
    /// Rewrites that void `vocabulary` apply when a string has one of these letters (None: any string).
    pub guard: Option<Option<HashSet<char>>>,
    /// Then every letter must be one of these (None: nothing can be excluded).
    pub letters: Option<HashSet<char>>,
}

pub(super) struct Tables {
    pub locales: Vec<Locale>,
    /// Words-mode locales accepting each word, as a bitset over `locales`.
    pub words: HashMap<&'static str, Vec<u64>>,
    /// Some locale's words could not be bounded: nothing can be proved.
    pub any: bool,
    pub variants: HashMap<char, char>,
    pub tz: HashSet<&'static str>,
    pub months: HashSet<&'static str>,
}

fn letters(fields: &[&str]) -> Option<HashSet<char>> {
    (!fields.contains(&"any") && !fields.contains(&"*")).then(|| fields.iter().flat_map(|f| f.chars()).collect())
}

fn hex_char(field: &str) -> char {
    u32::from_str_radix(field, 16).ok().and_then(char::from_u32).expect("a code point in tables.txt")
}

pub(super) static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut t = Tables {
        locales: Vec::new(),
        words: HashMap::new(),
        any: false,
        variants: HashMap::new(),
        tz: HashSet::new(),
        months: HashSet::new(),
    };
    let mut word_lists = Vec::new();
    for line in SOURCE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&'static str> = line.split(' ').collect();
        match f[0] {
            "L" | "X" => {
                let vocabulary = match f[2] {
                    "words" => Vocabulary::Words,
                    "pieces" => Vocabulary::Pieces(f[3..].iter().copied().collect()),
                    _ => {
                        t.any = true;
                        Vocabulary::Words
                    }
                };
                word_lists.push(if f[2] == "words" { f[3..].to_vec() } else { Vec::new() });
                t.locales.push(Locale { variant: f[0] == "X", vocabulary, guard: None, letters: None });
            }
            "G" => t.locales.last_mut().expect("a locale before G").guard = Some(letters(&f[1..])),
            "C" => t.locales.last_mut().expect("a locale before C").letters = letters(&f[1..]),
            "Y" => t.variants.extend(f[1..].chunks(2).map(|p| (hex_char(p[0]), hex_char(p[1])))),
            "T" => t.tz.extend(&f[1..]),
            "M" => t.months.extend(&f[1..]),
            _ => panic!("malformed dateparser table line {line:?}"),
        }
    }
    let blocks = t.locales.len().div_ceil(64);
    for (i, words) in word_lists.iter().enumerate() {
        for w in words {
            t.words.entry(w).or_insert_with(|| vec![0; blocks])[i / 64] |= 1 << (i % 64);
        }
    }
    t
});
