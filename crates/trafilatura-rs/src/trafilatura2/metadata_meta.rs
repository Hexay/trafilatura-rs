//! trafilatura 2.3.1 metadata.py `examine_meta` and `extract_opengraph`: `<meta>` tags.
use super::json_metadata_authors::normalize_authors;
use crate::dom::{Node, Result};
use crate::hooks::Hooks;
use crate::py;
use crate::py::value::Value;
use crate::trafilatura::document::Document;
use crate::trafilatura::meta_regexes::{CLEAN_META_TAGS, HTML_STRIP_TAGS};
use crate::trafilatura::metadata_meta::{
    is_space_only, keep_or, METANAME_AUTHOR, METANAME_DESCRIPTION, METANAME_IMAGE, METANAME_PUBLISHER, METANAME_TAG,
    METANAME_TITLE, OG_AUTHOR, PROPERTY_AUTHOR, TWITTER_ATTRS,
};
use crate::trafilatura::text::trim;
use crate::trafilatura::xml_unescape::unescape;

fn author_from(current: &Value, content: &str) -> Result<Value> {
    Ok(normalize_authors(current.as_str(), content)?.map_or(Value::None, Value::Str))
}

/// `normalize_tags`.
fn normalize_tags(tags: &str) -> String {
    let trimmed = trim(&unescape(tags));
    let tags = CLEAN_META_TAGS.sub(&trimmed, "");
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
    let mut tags: Vec<Value> = Vec::new();
    let mut backup_sitename: Option<String> = None;
    for elem in tree.xpath_nodes(".//head/meta[@content]")? {
        let content = py::strip(&HTML_STRIP_TAGS.sub(&elem.get("content").unwrap_or_default(), "")).to_owned();
        if content.is_empty() {
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
            } else if METANAME_IMAGE.contains(&name) {
                keep_or(&mut m.image, &content);
            } else if TWITTER_ATTRS.contains(&name) || name.contains("twitter:app:name") {
                backup_sitename = Some(content);
            } else if name == "twitter:url" && !m.url.truthy() && hooks.is_valid_url(&content)? {
                m.url = Value::Str(content);
            } else if METANAME_TAG.contains(&name) {
                tags.push(Value::Str(normalize_tags(&content)));
            }
        } else if elem.has_attr("itemprop") {
            match elem.get("itemprop").unwrap_or_default().to_lowercase().as_str() {
                "author" => m.author = author_from(&m.author, &content)?,
                "description" => keep_or(&mut m.description, &content),
                "headline" => keep_or(&mut m.title, &content),
                _ => {}
            }
        }
    }
    if let Some(backup) = backup_sitename {
        keep_or(&mut m.sitename, &backup);
    }
    m.tags = Value::List(tags);
    Ok(m)
}
