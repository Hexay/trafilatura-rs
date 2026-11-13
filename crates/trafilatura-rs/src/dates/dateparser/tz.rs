//! `dateparser.timezone_parser.pop_tz_offset_from_string`, without TIMEZONE_ABBREVIATIONS overrides. There are two
//! families (harness/gen_dateparser_locales.py checks their templates): `(.)<offsets>$`, REVERSE, whose alternatives
//! all have a sign and a digit, and `(\W|\d|_)<names>($|\W|(?=\d))` over names of letters only.
use super::locales::{may_match, pattern, Family, DATA};
use super::unicode::{fold, is, Class};

const MAX_EXTRA_TZ: usize = 2;

/// A REVERSE family finds the match of its first alternative that matches anywhere.
fn reverse_match(family: &Family, s: &str) -> Option<(usize, (usize, usize))> {
    let b = s.as_bytes();
    if !b.windows(2).any(|w| matches!(w[0], b'+' | b'-') && w[1].is_ascii_digit()) {
        return None;
    }
    if !may_match(family.pattern, s) {
        return None;
    }
    pattern(family.pattern).search(s)?;
    family.zones.iter().enumerate().find_map(|(k, zone)| {
        let m = may_match(zone.alone, s).then(|| pattern(zone.alone).search(s))??;
        let m = m.get(0).expect("group 0");
        Some((k, (m.start(), m.end())))
    })
}

fn letter(c: char) -> bool {
    is(Class::Word, c) && !is(Class::Digit, c) && c != '_'
}

/// `finditer(overlapped=True)` of the names family: the earliest timezone, ties to the leftmost match. A name matches
/// only as a whole run of letters after some character, and not before `_`.
fn names_match(family: &Family, s: &str) -> Option<(usize, (usize, usize))> {
    if s.contains('\n') {
        // `$` also matches before a final newline
        return overlapped_match(family, s);
    }
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut best: Option<(usize, (usize, usize))> = None;
    let mut i = 1;
    while i < chars.len() {
        if !letter(chars[i].1) || letter(chars[i - 1].1) {
            i += 1;
            continue;
        }
        let end = i + chars[i..].iter().take_while(|&&(_, c)| letter(c)).count();
        let folded: String = chars[i..end].iter().map(|&(_, c)| fold(c)).collect();
        let stop = match chars.get(end) {
            None => Some(s.len()),
            Some(&(_, c)) if is(Class::Digit, c) => Some(chars[end].0),
            Some(&(at, c)) if !is(Class::Word, c) => Some(at + c.len_utf8()),
            Some(_) => None,
        };
        if let (Some(&k), Some(stop)) = (family.names.get(&folded), stop) {
            if best.is_none_or(|(b, _)| k < b) {
                best = Some((k, (chars[i - 1].0, stop)));
            }
        }
        i = end;
    }
    best
}

fn overlapped_match(family: &Family, s: &str) -> Option<(usize, (usize, usize))> {
    let p = pattern(family.pattern);
    let mut best: Option<(usize, (usize, usize))> = None;
    let mut pos = 0;
    while pos <= s.len() {
        let Some(caps) = p.search_from(s, pos) else { break };
        let m = caps.get(0).expect("group 0");
        let k = (2..caps.len() - 1).find(|&g| caps.get(g).is_some()).expect("a timezone group") - 2;
        if best.is_none_or(|(b, _)| k < b) {
            best = Some((k, (m.start(), m.end())));
        }
        pos = m.start() + s[m.start()..].chars().next().map_or(1, char::len_utf8);
    }
    best
}

/// `_find_tz`: ((start, stop) in bytes, offset seconds).
fn find_tz(s: &str) -> Option<((usize, usize), i32)> {
    DATA.families.iter().find_map(|family| {
        let (k, span) = if family.reverse { reverse_match(family, s) } else { names_match(family, s) }?;
        Some((span, family.zones[k].offset))
    })
}

/// `_search_and_pop_tz`: the string without the timezone (the character before it stays), and its offset.
fn search_and_pop(s: &str) -> Option<(String, i32)> {
    let ((start, stop), offset) = find_tz(s)?;
    let keep = start + s[start..].chars().next().map_or(0, char::len_utf8);
    let glued = stop < s.len() && s[..stop].chars().next_back().is_some_and(|c| is(Class::Alpha, c));
    Some((format!("{}{}{}", &s[..keep], if glued { " " } else { "" }, &s[stop..]), offset))
}

/// `pop_tz_offset_from_string(s)`: the string without its timezone tokens, and the timezone's offset.
pub(super) fn pop_tz(s: &str) -> (String, Option<i32>) {
    let Some((mut out, offset)) = search_and_pop(s) else { return (s.to_owned(), None) };
    for _ in 0..MAX_EXTRA_TZ {
        let trimmed = out.trim_end_matches(|c| is(Class::PySpace, c)).to_owned();
        match search_and_pop(&trimmed) {
            Some((rest, o)) if o == offset => out = rest,
            _ => break,
        }
    }
    (out, Some(offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_hold_for_the_data() {
        for family in &DATA.families {
            for name in family.names.keys() {
                let unescaped = name.replace('\\', "");
                let b = unescaped.as_bytes();
                let ok = if family.reverse {
                    b.windows(2).any(|w| matches!(w[0], b'+' | b'-') && w[1].is_ascii_digit())
                } else {
                    name.chars().all(letter)
                };
                assert!(ok, "{name}");
            }
        }
    }

    #[test]
    fn names_match_the_pattern() {
        let names = DATA.families.iter().find(|f| !f.reverse).expect("a names family");
        for s in ["12 May 2021 10:00 CEST", "x est.", "1 EST_", "EST 2", "a(PST)b", "ſt cest2 ist", "UTCx gmt"] {
            assert_eq!(names_match(names, s), overlapped_match(names, s), "{s}");
        }
        assert_eq!(pop_tz("12 May 2021 10:00 CEST").1, Some(7200));
        assert_eq!(pop_tz("1 Jan 2020 GMT+2").1, Some(7200));
    }
}
