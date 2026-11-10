//! Parsing of one location step into a `Step` (grammar in pred.rs). Splitting is quote- and bracket-aware:
//! trafilatura 2.x regex arguments contain `[`, `]`, `(`, `)` and `|`.
use super::pred::{AttrTests, Step, UnionKind, UnionTest};
use aho_corasick::AhoCorasick;
use regex::Regex;
use std::sync::LazyLock;

const ATTRS: &str = r"@[\w-]+(?:\s*\|\s*@[\w-]+)*";

static STR_FN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"^(contains|starts-with)\(\s*({ATTRS})\s*,\s*(["'])(.*?)["']\s*\)$"#)).unwrap()
});
static TRANSLATE_FN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(contains|starts-with)\(\s*translate\(\s*@([\w-]+)\s*,\s*["']([A-Za-z]+)["']\s*,\s*["']([a-z]+)["']\s*\)\s*,\s*["']([^"']*)["']\s*\)$"#).unwrap()
});
static RE_TEST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"^re:test\(\s*({ATTRS})\s*,\s*'([^']*)'\s*(?:,\s*'([^']*)'\s*)?\)$"#)).unwrap()
});
static EQUALS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^@([\w-]+)\s*=\s*(["'])(.*?)["']$"#).unwrap());
static PRESENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^@([\w-]+)$").unwrap());

/// Splits `s` at top-level (outside quotes, parentheses and brackets) occurrences of `sep`.
pub(crate) fn split_top<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let (mut depth, mut quote, mut start, mut parts) = (0i32, None, 0, Vec::new());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'"' || c == b'\'' => quote = Some(c),
            None if c == b'(' || c == b'[' => depth += 1,
            None if c == b')' || c == b']' => depth -= 1,
            None if depth == 0 && s[i..].starts_with(sep) => {
                parts.push(&s[start..i]);
                i += sep.len();
                start = i;
                continue;
            }
            None => {}
        }
        i += 1;
    }
    parts.push(&s[start..]);
    parts
}

/// `head[g1][g2]...` -> (head, group bodies); None when the brackets don't close cleanly.
fn bracket_groups(s: &str) -> Option<(&str, Vec<&str>)> {
    let first = s.find('[').unwrap_or(s.len());
    let (head, mut rest) = s.split_at(first);
    let mut groups = Vec::new();
    while !rest.is_empty() {
        let (depth0, mut depth, mut quote, mut end) = (rest.strip_prefix('[')?, 0i32, None, None);
        for (i, c) in depth0.char_indices() {
            match (quote, c) {
                (Some(q), _) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '[' | '(') => depth += 1,
                (None, ')') => depth -= 1,
                (None, ']') if depth == 0 => {
                    end = Some(i);
                    break;
                }
                (None, ']') => depth -= 1,
                _ => {}
            }
        }
        let end = end?;
        groups.push(depth0[..end].trim());
        rest = depth0[end + 1..].trim_start();
    }
    Some((head, groups))
}

fn tag_group(g: &str) -> Option<Vec<String>> {
    let inner = g.strip_prefix('(').and_then(|x| x.strip_suffix(')')).unwrap_or(g);
    split_top(inner, " or ")
        .into_iter()
        .map(|t| t.trim().strip_prefix("self::").filter(|n| super::is_name(n)).map(str::to_owned))
        .collect()
}

fn attr_names(list: &str) -> Vec<Vec<u8>> {
    list.split('|').map(|a| a.trim().trim_start_matches('@').as_bytes().to_vec()).collect()
}

pub(crate) fn parse_step(b: &str) -> Option<Step> {
    let (head, groups) = bracket_groups(b.strip_prefix(".//")?)?;
    let mut tags = match head {
        "*" => None,
        name if super::is_name(name) => Some(vec![name.to_owned()]),
        _ => return None,
    };
    let mut groups = groups.as_slice();
    if tags.is_none() {
        if let Some(t) = groups.first().and_then(|g| tag_group(g)) {
            tags = Some(t);
            groups = &groups[1..];
        }
    }
    match groups {
        [] => Some(Step::new(tags, Vec::new(), Vec::new(), true)),
        [preds] => parse_preds(preds).map(|(tests, unions)| Step::new(tags, tests, unions, false)),
        _ => None,
    }
}

type Group = (Vec<u8>, Option<String>, Vec<String>, Vec<Vec<u8>>, Vec<Vec<u8>>, bool);

fn group<'g>(groups: &'g mut Vec<Group>, attr: &[u8], map_key: Option<String>) -> &'g mut Group {
    let i = match groups.iter().position(|g| g.0 == attr && g.1 == map_key) {
        Some(i) => i,
        None => {
            groups.push((attr.to_vec(), map_key, vec![], vec![], vec![], false));
            groups.len() - 1
        }
    };
    &mut groups[i]
}

fn parse_preds(body: &str) -> Option<(Vec<AttrTests>, Vec<UnionTest>)> {
    let mut groups: Vec<Group> = Vec::new();
    let mut unions = Vec::new();
    for atom in split_top(body, " or ").into_iter().map(str::trim) {
        if let Some(c) = RE_TEST.captures(atom) {
            let ignorecase = c.get(3).is_some_and(|f| f.as_str().contains('i'));
            let regex = crate::py::re::compile(&c[2], ignorecase);
            unions.push(UnionTest { names: attr_names(&c[1]), kind: UnionKind::Regex(regex) });
        } else if let Some(c) = STR_FN.captures(atom) {
            let names = attr_names(&c[2]);
            let needle = c[4].as_bytes().to_vec();
            let starts = &c[1] == "starts-with";
            if names.len() > 1 {
                let kind = if starts { UnionKind::StartsWith(needle) } else { UnionKind::Contains(needle) };
                unions.push(UnionTest { names, kind });
            } else if starts {
                group(&mut groups, &names[0], None).3.push(needle);
            } else {
                group(&mut groups, &names[0], None).2.push(c[4].to_owned());
            }
        } else if let Some(c) = TRANSLATE_FN.captures(atom) {
            let g = group(&mut groups, c[2].as_bytes(), Some(format!("{}\0{}", &c[3], &c[4])));
            if &c[1] == "starts-with" {
                g.3.push(c[5].as_bytes().to_vec());
            } else {
                g.2.push(c[5].to_owned());
            }
        } else if let Some(c) = EQUALS.captures(atom) {
            group(&mut groups, c[1].as_bytes(), None).4.push(c[3].as_bytes().to_vec());
        } else {
            let c = PRESENT.captures(atom)?;
            group(&mut groups, c[1].as_bytes(), None).5 = true;
        }
    }
    let tests = groups
        .into_iter()
        .map(|(name, map_key, contains, starts, equals, present)| {
            let map = map_key.map(|k| {
                let (from, to) = k.split_once('\0').expect("key");
                let mut m: [u8; 256] = std::array::from_fn(|i| i as u8);
                // XPath translate: extra source chars (no target) are deleted; our expressions never have any.
                assert_eq!(from.len(), to.len(), "translate with deletions is not supported");
                for (f, t) in from.bytes().zip(to.bytes()) {
                    m[f as usize] = t;
                }
                m
            });
            let contains = (!contains.is_empty()).then(|| AhoCorasick::new(&contains).expect("patterns"));
            AttrTests { name, map, contains, starts, equals, present }
        })
        .collect();
    Some((tests, unions))
}
