//! trafilatura/json_metadata.py: JSON-LD metadata. Python's dynamic typing is kept: `in` on a str is a substring
//! test, wrong types raise (and `extract_metadata` swallows the exception, keeping the fields already set).
use super::document::Document;
use super::json_metadata_authors::normalize_authors;
use super::meta_regexes::{
    JSON_AUTHOR_1, JSON_AUTHOR_2, JSON_AUTHOR_REMOVE, JSON_CATEGORY, JSON_HEADLINE, JSON_NAME, JSON_PUBLISHER,
    JSON_REMOVE_HTML, JSON_SCHEMA_ORG, JSON_TYPE, JSON_UNICODE_REPLACE,
};
use super::text::trim;
use super::xml_unescape::unescape;
use crate::dom::{Error, Result};
use crate::py::json::{loads, LoadError};
use crate::py::pyre::{group, Pattern};
use crate::py::value::Value;

const JSON_ARTICLE_SCHEMA: &[&str] = &[
    "article",
    "backgroundnewsarticle",
    "blogposting",
    "medicalscholarlyarticle",
    "newsarticle",
    "opinionnewsarticle",
    "reportagenewsarticle",
    "scholarlyarticle",
    "socialmediaposting",
    "liveblogposting",
];
const JSON_OGTYPE_SCHEMA: &[&str] = &[
    "aboutpage",
    "checkoutpage",
    "collectionpage",
    "contactpage",
    "faqpage",
    "itempage",
    "medicalwebpage",
    "profilepage",
    "qapage",
    "realestatelisting",
    "searchresultspage",
    "webpage",
    "website",
    "article",
    "advertisercontentarticle",
    "newsarticle",
    "analysisnewsarticle",
    "askpublicnewsarticle",
    "backgroundnewsarticle",
    "opinionnewsarticle",
    "reportagenewsarticle",
    "reviewnewsarticle",
    "report",
    "satiricalarticle",
    "scholarlyarticle",
    "medicalscholarlyarticle",
    "socialmediaposting",
    "blogposting",
    "liveblogposting",
    "discussionforumposting",
    "techarticle",
    "blog",
    "jobposting",
];
const JSON_PUBLISHER_SCHEMA: &[&str] = &["newsmediaorganization", "organization", "webpage", "website"];
const AUTHOR_ATTRS: [&str; 3] = ["givenName", "additionalName", "familyName"];

/// `is_plausible_sitename`.
fn is_plausible_sitename(metadata: &Document, candidate: &Value, content_type: Option<&str>) -> Result<bool> {
    let Some(candidate) = candidate.as_str().filter(|c| !c.is_empty()) else { return Ok(false) };
    let sitename = &metadata.sitename;
    if !sitename.truthy() || (sitename.py_len()? < crate::py::len(candidate) && content_type != Some("webpage")) {
        return Ok(true);
    }
    Ok(sitename.truthy() && sitename.startswith("http")? && !candidate.starts_with("http"))
}

fn set_author(metadata: &mut Document, author_string: &str) -> Result<()> {
    let current = metadata.author.as_str().map(str::to_owned);
    metadata.author = normalize_authors(current.as_deref(), author_string)?.map_or(Value::None, Value::Str);
    Ok(())
}

/// The author loop of `process_parent` for article types.
fn process_authors(content: &Value, metadata: &mut Document) -> Result<()> {
    let mut list_authors = content.index("author")?.clone();
    if let Value::Str(s) = list_authors.clone() {
        match loads(&s) {
            Ok(v) => list_authors = v,
            Err(LoadError::Decode) => set_author(metadata, &s)?,
            Err(LoadError::Other) => return Err(Error::Value("json.loads".into())),
            Err(LoadError::Unsupported) => return Err(Error::Unsupported("json.loads".into())),
        }
    }
    let authors = match list_authors {
        Value::List(items) => items,
        other => vec![other],
    };
    for author in &authors {
        if !author.contains("@type")? || author.index("@type")?.as_str() == Some("Person") {
            let mut author_name = Value::None;
            if author.contains("name")? {
                author_name = author.dict_get("name")?;
                if let Value::List(names) = &author_name {
                    let names: Option<Vec<&str>> = names.iter().map(Value::as_str).collect();
                    let joined = names.ok_or_else(|| Error::Type("sequence item: expected str".into()))?.join("; ");
                    author_name = Value::Str(joined.trim_matches(|c| c == ';' || c == ' ').to_owned());
                } else if matches!(author_name, Value::Dict(_)) && author_name.contains("name")? {
                    author_name = author_name.index("name")?.clone();
                }
            } else if author.contains("givenName")? && author.contains("familyName")? {
                let mut parts = Vec::new();
                for attr in AUTHOR_ATTRS {
                    if author.contains(attr)? {
                        let part = author.index(attr)?;
                        parts.push(part.as_str().ok_or_else(|| Error::Type("expected str instance".into()))?);
                    }
                }
                author_name = Value::Str(parts.join(" "));
            }
            if let Value::Str(name) = &author_name {
                set_author(metadata, name)?;
            }
        }
    }
    Ok(())
}

/// `process_parent`.
fn process_parent(parent: &Value, metadata: &mut Document) -> Result<()> {
    for content in parent.iter()?.iter().filter(|c| c.truthy()) {
        if content.contains("publisher")? && content.index("publisher")?.contains("name")? {
            metadata.sitename = content.index("publisher")?.index("name")?.clone();
        }
        if !content.contains("@type")? || !content.index("@type")?.truthy() {
            continue;
        }
        let content_type = match content.index("@type")? {
            Value::List(items) => items[0].lower()?,
            other => other.lower()?,
        };
        if JSON_OGTYPE_SCHEMA.contains(&content_type.as_str()) && !metadata.pagetype.truthy() {
            metadata.pagetype = Value::Str(normalize_json(&content_type));
        }
        if JSON_PUBLISHER_SCHEMA.contains(&content_type.as_str()) {
            let mut candidate = content.dict_get("name")?;
            for key in ["legalName", "alternateName"] {
                if !candidate.truthy() {
                    candidate = content.dict_get(key)?;
                }
            }
            if is_plausible_sitename(metadata, &candidate, Some(&content_type))? {
                metadata.sitename = candidate;
            }
        } else if content_type == "person" {
            let name = content.dict_get("name")?;
            if name.truthy() && !name.startswith("http")? {
                set_author(metadata, name.as_str().expect("startswith succeeded on a str"))?;
            }
        } else if JSON_ARTICLE_SCHEMA.contains(&content_type.as_str()) {
            if content.contains("author")? {
                process_authors(content, metadata)?;
            }
            if !metadata.categories.truthy() && content.contains("articleSection")? {
                metadata.categories = match content.index("articleSection")? {
                    s @ Value::Str(_) => Value::List(vec![s.clone()]),
                    other => Value::List(other.iter()?.into_iter().filter(Value::truthy).collect()),
                };
            }
            if !metadata.title.truthy() {
                if content.contains("name")? && content_type == "article" {
                    metadata.title = content.index("name")?.clone();
                } else if content.contains("headline")? {
                    metadata.title = content.index("headline")?.clone();
                }
            }
        }
    }
    Ok(())
}

/// `extract_json`: mutates `metadata` in place, so fields set before an exception stay set.
pub fn extract_json(schema: &Value, metadata: &mut Document) -> Result<()> {
    let schema = match schema {
        Value::Dict(_) => Value::List(vec![schema.clone()]),
        other => other.clone(),
    };
    for parent in schema.iter()? {
        let context = parent.dict_get("@context")?;
        let Some(context) = context.as_str().filter(|c| !c.is_empty()) else { continue };
        if JSON_SCHEMA_ORG.match_(context).is_none() {
            continue;
        }
        let wrap = |v: &Value| if matches!(v, Value::List(_)) { v.clone() } else { Value::List(vec![v.clone()]) };
        let parent = if parent.contains("@graph")? {
            wrap(parent.index("@graph")?)
        } else if parent.contains("@type")?
            && matches!(parent.index("@type")?, Value::Str(t) if t.to_lowercase().contains("liveblogposting"))
            && parent.contains("liveBlogUpdate")?
        {
            wrap(parent.index("liveBlogUpdate")?)
        } else {
            schema.clone()
        };
        process_parent(&parent, metadata)?;
    }
    Ok(())
}

/// `extract_json_author`.
fn extract_json_author(elemtext: &str, pattern: &Pattern) -> Result<Option<String>> {
    let mut text = elemtext.to_owned();
    let mut authors: Option<String> = None;
    while let Some(caps) = pattern.search(&text) {
        let first = group(&caps, 1).ok_or_else(|| Error::Type("argument of type 'NoneType' is not iterable".into()))?;
        if !first.contains(' ') {
            break;
        }
        authors = normalize_authors(authors.as_deref(), first)?;
        text = pattern.sub_with(&text, 1, |_| String::new());
    }
    Ok(authors.filter(|a| !a.is_empty()))
}

/// `extract_json_parse_error`.
pub fn extract_json_parse_error(elem: &str, metadata: &mut Document) -> Result<()> {
    let element_text_author = JSON_AUTHOR_REMOVE.sub(elem, "");
    let mut author = extract_json_author(&element_text_author, &JSON_AUTHOR_1)?;
    if author.is_none() {
        author = extract_json_author(&element_text_author, &JSON_AUTHOR_2)?;
    }
    if let Some(author) = author {
        metadata.author = Value::Str(author);
    }
    if elem.contains("@type") {
        if let Some(m) = JSON_TYPE.search(elem) {
            let candidate = normalize_json(&group(&m, 1).unwrap_or("").to_lowercase());
            if JSON_OGTYPE_SCHEMA.contains(&candidate.as_str()) {
                metadata.pagetype = Value::Str(candidate);
            }
        }
    }
    if elem.contains("\"publisher\"") {
        if let Some(m) = JSON_PUBLISHER.search(elem) {
            let name = group(&m, 1).unwrap_or("");
            if !name.contains(',') {
                let candidate = Value::Str(normalize_json(name));
                if is_plausible_sitename(metadata, &candidate, None)? {
                    metadata.sitename = candidate;
                }
            }
        }
    }
    if elem.contains("\"articleSection\"") {
        if let Some(m) = JSON_CATEGORY.search(elem) {
            metadata.categories = Value::List(vec![Value::Str(normalize_json(group(&m, 1).unwrap_or("")))]);
        }
    }
    for (key, pattern) in [("\"name\"", &JSON_NAME), ("\"headline\"", &JSON_HEADLINE)] {
        if elem.contains(key) && !metadata.title.truthy() {
            if let Some(m) = pattern.search(elem) {
                metadata.title = Value::Str(normalize_json(group(&m, 1).unwrap_or("")));
                break;
            }
        }
    }
    Ok(())
}

/// `normalize_json`.
pub fn normalize_json(string: &str) -> String {
    let mut s = string.to_owned();
    if s.contains('\\') {
        s = s.replace("\\n", "").replace("\\r", "").replace("\\t", "");
        s = JSON_UNICODE_REPLACE.sub_with(&s, 0, |caps| {
            let code = u32::from_str_radix(group(caps, 1).expect("hex group"), 16).expect("hex");
            // surrogates are dropped right after in Python
            char::from_u32(code).map(String::from).unwrap_or_default()
        });
        s = unescape(&s);
    }
    trim(&JSON_REMOVE_HTML.sub(&s, ""))
}
