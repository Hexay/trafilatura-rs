//! htmldate/core.py `search_page` and its Counter-based candidate selection.
use super::extractors::regex_parse;
use super::regexes::{
    COPYRIGHT_PATTERN, DATESTRINGS_CATCH, DATESTRINGS_PATTERN, MMYYYY_PATTERN, SELECT_YMD_PATTERN, SELECT_YMD_YEAR,
    SIMPLE_PATTERN, SLASHES_PATTERN, SLASHES_YEAR, THREE_COMP_REGEX_A, THREE_COMP_REGEX_B, THREE_LOOSE_CATCH,
    THREE_LOOSE_PATTERN, THREE_PATTERN, TWO_COMP_REGEX, YEAR_PATTERN, YMD_PATTERN, YYYYMM_CATCH, YYYYMM_PATTERN,
};
use super::validators::{
    correct_year, filter_ymd_candidate, is_valid_datetime, plausible_year_filter, validate_and_convert, Counter,
};
use super::{has_plausible_candidates, Env};
use crate::dom::{Error, Result};
use crate::py::datetime::DateTime;
use crate::py::int::py_int;
use crate::py::pyre::{group, Pattern};
use std::collections::HashMap;

/// A match as `m[0], m[1], ...` (None for groups that did not participate).
type Groups = Vec<Option<String>>;

fn search_groups(pattern: &Pattern, s: &str) -> Option<Groups> {
    pattern.search(s).map(|m| (0..m.len()).map(|i| group(&m, i).map(str::to_owned)).collect())
}

fn index_error<T>() -> Result<T> {
    Err(Error::Runtime("IndexError: list index out of range".into()))
}

/// `select_candidate`: among the 10 most frequent, the first two in sort order (newest first unless original).
fn select_candidate(occurrences: &Counter, catch: &Pattern, yearpat: &Pattern, env: &Env) -> Result<Option<Groups>> {
    if !has_plausible_candidates(occurrences.len()) {
        return Ok(None);
    }
    if occurrences.len() == 1 {
        return Ok(search_groups(catch, &occurrences[0].0));
    }
    let mut firstselect: Vec<&(String, usize)> = occurrences.iter().collect();
    firstselect.sort_by_key(|e| std::cmp::Reverse(e.1));
    firstselect.truncate(10);
    firstselect.sort();
    if !env.original {
        firstselect.reverse();
    }
    let (patterns, counts) = ([&firstselect[0].0, &firstselect[1].0], [firstselect[0].1, firstselect[1].1]);
    let years: Vec<String> =
        patterns.iter().filter_map(|p| yearpat.search(p).and_then(|m| group(&m, 1).map(str::to_owned))).collect();
    let (min_year, max_year) = (i128::from(env.min.year), i128::from(env.max.year));
    let validation =
        years.iter().map(|y| py_int(y).map(|y| min_year <= y && y <= max_year)).collect::<Result<Vec<bool>>>()?;
    if validation.iter().all(|&v| v) {
        // the runner-up wins when from another year and more than half as frequent, except on ties
        let runner_up = counts[0] != counts[1] && {
            if years.len() < 2 {
                return index_error();
            }
            years[1] != years[0] && 2 * counts[1] > counts[0]
        };
        Ok(search_groups(catch, patterns[usize::from(runner_up)]))
    } else if let Some(i) = validation.iter().position(|&v| v) {
        Ok(search_groups(catch, patterns[i]))
    } else if years.len() < 2 {
        // LOGGER.debug's arguments are evaluated
        index_error()
    } else {
        Ok(None)
    }
}

/// `search_pattern`.
fn search_pattern(
    html: &str,
    pattern: &Pattern,
    catch: &Pattern,
    yearpat: &Pattern,
    env: &Env,
) -> Result<Option<Groups>> {
    let candidates = plausible_year_filter(html, pattern, yearpat, env)?;
    select_candidate(&candidates, catch, yearpat, env)
}

fn zfill2(s: &str) -> String {
    format!("{}{s}", "0".repeat(2usize.saturating_sub(crate::py::len(s))))
}

/// `normalize_match`: zero-pad the components and expand a 2-digit year.
fn normalize_match(pattern: &Pattern, item: &str) -> Result<String> {
    let m = pattern.match_(item).ok_or_else(|| Error::Runtime("AttributeError: 'NoneType' object".into()))?;
    let parts: Vec<String> = (1..m.len()).filter_map(|i| group(&m, i)).filter(|g| !g.is_empty()).map(zfill2).collect();
    let [day, month, year] = <[String; 3]>::try_from(parts).map_err(|_| Error::Value("unpack".into()))?;
    let year = if crate::py::len(&year) == 2 { correct_year(py_int(&year)?).to_string() } else { year };
    Ok(format!("{year}-{month}-{day}"))
}

/// `normalize_two_comp`: MM-YYYY into YYYY-MM-01.
fn normalize_two_comp(item: &str) -> Result<String> {
    let m = TWO_COMP_REGEX.match_(item).ok_or_else(|| Error::Type("'NoneType' object is not subscriptable".into()))?;
    Ok(format!("{}-{}-01", group(&m, 2).unwrap_or_default(), zfill2(group(&m, 1).unwrap_or_default())))
}

/// `_filter_ymd`.
fn filter_ymd(bestmatch: Option<Groups>, copyear: i128, env: &Env) -> Result<Option<String>> {
    filter_ymd_candidate(bestmatch.as_ref().map(|g| &g[1..]), copyear, env)
}

/// `search_normalized`.
fn search_normalized(
    html: &str,
    pattern: &Pattern,
    yearpat: &Pattern,
    normalizer: &dyn Fn(&str) -> Result<String>,
    copyear: i128,
    env: &Env,
) -> Result<Option<String>> {
    let candidates = plausible_year_filter(html, pattern, yearpat, env)?;
    let mut normalized: Counter = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (item, count) in candidates {
        let key = normalizer(&item)?;
        match index.get(&key) {
            Some(&i) => normalized[i].1 += count,
            None => {
                index.insert(key.clone(), normalized.len());
                normalized.push((key, count));
            }
        }
    }
    let bestmatch = select_candidate(&normalized, &YMD_PATTERN, &YEAR_PATTERN, env)?;
    filter_ymd(bestmatch, copyear, env)
}

/// `_search_and_filter`.
fn search_and_filter(
    html: &str,
    pattern: &Pattern,
    catch: &Pattern,
    copyear: i128,
    env: &Env,
) -> Result<Option<String>> {
    filter_ymd(search_pattern(html, pattern, catch, &YEAR_PATTERN, env)?, copyear, env)
}

/// `_finalize_candidate`: the copyright-year floor, then validation.
fn finalize_candidate(dateobject: Option<DateTime>, copyear: i128, env: &Env) -> Option<String> {
    dateobject
        .filter(|d| copyear == 0 || i128::from(d.year) >= copyear)
        .and_then(|d| validate_and_convert(Some(d), env.format, env))
}

fn int_group(g: &Groups, i: usize) -> Result<i128> {
    py_int(g.get(i).and_then(|g| g.as_deref()).unwrap_or_default())
}

/// `search_page`: copyright year, then 3-, 2- and 1-component patterns over the serialized page.
pub(super) fn search_page(env: &Env, html: &str) -> Result<Option<String>> {
    let mut copyear = 0;
    if let Some(bestmatch) = search_pattern(html, &COPYRIGHT_PATTERN, &YEAR_PATTERN, &YEAR_PATTERN, env)? {
        let year = int_group(&bestmatch, 1)?;
        if is_valid_datetime(&DateTime::ymd(year, 1, 1)?, env) {
            copyear = year;
        }
    }
    for pattern in [&THREE_PATTERN, &THREE_LOOSE_PATTERN] {
        if let Some(r) = search_and_filter(html, pattern, &THREE_LOOSE_CATCH, copyear, env)? {
            return Ok(Some(r));
        }
    }
    let three_a = |item: &str| normalize_match(&THREE_COMP_REGEX_A, item);
    if let Some(r) = search_normalized(html, &SELECT_YMD_PATTERN, &SELECT_YMD_YEAR, &three_a, copyear, env)? {
        return Ok(Some(r));
    }
    if let Some(r) = search_and_filter(html, &DATESTRINGS_PATTERN, &DATESTRINGS_CATCH, copyear, env)? {
        return Ok(Some(r));
    }
    let three_b = |item: &str| normalize_match(&THREE_COMP_REGEX_B, item);
    if let Some(r) = search_normalized(html, &SLASHES_PATTERN, &SLASHES_YEAR, &three_b, copyear, env)? {
        return Ok(Some(r));
    }
    if let Some(bestmatch) = search_pattern(html, &YYYYMM_PATTERN, &YYYYMM_CATCH, &YEAR_PATTERN, env)? {
        let dateobject = DateTime::ymd(int_group(&bestmatch, 1)?, int_group(&bestmatch, 2)?, 1)?;
        if let Some(r) = finalize_candidate(Some(dateobject), copyear, env) {
            return Ok(Some(r));
        }
    }
    if let Some(r) = search_normalized(html, &MMYYYY_PATTERN, &SELECT_YMD_YEAR, &normalize_two_comp, copyear, env)? {
        return Ok(Some(r));
    }
    if let Some(r) = finalize_candidate(regex_parse(html)?, copyear, env) {
        return Ok(Some(r));
    }
    if copyear != 0 {
        return Ok(Some(DateTime::ymd(copyear, 1, 1)?.strftime(env.format)));
    }
    if let Some(bestmatch) = search_pattern(html, &SIMPLE_PATTERN, &YEAR_PATTERN, &YEAR_PATTERN, env)? {
        return Ok(finalize_candidate(Some(DateTime::ymd(int_group(&bestmatch, 1)?, 1, 1)?), copyear, env));
    }
    Ok(None)
}
