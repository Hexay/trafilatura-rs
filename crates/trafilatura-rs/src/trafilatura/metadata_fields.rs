//! metadata.py field extractors that read the tree: title, author, URL, categories/tags, license, image.
use super::htmlprocessing::prune_unwanted_nodes;
use super::json_metadata_authors::normalize_authors;
use super::meta_regexes::{CATEGORY_HREF, HTMLTITLE_REGEX, LICENSE_REGEX, TAG_HREF, TEXT_LICENSE_REGEX};
use super::metadata_xpaths::{AUTHOR_DISCARD_XPATHS, AUTHOR_XPATHS, CATEGORIES_XPATHS, TAGS_XPATHS, TITLE_XPATHS};
use super::text::{line_processing, trim};
use crate::dom::{Ctx, Error, Node, Result};
use crate::hooks::Hooks;
use crate::py::pyre::group;

/// `extract_metainfo`.
pub(crate) fn extract_metainfo(tree: Node, expressions: &[&str], len_limit: usize) -> Result<Option<String>> {
    for expression in expressions {
        for elem in tree.xpath_nodes(expression)? {
            let content = trim(&elem.itertext(true).join(" "));
            let len = crate::py::len(&content);
            if !content.is_empty() && 2 < len && len < len_limit {
                return Ok(Some(content));
            }
        }
    }
    Ok(None)
}

/// `examine_title_element`: the trimmed `<title>` and the two parts around a separator.
pub fn examine_title_element(tree: Node) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let Some(title_element) = tree.xpath_first(".//head//title")? else { return Ok((None, None, None)) };
    let title = trim(&title_element.text_content());
    if let Some(m) = HTMLTITLE_REGEX.match_(&title) {
        let (first, second) = (group(&m, 1).map(str::to_owned), group(&m, 2).map(str::to_owned));
        return Ok((Some(title), first, second));
    }
    Ok((Some(title), None, None))
}

/// `extract_title`.
pub fn extract_title(tree: Node) -> Result<Option<String>> {
    let h1_results = tree.findall(".//h1");
    if h1_results.len() == 1 {
        let title = trim(&h1_results[0].text_content());
        if !title.is_empty() {
            return Ok(Some(title));
        }
    }
    if let Some(title) = extract_metainfo(tree, &TITLE_XPATHS, 200)? {
        return Ok(Some(title));
    }
    let (title, first, second) = examine_title_element(tree)?;
    if let Some(t) = [first, second].into_iter().flatten().find(|t| !t.is_empty() && !t.contains('.')) {
        return Ok(Some(t));
    }
    if let Some(h1) = h1_results.first() {
        return Ok(Some(h1.text_content()));
    }
    if let Some(h2) = tree.xpath_first(".//h2")? {
        return Ok(Some(h2.text_content()));
    }
    Ok(title)
}

/// `extract_author`.
pub fn extract_author(ctx: &Ctx, tree: Node) -> Result<Option<String>> {
    let subtree = prune_unwanted_nodes(ctx, tree.deep_copy(ctx), &AUTHOR_DISCARD_XPATHS, false)?;
    match extract_metainfo(subtree, &AUTHOR_XPATHS, 120)? {
        Some(author) if !author.is_empty() => normalize_authors(None, &author),
        other => Ok(other),
    }
}

/// `extract_url`: canonical (or x-default) link made absolute, validated and normalized, else `default_url`.
pub fn extract_url(tree: Node, default_url: Option<&str>, hooks: &dyn Hooks) -> Result<Option<String>> {
    let mut url = None;
    if let Some(element) = tree.xpath_first(r#".//head//link[@rel="canonical"][@href]"#)? {
        url = element.get("href");
    } else if let Some(element) = tree.xpath_first(r#".//head//link[@rel="alternate"][@hreflang="x-default"]"#)? {
        url = Some(element.get("href").ok_or_else(|| Error::Unsupported("uncaught KeyError: 'href'".into()))?);
    }
    if let Some(u) = url.as_deref().filter(|u| u.starts_with('/')) {
        let relative = u.to_owned();
        for element in tree.xpath_nodes(".//head//meta[@content]")? {
            let attrtype = [element.get("name"), element.get("property")]
                .into_iter()
                .flatten()
                .find(|a| !a.is_empty())
                .unwrap_or_default();
            if attrtype.starts_with("og:") || attrtype.starts_with("twitter:") {
                let base_url = hooks.get_base_url(&element.get("content").unwrap_or_default())?;
                if !base_url.is_empty() {
                    url = Some(base_url + &relative);
                    break;
                }
            }
        }
    }
    if let Some(u) = url {
        url = hooks.validated_url(&u)?;
    }
    Ok(url.filter(|u| !u.is_empty()).or_else(|| default_url.map(str::to_owned)))
}

/// `extract_catstags`: link texts of category/tag lists (or `article:section` metas), cleaned and deduplicated.
pub fn extract_catstags(metatype: &str, tree: Node) -> Result<Vec<String>> {
    let (expressions, href) =
        if metatype == "category" { (&CATEGORIES_XPATHS[..], &CATEGORY_HREF) } else { (&TAGS_XPATHS[..], &TAG_HREF) };
    let mut results = Vec::new();
    for expression in expressions {
        for elem in tree.xpath_nodes(expression)? {
            if href.is_match(&elem.get("href").unwrap_or_default()) {
                results.push(elem.text_content());
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
fn parse_license_element(element: Node, strict: bool) -> Option<String> {
    if let Some(m) = LICENSE_REGEX.search(&element.get("href").unwrap_or_default()) {
        return Some(format!("CC {} {}", group(&m, 1)?.to_uppercase(), group(&m, 2)?));
    }
    let text = element.text().filter(|t| !t.is_empty())?;
    if strict {
        return TEXT_LICENSE_REGEX.search(&text).map(|m| group(&m, 0).expect("match").to_owned());
    }
    Some(trim(&text))
}

/// `extract_license`.
pub fn extract_license(tree: Node) -> Result<Option<String>> {
    for element in tree.xpath_nodes(r#".//a[@rel="license"][@href]"#)? {
        if let Some(result) = parse_license_element(element, false) {
            return Ok(Some(result));
        }
    }
    let footer = r#".//footer//a[@href]|.//div[contains(@class, "footer") or contains(@id, "footer")]//a[@href]"#;
    for element in tree.xpath_nodes(footer)? {
        if let Some(result) = parse_license_element(element, true) {
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// `extract_image`.
pub fn extract_image(tree: Node) -> Result<Option<String>> {
    for expr in [
        r#".//head/meta[@property="og:image" or @property="og:image:url"][@content]"#,
        r#".//head/meta[@property="twitter:image" or @property="twitter:image:src"][@content]"#,
    ] {
        if let Some(elem) = tree.xpath_first(expr)? {
            return Ok(elem.get("content"));
        }
    }
    Ok(None)
}
