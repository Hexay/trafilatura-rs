//! Port of htmldate 1.11.0's `find_date`, as trafilatura 1.11's metadata extraction calls it.
//! dateutil and dateparser are not ported: they run through `Hooks` (see docs/metadata-formats-scope.md).
use crate::dom::{Ctx, Error, Method, Node, Result};
use crate::hooks::Hooks;
use crate::py::datetime::{today_end, DateTime};

mod elements;
mod extractors;
mod header;
#[rustfmt::skip]
mod regexes;
mod search_page;
mod searches;
mod validators;

/// The `date_extraction_params` trafilatura passes (`set_date_params`): `outputformat` "%Y-%m-%d", no min_date.
#[derive(Clone, Debug)]
pub struct DateParams {
    pub original_date: bool,
    pub extensive_search: bool,
    /// ISO date ("YYYY-MM-DD") as `set_date_params` formats today's local date.
    pub max_date: String,
}

/// htmldate's `Extractor` options plus the Python leaves.
pub(crate) struct Env<'a> {
    extensive: bool,
    max: DateTime,
    min: DateTime,
    original: bool,
    format: &'static str,
    hooks: &'a dyn Hooks,
}

const OUTPUT_FORMAT: &str = "%Y-%m-%d";
const MAX_POSSIBLE_CANDIDATES: usize = 1000;
const MIN_SEGMENT_LEN: usize = 6;
const MAX_SEGMENT_LEN: usize = 52;
const CLEANING_LIST: &[&str] = &[
    "applet", "audio", "canvas", "datalist", "embed", "frame", "frameset", "iframe", "label", "map", "math",
    "noframes", "object", "picture", "rdf", "svg", "track", "video",
];

/// `except ValueError`: Ok(None) for a ValueError, other errors propagate.
fn caught<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(Error::Value(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `s[a:b]` in code points.
fn cslice(s: &str, a: usize, b: usize) -> &str {
    let start = s.char_indices().nth(a).map_or(s.len(), |(i, _)| i);
    let end = s[start..].char_indices().nth(b.saturating_sub(a)).map_or(s.len(), |(i, _)| start + i);
    &s[start..end]
}

/// `str.isdigit()`.
fn isdigit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(crate::py::is_digit)
}

/// `has_plausible_candidates`.
fn has_plausible_candidates(n: usize) -> bool {
    0 < n && n <= MAX_POSSIBLE_CANDIDATES
}

/// `get_max_date`.
fn get_max_date(max_date: &str) -> Result<DateTime> {
    Ok(caught(DateTime::fromisoformat(max_date))?.unwrap_or_else(today_end))
}

/// `clean_html` (drop_tree keeps the tail text).
fn clean_html(ctx: &Ctx, tree: Node, elemlist: &[&str]) -> Result<()> {
    for element in tree.iter(elemlist) {
        element.drop_tree(ctx)?;
    }
    Ok(())
}

/// `find_date(tree, extensive_search, original_date, url=url, max_date=max_date)`. The input tree is not mutated.
pub fn find_date(
    ctx: &Ctx,
    tree: Node,
    url: Option<&str>,
    params: &DateParams,
    hooks: &dyn Hooks,
) -> Result<Option<String>> {
    let env = Env {
        extensive: params.extensive_search,
        max: get_max_date(&params.max_date)?,
        min: DateTime::ymd(1995, 1, 1)?,
        original: params.original_date,
        format: OUTPUT_FORMAT,
        hooks,
    };
    let mut url = url.map(str::to_owned);
    if url.is_none() {
        if let Some(urlelem) = tree.find(".//link[@rel=\"canonical\"]") {
            url = urlelem.get("href");
        }
    }
    if let Some(r) = searches::extract_url_date(&env, url.as_deref())? {
        return Ok(Some(r));
    }
    if let Some(r) = header::examine_header(&env, tree)? {
        return Ok(Some(r));
    }
    if let Some(r) = searches::json_search(&env, tree)? {
        return Ok(Some(r));
    }
    if let Some(r) = elements::examine_abbr_elements(&env, tree)? {
        return Ok(Some(r));
    }

    let search_tree = tree.deep_copy(ctx);
    let pruned =
        clean_html(ctx, search_tree, CLEANING_LIST).and_then(|()| searches::discard_unwanted(ctx, search_tree));
    // rare lxml error (NULL bytes, control characters): search the partly pruned tree
    caught(pruned)?;

    if let Some(r) = elements::examine_elements(&env, &elements::date_candidates(search_tree, env.extensive))? {
        return Ok(Some(r));
    }
    if let Some(r) = elements::examine_elements(&env, &search_tree.xpath_nodes(".//title|.//h1")?)? {
        return Ok(Some(r));
    }
    if let Some(r) = elements::examine_time_elements(&env, search_tree)? {
        return Ok(Some(r));
    }

    let htmlstring = search_tree.tostring(Method::Html, true);
    if let Some(r) = searches::pattern_search(&env, &htmlstring, &regexes::TIMESTAMP_PATTERN)? {
        return Ok(Some(r));
    }
    if let Some(r) = searches::img_search(&env, search_tree)? {
        return Ok(Some(r));
    }
    if let Some(r) = searches::idiosyncrasies_search(&env, &htmlstring)? {
        return Ok(Some(r));
    }

    if !env.extensive {
        clean_html(ctx, search_tree, &["script", "style"])?;
        let text = search_tree.itertext(true).join(" ");
        return searches::pattern_search(&env, &text, &regexes::TIMESTAMP_LOOSE_PATTERN);
    }

    let mut reference = None;
    for segment in search_tree.xpath_strings(extractors::FREE_TEXT_EXPRESSIONS)? {
        let segment = crate::py::strip(&segment);
        let n = crate::py::len(segment);
        if !(MIN_SEGMENT_LEN < n && n < MAX_SEGMENT_LEN) {
            continue;
        }
        reference = elements::compare_reference(&env, reference, segment)?;
    }
    if let Some(r) = validators::check_extracted_reference(reference, &env) {
        return Ok(Some(r));
    }
    search_page::search_page(&env, &htmlstring)
}
