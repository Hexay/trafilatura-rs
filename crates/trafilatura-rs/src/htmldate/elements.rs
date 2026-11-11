//! htmldate/core.py: date-bearing elements (candidates by attributes, `abbr`, `time`).
use super::extractors::{try_date_expr, try_date_expr_opts};
use super::regexes::{CLASS_CUES, ID_CLASS_CUES, NON_DIGITS_REGEX};
use super::validators::{check_extracted_reference, compare_values, update_reference, REFERENCE_FORMAT};
use super::{caught, cslice, has_plausible_candidates, Env, MAX_SEGMENT_LEN, MIN_SEGMENT_LEN};
use crate::dom::{Node, Result};
use crate::py::datetime::DateTime;
use crate::py::int::py_int;

const FAST_TAGS: &[&str] = &["div", "h2", "h3", "h4", "li", "p", "span", "time", "ul", "footer", "small"];
const ALWAYS_TAGS: &[&str] = &["footer", "small"];
const CLASS_ATTRS: &[&str] = &["date-published", "published", "time published"];

/// `examine_text` (`text` already passed through `trim_text`).
fn examine_text(env: &Env, text: &str) -> Result<Option<String>> {
    if crate::py::len(text) <= MIN_SEGMENT_LEN {
        return Ok(None);
    }
    let text = NON_DIGITS_REGEX.sub(cslice(text, 0, MAX_SEGMENT_LEN), "");
    try_date_expr_opts(env, Some(&text))
}

/// `is_date_candidate`: of id and class, only the first in source order counts.
fn is_date_candidate(elem: Node) -> bool {
    let (mut first, mut itemprop) = (None, None);
    for (key, value) in elem.attrs() {
        if key == "itemprop" {
            itemprop = Some(value);
        } else if first.is_none() && (key == "id" || key == "class") {
            first = Some(value);
        }
    }
    for cue in [&first, &itemprop].into_iter().flatten() {
        let folded = cue.replace('D', "d");
        if folded.contains("date") || folded.contains("datum") {
            return true;
        }
    }
    let Some(first) = first else { return false };
    if ID_CLASS_CUES.is_match(&first) {
        return true;
    }
    if elem.get("class").is_some_and(|cls| CLASS_CUES.is_match(&cls)) {
        return true;
    }
    elem.get("id").is_some_and(|id| id.contains("footer-info-lastmod"))
}

/// `date_candidates`.
pub(super) fn date_candidates(tree: Node, extensive_search: bool) -> Vec<Node> {
    let tags: &[&str] = if extensive_search { &["*"] } else { FAST_TAGS };
    tree.iter_descendants(tags).filter(|e| ALWAYS_TAGS.contains(&e.tag()) || is_date_candidate(*e)).collect()
}

/// `examine_elements`: element text, then its title (Blogspot).
pub(super) fn examine_elements(env: &Env, elements: &[Node]) -> Result<Option<String>> {
    if !has_plausible_candidates(elements.len()) {
        return Ok(None);
    }
    for elem in elements {
        let title = crate::py::Collapse::of(&elem.get("title").unwrap_or_default());
        for text in [elem.text_content_trimmed(), title] {
            if let Some(attempt) = examine_text(env, &text)? {
                return Ok(Some(attempt));
            }
        }
    }
    Ok(None)
}

/// `compare_reference`.
pub(super) fn compare_reference(env: &Env, reference: Option<DateTime>, expression: &str) -> Result<Option<DateTime>> {
    match try_date_expr(env, Some(expression), REFERENCE_FORMAT)? {
        Some(attempt) => compare_values(reference, &attempt, env),
        None => Ok(reference),
    }
}

/// `examine_abbr_elements`.
pub(super) fn examine_abbr_elements(env: &Env, tree: Node) -> Result<Option<String>> {
    let elements = tree.findall(".//abbr");
    if !has_plausible_candidates(elements.len()) {
        return Ok(None);
    }
    let mut reference = None;
    for &elem in &elements {
        if elem.has_attr("data-utime") {
            // untrusted: may be outside the platform timestamp range
            let utime = caught(py_int(&elem.get("data-utime").unwrap_or_default()))?;
            let Some(candidate) = utime.and_then(DateTime::fromtimestamp_utc) else { continue };
            reference = Some(update_reference(reference, candidate, env.original));
        } else if elem.get("class").is_some_and(|c| CLASS_ATTRS.contains(&c.as_str())) {
            if let Some(trytext) = elem.get("title") {
                if env.original {
                    if let Some(attempt) = try_date_expr_opts(env, Some(&trytext))? {
                        return Ok(Some(attempt));
                    }
                } else {
                    reference = compare_reference(env, reference, &trytext)?;
                    if reference.is_some() {
                        break;
                    }
                }
            } else if let Some(text) = elem.text().filter(|t| crate::py::len(t) > 10) {
                reference = compare_reference(env, reference, &text)?;
            }
        }
    }
    if let Some(r) = check_extracted_reference(reference, env) {
        return Ok(Some(r));
    }
    examine_elements(env, &elements)
}

/// `examine_time_elements`.
pub(super) fn examine_time_elements(env: &Env, tree: Node) -> Result<Option<String>> {
    let elements = tree.findall(".//time");
    if !has_plausible_candidates(elements.len()) {
        return Ok(None);
    }
    let mut reference = None;
    for elem in elements {
        let datetime_attr = elem.get("datetime").unwrap_or_default();
        if crate::py::len(&datetime_attr) > 6 {
            let class_attr = elem.get("class").unwrap_or_default();
            let shortcut_flag = if env.original {
                elem.get("pubdate").as_deref() == Some("pubdate")
                    || class_attr.starts_with("entry-date")
                    || class_attr.starts_with("entry-time")
            } else {
                class_attr == "updated"
            };
            if shortcut_flag {
                if let Some(attempt) = try_date_expr_opts(env, Some(&datetime_attr))? {
                    return Ok(Some(attempt));
                }
            } else {
                reference = compare_reference(env, reference, &datetime_attr)?;
            }
        } else if let Some(text) = elem.text().filter(|t| crate::py::len(t) > 6) {
            reference = compare_reference(env, reference, &text)?;
        }
    }
    Ok(check_extracted_reference(reference, env))
}
