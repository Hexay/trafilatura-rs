//! htmldate/extractors.py: URL, image, JSON and regex searches over the tree or its serialization.
use super::extractors::{build_dmy, custom_parse};
use super::regexes::{COMPLETE_URL, DATE_CORE_PATTERN, JSON_MODIFIED, JSON_PUBLISHED, TEXT_PATTERNS};
use super::validators::{convert_date, is_valid_date_str, validate_and_convert};
use super::{caught, Env};
use crate::dom::{Ctx, Node, Result};
use crate::py::datetime::DateTime;
use crate::py::int::py_int;
use crate::py::pyre::{group, Pattern};

/// Strftime directives carrying time of day or time zone.
const TIME_TZ_DIRECTIVES: &[&str] = &["%H", "%I", "%M", "%S", "%f", "%z", "%Z", "%p", "%X", "%c"];
const CORE_WINDOW: usize = 60;
const MAX_CORES: usize = 1000;

/// `discard_unwanted`: archive.org banner inserts.
pub(super) fn discard_unwanted(ctx: &Ctx, tree: Node) -> Result<()> {
    for subtree in tree.xpath_nodes(".//div[@id=\"wm-ipp-base\" or @id=\"wm-ipp\"]")? {
        if let Some(parent) = subtree.parent() {
            parent.remove(ctx, subtree)?;
        }
    }
    Ok(())
}

/// `extract_url_date`.
pub(super) fn extract_url_date(env: &Env, testurl: Option<&str>) -> Result<Option<String>> {
    let Some(m) = testurl.and_then(|u| COMPLETE_URL.search(u)) else { return Ok(None) };
    let int = |i| py_int(group(&m, i).unwrap_or_default());
    let dateobject = caught((|| DateTime::ymd(int(1)?, int(2)?, int(3)?))())?;
    Ok(dateobject.and_then(|d| validate_and_convert(Some(d), env.format, env)))
}

/// `img_search`.
pub(super) fn img_search(env: &Env, tree: Node) -> Result<Option<String>> {
    let element = tree.findall(".//meta[@property=\"og:image\"]").into_iter().find(|e| e.has_attr("content"));
    match element {
        Some(e) => extract_url_date(env, e.get("content").as_deref()),
        None => Ok(None),
    }
}

/// `pattern_search`: group 1 is the date, the optional group 2 the time/tz suffix.
pub(super) fn pattern_search(env: &Env, text: &str, date_pattern: &Pattern) -> Result<Option<String>> {
    let Some(m) = date_pattern.search(text) else { return Ok(None) };
    let date = group(&m, 1).unwrap_or_default();
    if !is_valid_date_str(date, env)? {
        return Ok(None);
    }
    let suffix = group(&m, 2).filter(|s| !s.is_empty());
    if let Some(suffix) = suffix.filter(|_| TIME_TZ_DIRECTIVES.iter().any(|d| env.format.contains(d))) {
        if let Some(full) = custom_parse(env, &format!("{date}{suffix}"), env.format)? {
            return Ok(Some(full));
        }
    }
    convert_date(date, env.format).map(Some)
}

/// `json_search`.
pub(super) fn json_search(env: &Env, tree: Node) -> Result<Option<String>> {
    let json_pattern = if env.original { &JSON_PUBLISHED } else { &JSON_MODIFIED };
    let expr = ".//script[@type=\"application/ld+json\" or @type=\"application/settings+json\"]";
    for elem in tree.xpath_nodes(expr)? {
        let Some(text) = elem.text().filter(|t| t.contains("\"date")) else { continue };
        if let Some(r) = pattern_search(env, &text, json_pattern)? {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

/// Byte offset `n` code points before (`back`) or after `from`, clamped to the string.
fn step_chars(s: &str, from: usize, n: usize, back: bool) -> usize {
    if back {
        s[..from].char_indices().rev().nth(n - 1).map_or(0, |(i, _)| i)
    } else {
        s[from..].char_indices().nth(n).map_or(s.len(), |(i, _)| from + i)
    }
}

/// `idiosyncrasies_search`: probe ±60-char windows around the first 1000 numeric cores, then search unbounded.
pub(super) fn idiosyncrasies_search(env: &Env, htmlstring: &str) -> Result<Option<String>> {
    let mut found = None;
    let mut pos = 0;
    let mut seen = 0;
    while let Some(core) = DATE_CORE_PATTERN.search_from(htmlstring, pos) {
        let core = core.get(0).expect("group 0");
        pos = core.end();
        if seen == MAX_CORES {
            // cap hit on a date-dense document
            found = TEXT_PATTERNS.search(htmlstring);
            break;
        }
        seen += 1;
        let start = step_chars(htmlstring, core.start(), CORE_WINDOW, true);
        let end = step_chars(htmlstring, core.end(), CORE_WINDOW, false);
        if TEXT_PATTERNS.search_from(&htmlstring[..end], start).is_some() {
            found = TEXT_PATTERNS.search_from(htmlstring, start);
            break;
        }
    }
    let Some(m) = found else { return Ok(None) };
    let parts: Vec<&str> = (1..m.len()).filter_map(|i| group(&m, i)).filter(|g| !g.is_empty()).collect();
    if parts.len() < 3 {
        // IndexError, caught
        return Ok(None);
    }
    let int = |i: usize| py_int(parts[i]);
    let candidate = (|| {
        if crate::py::len(parts[0]) == 4 {
            DateTime::ymd(int(0)?, int(1)?, int(2)?)
        } else {
            build_dmy(int(0)?, int(1)?, int(2)?)
        }
    })();
    Ok(caught(candidate)?.and_then(|c| validate_and_convert(Some(c), env.format, env)))
}
