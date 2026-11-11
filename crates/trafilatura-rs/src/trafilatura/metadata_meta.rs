//! metadata.py `examine_meta` and `extract_opengraph`: `<meta>` tags.
use super::document::Document;
use super::json_metadata_authors::normalize_authors;
use super::meta_regexes::{CLEAN_META_TAGS, HTML_STRIP_TAGS};
use super::text::trim;
use super::xml_unescape::unescape;
use crate::dom::{Node, Result};
use crate::hooks::Hooks;
use crate::py::value::Value;

pub(crate) const METANAME_AUTHOR: &[&str] = &[
    "article:author",
    "atc-metaauthor",
    "author",
    "authors",
    "byl",
    "citation_author",
    "creator",
    "dc.creator",
    "dc.creator.aut",
    "dc:creator",
    "dcterms.creator",
    "dcterms.creator.aut",
    "dcsext.author",
    "parsely-author",
    "rbauthors",
    "sailthru.author",
    "shareaholic:article_author_name",
];
pub(crate) const METANAME_DESCRIPTION: &[&str] = &[
    "dc.description",
    "dc:description",
    "dcterms.abstract",
    "dcterms.description",
    "description",
    "sailthru.description",
    "twitter:description",
];
pub(crate) const METANAME_PUBLISHER: &[&str] = &[
    "article:publisher",
    "citation_journal_title",
    "copyright",
    "dc.publisher",
    "dc:publisher",
    "dcterms.publisher",
    "publisher",
    "sailthru.publisher",
    "rbpubname",
    "twitter:site",
];
pub(crate) const METANAME_TAG: &[&str] =
    &["citation_keywords", "dcterms.subject", "keywords", "parsely-tags", "shareaholic:keywords", "tags"];
pub(crate) const METANAME_TITLE: &[&str] = &[
    "citation_title",
    "dc.title",
    "dcterms.title",
    "fb_title",
    "headline",
    "parsely-title",
    "sailthru.title",
    "shareaholic:title",
    "rbtitle",
    "title",
    "twitter:title",
];
pub(crate) const METANAME_IMAGE: &[&str] =
    &["image", "og:image", "og:image:url", "og:image:secure_url", "twitter:image", "twitter:image:src"];
pub(crate) const OG_AUTHOR: &[&str] = &["og:author", "og:article:author"];
pub(crate) const PROPERTY_AUTHOR: &[&str] = &["author", "article:author"];
pub(crate) const TWITTER_ATTRS: &[&str] = &["twitter:site", "application-name"];

pub(crate) fn is_space_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(crate::py::is_space)
}

fn author_from(current: &Value, content: &str) -> Result<Value> {
    Ok(normalize_authors(current.as_str(), content)?.map_or(Value::None, Value::Str))
}

/// `metadata.x = metadata.x or content`.
pub(crate) fn keep_or(field: &mut Value, content: &str) {
    if !field.truthy() {
        *field = Value::str(content);
    }
}

/// `normalize_tags`.
fn normalize_tags(tags: &str) -> String {
    let tags = CLEAN_META_TAGS.sub(&trim(&unescape(tags)), "");
    tags.split(", ").filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ")
}

/// `extract_opengraph`, as the Document `examine_meta` starts from.
fn extract_opengraph(tree: Node, hooks: &dyn Hooks) -> Result<Document> {
    let mut doc = Document::default();
    for elem in tree.xpath_nodes(r#".//head/meta[starts-with(@property, "og:")]"#)? {
        let (property, content) = (elem.get("property").unwrap_or_default(), elem.get("content"));
        let Some(content) = content.filter(|c| !c.is_empty() && !is_space_only(c)) else { continue };
        let field = match property.as_str() {
            "og:title" => &mut doc.title,
            "og:description" => &mut doc.description,
            "og:site_name" => &mut doc.sitename,
            "og:image" | "og:image:url" | "og:image:secure_url" => &mut doc.image,
            "og:type" => &mut doc.pagetype,
            "og:url" => {
                if hooks.is_valid_url(&content)? {
                    doc.url = Value::Str(content);
                }
                continue;
            }
            p if OG_AUTHOR.contains(&p) => {
                doc.author = author_from(&Value::None, &content)?;
                continue;
            }
            _ => continue,
        };
        *field = Value::Str(content);
    }
    Ok(doc)
}

/// `examine_meta`.
pub fn examine_meta(tree: Node, hooks: &dyn Hooks) -> Result<Document> {
    let mut m = extract_opengraph(tree, hooks)?;
    if [&m.title, &m.author, &m.url, &m.description, &m.sitename, &m.image].iter().all(|v| v.truthy()) {
        return Ok(m);
    }
    let mut tags: Vec<Value> = Vec::new();
    let mut backup_sitename: Option<String> = None;
    for elem in tree.xpath_nodes(".//head/meta[@content]")? {
        let content = HTML_STRIP_TAGS.sub(&elem.get("content").unwrap_or_default(), "");
        if content.is_empty() || is_space_only(&content) {
            continue;
        }
        if elem.has_attr("property") {
            let property = elem.get("property").unwrap_or_default().to_lowercase();
            if property.starts_with("og:") {
                continue;
            }
            if property == "article:tag" {
                tags.push(Value::Str(normalize_tags(&content)));
            } else if PROPERTY_AUTHOR.contains(&property.as_str()) {
                m.author = author_from(&m.author, &content)?;
            } else if property == "article:publisher" {
                keep_or(&mut m.sitename, &content);
            } else if METANAME_IMAGE.contains(&property.as_str()) {
                keep_or(&mut m.image, &content);
            }
        } else if elem.has_attr("name") {
            let name = elem.get("name").unwrap_or_default().to_lowercase();
            let name = name.as_str();
            if METANAME_AUTHOR.contains(&name) {
                m.author = author_from(&m.author, &content)?;
            } else if METANAME_TITLE.contains(&name) {
                keep_or(&mut m.title, &content);
            } else if METANAME_DESCRIPTION.contains(&name) {
                keep_or(&mut m.description, &content);
            } else if METANAME_PUBLISHER.contains(&name) {
                keep_or(&mut m.sitename, &content);
            } else if TWITTER_ATTRS.contains(&name) || name.contains("twitter:app:name") {
                backup_sitename = Some(content);
            } else if name == "twitter:url" {
                if m.url.is_none() && hooks.is_valid_url(&content)? {
                    m.url = Value::Str(content);
                }
            } else if METANAME_TAG.contains(&name) {
                tags.push(Value::Str(normalize_tags(&content)));
            }
        } else if elem.has_attr("itemprop") {
            match elem.get("itemprop").as_deref() {
                Some("author") => m.author = author_from(&m.author, &content)?,
                Some("description") => keep_or(&mut m.description, &content),
                Some("headline") => keep_or(&mut m.title, &content),
                _ => {}
            }
        }
    }
    if let Some(backup) = backup_sitename {
        keep_or(&mut m.sitename, &backup);
    }
    if !tags.is_empty() {
        m.tags = Value::List(tags);
    }
    Ok(m)
}
