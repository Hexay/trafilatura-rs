//! trafilatura 2.3.1 metadata.py field extractors that read the tree: title, author, URL, categories/tags,
//! license. Texts are read with `block_text` (word boundaries kept at block tags) where 1.11 used text_content().
use super::baseline::block_text;
use super::htmlprocessing::prune_unwanted_nodes;
use super::json_metadata_authors::normalize_authors;
use super::meta_regexes::{CATEGORY_HREF, TAG_HREF};
use super::metadata_xpaths::{AUTHOR_DISCARD_XPATHS, AUTHOR_XPATHS, CATEGORIES_XPATHS, TAGS_XPATHS, TITLE_XPATHS};
use crate::dom::{Ctx, Node, Result};
use crate::hooks::Hooks;
use crate::py;
use crate::py::pyre::group;
use crate::trafilatura::meta_regexes::{LICENSE_REGEX, TEXT_LICENSE_REGEX};
use crate::trafilatura::metadata_fields::{examine_title_element, extract_metainfo};
use crate::trafilatura::text::line_processing;

const URL_SELECTORS: [&str; 3] = [
    r#".//head//link[@rel="canonical"]"#,
    ".//head//base",
    r#".//head//link[@rel="alternate"][@hreflang="x-default"]"#,
];

fn non_empty(s: String) -> Option<String> {
    Some(s).filter(|s| !s.is_empty())
}

/// `extract_title`.
pub fn extract_title(ctx: &Ctx, tree: Node) -> Result<Option<String>> {
    let h1_results = tree.findall(".//h1");
    if h1_results.len() == 1 {
        if let Some(title) = non_empty(block_text(ctx, h1_results[0])?) {
            return Ok(Some(title));
        }
    }
    if let Some(title) = extract_metainfo(tree, &TITLE_XPATHS, 200)?.and_then(non_empty) {
        return Ok(Some(title));
    }
    let (title, first, second) = examine_title_element(tree)?;
    let title = title.unwrap_or_default();
    if let Some(t) =
        [first, second, Some(title.clone())].into_iter().flatten().find(|t| !t.is_empty() && !t.contains('.'))
    {
        return Ok(Some(t));
    }
    for &h1 in &h1_results {
        if let Some(title) = non_empty(block_text(ctx, h1)?) {
            return Ok(Some(title));
        }
    }
    // Python's `if title := block_text(h1)` loop leaves title = "" once any h1 exists
    let title = if h1_results.is_empty() { title } else { String::new() };
    let title = match tree.xpath_first(".//h2")? {
        Some(h2) => block_text(ctx, h2)?,
        None => title,
    };
    Ok(non_empty(title))
}

/// `extract_author`.
pub fn extract_author(ctx: &Ctx, tree: Node) -> Result<Option<String>> {
    let subtree = prune_unwanted_nodes(ctx, tree.deep_copy(ctx), &AUTHOR_DISCARD_XPATHS, false)?;
    match extract_metainfo(subtree, &AUTHOR_XPATHS, 120)? {
        Some(author) if !author.is_empty() => normalize_authors(None, &author),
        other => Ok(other),
    }
}

/// `extract_url`: canonical, base or x-default link made absolute, validated and normalized, else `default_url`.
pub fn extract_url(tree: Node, default_url: Option<&str>, hooks: &dyn Hooks) -> Result<Option<String>> {
    let mut url = None;
    for selector in URL_SELECTORS {
        url = tree.xpath_first(selector)?.and_then(|e| e.get("href"));
        if url.as_deref().is_some_and(|u| !u.is_empty()) {
            break;
        }
    }
    if let Some(u) = url.as_deref().filter(|u| u.starts_with("//")) {
        url = Some(format!("https:{u}"));
    } else if let Some(u) = url.as_deref().filter(|u| u.starts_with('/')) {
        let relative = u.to_owned();
        for element in tree.xpath_nodes(".//head//meta[@content]")? {
            let attrtype = [element.get("name"), element.get("property")]
                .into_iter()
                .flatten()
                .find(|a| !a.is_empty())
                .unwrap_or_default();
            if attrtype.starts_with("og:") || attrtype.starts_with("twitter:") {
                let base_url = hooks.safe_base_url(&element.get("content").unwrap_or_default())?;
                if !base_url.is_empty() {
                    url = Some(base_url + &relative);
                    break;
                }
            }
        }
    }
    if let Some(u) = url.filter(|u| !u.is_empty()) {
        url = hooks.validated_url(&u)?;
    } else {
        url = None;
    }
    Ok(url.filter(|u| !u.is_empty()).or_else(|| default_url.map(str::to_owned)))
}

/// `extract_catstags`: link texts of category/tag lists (or `article:section` metas), cleaned and deduplicated.
pub fn extract_catstags(ctx: &Ctx, metatype: &str, tree: Node) -> Result<Vec<String>> {
    let (expressions, href) =
        if metatype == "category" { (&CATEGORIES_XPATHS[..], &CATEGORY_HREF) } else { (&TAGS_XPATHS[..], &TAG_HREF) };
    let mut results = Vec::new();
    for expression in expressions {
        for elem in tree.xpath_nodes(expression)? {
            if href.is_match(&elem.get("href").unwrap_or_default()) {
                results.push(block_text(ctx, elem)?);
            }
        }
        if !results.is_empty() {
            break;
        }
    }
    if metatype == "category" && results.is_empty() {
        let metas = r#".//head//meta[@property="article:section" or contains(@name, "subject")][@content]"#;
        for element in tree.xpath_nodes(metas)? {
            results.push(element.get("content").unwrap_or_default());
        }
    }
    let mut out: Vec<String> = Vec::new();
    for r in results.iter().filter(|r| !r.is_empty()) {
        if let Some(line) = line_processing(r, false, false).filter(|l| !l.is_empty()) {
            if !out.contains(&line) {
                out.push(line);
            }
        }
    }
    Ok(out)
}

/// `parse_license_element`.
fn parse_license_element(ctx: &Ctx, element: Node, strict: bool) -> Result<Option<String>> {
    if let Some(m) = LICENSE_REGEX.search(&element.get("href").unwrap_or_default()) {
        let part = |i| group(&m, i).unwrap_or_default();
        return Ok(Some(format!("CC {} {}", part(1).to_uppercase(), part(2))));
    }
    let Some(text) = non_empty(block_text(ctx, element)?) else { return Ok(None) };
    if strict {
        return Ok(TEXT_LICENSE_REGEX.search(&text).map(|m| group(&m, 0).expect("match").to_owned()));
    }
    Ok(Some(text))
}

/// `extract_license`.
pub fn extract_license(ctx: &Ctx, tree: Node) -> Result<Option<String>> {
    for element in tree.xpath_nodes(".//a[@rel][@href]")? {
        let rel = element.get("rel").unwrap_or_default().to_lowercase();
        if !py::split(&rel).any(|r| r == "license") {
            continue;
        }
        if let Some(result) = parse_license_element(ctx, element, false)? {
            return Ok(Some(result));
        }
    }
    let footer = r#".//footer//a[@href]|.//div[contains(@class, "footer") or contains(@id, "footer")]//a[@href]"#;
    for element in tree.xpath_nodes(footer)? {
        if let Some(result) = parse_license_element(ctx, element, true)? {
            return Ok(Some(result));
        }
    }
    Ok(None)
}
