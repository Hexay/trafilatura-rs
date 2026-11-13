//! Locale data and patterns from locales.txt (harness/gen_dateparser_locales.py).
use super::quick;
use super::unicode::fold;
use crate::py::pyre::Pattern;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, OnceLock};

const SOURCE: &str = include_str!("locales.txt");

/// A pattern compiled on first use, its `@<n>@` class references expanded.
pub(super) struct Rx {
    compact: &'static str,
    compiled: OnceLock<Pattern>,
    quick: OnceLock<Option<regex::Regex>>,
}

impl Rx {
    fn source(&self) -> String {
        let mut src = String::with_capacity(self.compact.len());
        let mut parts = self.compact.split('@');
        src.push_str(parts.next().unwrap_or(""));
        while let (Some(n), Some(rest)) = (parts.next(), parts.next()) {
            src.push_str(DATA.classes[n.parse::<usize>().expect("a class number")]);
            src.push_str(rest);
        }
        src
    }

    pub fn get(&self) -> &Pattern {
        self.compiled.get_or_init(|| Pattern::new(Box::leak(self.source().into_boxed_str())))
    }

    /// False when the pattern certainly finds no match in `s` (quick.rs).
    pub fn may_match(&self, s: &str) -> bool {
        self.quick.get_or_init(|| quick::compile(&self.source())).as_ref().is_none_or(|q| q.is_match(s))
    }
}

#[derive(Default)]
pub(super) struct Trie {
    next: Vec<HashMap<char, u32>>,
    end: Vec<bool>,
}

impl Trie {
    fn insert(&mut self, word: &str) {
        let mut node = 0;
        for c in word.chars().map(fold) {
            node = match self.next[node].get(&c) {
                Some(&n) => n as usize,
                None => {
                    self.next.push(HashMap::new());
                    self.end.push(false);
                    let n = self.next.len() - 1;
                    self.next[node].insert(c, n as u32);
                    n
                }
            };
        }
        self.end[node] = true;
    }

    /// Char counts of the words matching `chars` from its start, case-insensitively, longest first.
    pub fn prefixes(&self, chars: &[char]) -> Vec<usize> {
        let (mut node, mut out) = (0, Vec::new());
        for (i, &c) in chars.iter().enumerate() {
            let Some(&n) = self.next[node].get(&fold(c)) else { break };
            node = n as usize;
            if self.end[node] {
                out.push(i + 1);
            }
        }
        out.reverse();
        out
    }
}

#[derive(Default)]
pub(super) struct Dict {
    pub entries: HashMap<&'static str, Option<&'static str>>,
    pub meanings: HashMap<&'static str, Vec<&'static str>>,
    pub conflicts: HashMap<&'static str, &'static str>,
    pub months: HashSet<&'static str>,
    keys: Vec<&'static str>,
    trie: OnceLock<Trie>,
}

impl Dict {
    pub fn trie(&self) -> &Trie {
        self.trie.get_or_init(|| {
            let mut t = Trie { next: vec![HashMap::new()], end: vec![false] };
            self.keys.iter().for_each(|k| t.insert(k));
            t
        })
    }

    /// `Dictionary._is_unambiguous_month`.
    pub fn unambiguous_month(&self, token: &str) -> bool {
        self.months.contains(token) && !self.conflicts.contains_key(token)
    }
}

pub(super) struct Locale {
    pub name: &'static str,
    pub date_order: Option<&'static str>,
    pub no_word_spacing: bool,
    pub letter_variants: bool,
    /// None: the locale could not be ported.
    pub dict: Option<usize>,
    pub simplifications: Vec<(usize, &'static str)>,
    pub split_relative: usize,
    pub match_relative: usize,
    /// (pattern, replacement, merged groups)
    pub relative: Vec<(usize, &'static str, bool)>,
}

pub(super) struct Zone {
    pub offset: i32,
    pub alone: usize,
}

pub(super) struct Family {
    pub pattern: usize,
    pub reverse: bool,
    pub zones: Vec<Zone>,
    /// the zone names as written in the pattern, folded case-insensitively, and the first zone of each
    pub names: HashMap<String, usize>,
}

pub(super) struct Data {
    pub versions: (&'static str, &'static str),
    classes: Vec<&'static str>,
    pub patterns: Vec<Rx>,
    pub dicts: Vec<Dict>,
    pub locales: Vec<Locale>,
    /// (locale, the locale whose applicability decides), in loader order
    pub order: Vec<(usize, usize)>,
    pub families: Vec<Family>,
    pub globals: HashMap<&'static str, usize>,
    pub variants: HashMap<char, char>,
}

fn num(field: &str) -> usize {
    field.parse().unwrap_or_else(|_| panic!("bad number {field:?} in locales.txt"))
}

pub(super) static DATA: LazyLock<Data> = LazyLock::new(|| {
    let mut d = Data {
        versions: ("", ""),
        classes: Vec::new(),
        patterns: Vec::new(),
        dicts: Vec::new(),
        locales: Vec::new(),
        order: Vec::new(),
        families: Vec::new(),
        globals: HashMap::new(),
        variants: HashMap::new(),
    };
    for line in SOURCE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&'static str> = line.split('\t').collect();
        fn dict(dicts: &mut [Dict]) -> &mut Dict {
            dicts.last_mut().expect("a D line first")
        }
        match f[0] {
            "V" => d.versions = (f[2], f[3]),
            "K" => d.classes.push(f[2]),
            "P" => d.patterns.push(Rx { compact: f[2], compiled: OnceLock::new(), quick: OnceLock::new() }),
            "D" => d.dicts.push(Dict::default()),
            "W" => {
                let dict = dict(&mut d.dicts);
                dict.keys.push(f[1]);
                dict.entries.entry(f[1]).or_insert(f.get(2).copied());
            }
            "A" => _ = dict(&mut d.dicts).meanings.insert(f[1], f[2..].to_vec()),
            "C" => _ = dict(&mut d.dicts).conflicts.insert(f[1], f[2]),
            "M" => dict(&mut d.dicts).months.extend(f[1..].iter().filter(|w| !w.is_empty())),
            "L" => d.locales.push(Locale {
                name: f[3],
                date_order: (f[4] != "-").then_some(f[4]),
                no_word_spacing: f[5] == "1",
                letter_variants: f[6] == "1",
                dict: (f[7] != "-").then(|| num(f[7])),
                simplifications: Vec::new(),
                split_relative: 0,
                match_relative: 0,
                relative: Vec::new(),
            }),
            "U" => d.locales.last_mut().expect("a locale").dict = None,
            "S" => d.locales.last_mut().expect("a locale").simplifications.push((num(f[1]), f[2])),
            "R" => d.locales.last_mut().expect("a locale").split_relative = num(f[1]),
            "Q" => d.locales.last_mut().expect("a locale").match_relative = num(f[1]),
            "T" => d.locales.last_mut().expect("a locale").relative.push((num(f[1]), f[2], f[3] == "1")),
            "O" => d.order.push((num(f[1]), num(f[2]))),
            "Z" => d.families.push(Family {
                pattern: num(f[1]),
                reverse: f[2] == "1",
                zones: Vec::new(),
                names: HashMap::new(),
            }),
            "z" => {
                let family = d.families.last_mut().expect("a family");
                let first = family.zones.len();
                family.names.entry(f[1].chars().map(fold).collect()).or_insert(first);
                family.zones.push(Zone { offset: f[2].parse().expect("an offset"), alone: num(f[3]) });
            }
            "G" => _ = d.globals.insert(f[1], num(f[2])),
            "Y" => {
                _ = d.variants.insert(f[1].chars().next().expect("a letter"), f[2].chars().next().expect("a letter"))
            }
            _ => panic!("malformed locales.txt line {line:?}"),
        }
    }
    d
});

pub(super) fn pattern(n: usize) -> &'static Pattern {
    DATA.patterns[n].get()
}

/// `Rx::may_match` of pattern `n`.
pub(super) fn may_match(n: usize, s: &str) -> bool {
    DATA.patterns[n].may_match(s)
}

pub(super) fn global(name: &str) -> &'static Pattern {
    pattern(DATA.globals[name])
}
