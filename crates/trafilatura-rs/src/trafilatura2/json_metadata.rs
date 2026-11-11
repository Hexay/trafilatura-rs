//! trafilatura 2.3.1 json_metadata.py: JSON-LD metadata. As in 1.11 Python's dynamic typing is kept (wrong types
//! raise, and `extract_metadata` swallows the exception, keeping the fields already set).
use super::json_metadata_authors::normalize_authors;
use crate::dom::{Error, Result};
use crate::py::json::{loads, LoadError};
use crate::py::pyre::{group, Pattern};
use crate::py::value::Value;
use crate::trafilatura::document::Document;
use crate::trafilatura::json_metadata::{
    is_plausible_sitename, JSON_ARTICLE_SCHEMA, JSON_OGTYPE_SCHEMA, JSON_PUBLISHER_SCHEMA,
};
use crate::trafilatura::meta_regexes::{
    HTML_STRIP_TAGS, JSON_AUTHOR_1, JSON_AUTHOR_2, JSON_AUTHOR_REMOVE, JSON_CATEGORY, JSON_HEADLINE, JSON_NAME,
    JSON_PUBLISHER, JSON_SCHEMA_ORG, JSON_TYPE, JSON_UNICODE_REPLACE,
};
use crate::trafilatura::text::trim;
use crate::trafilatura::xml_unescape::unescape;

const AUTHOR_ATTRS: [&str; 3] = ["givenName", "additionalName", "familyName"];

/// `JSON_SCHEMA_TYPES`.
fn is_schema_type(t: &str) -> bool {
    JSON_ARTICLE_SCHEMA.contains(&t)
        || JSON_OGTYPE_SCHEMA.contains(&t)
        || JSON_PUBLISHER_SCHEMA.contains(&t)
        || t == "person"
}

/// `utils.as_list`.
fn as_list(v: &Value) -> Vec<Value> {
    match v {
        Value::None => vec![],
        Value::List(items) => items.clone(),
        other => vec![other.clone()],
    }
}

fn set_author(metadata: &mut Document, author_string: &str) -> Result<()> {
    let current = metadata.author.as_str().map(str::to_owned);
    metadata.author = normalize_authors(current.as_deref(), author_string)?.map_or(Value::None, Value::Str);
    Ok(())
}

fn type_error(what: &str) -> Error {
    Error::Type(what.into())
}

/// The name of one `author` entry, as the article branch of `process_parent` reads it.
fn author_name(author: &Value) -> Result<Value> {
    if author.contains("name")? {
        let name = author.dict_get("name")?;
        return Ok(match &name {
            Value::List(names) => {
                let names: Option<Vec<&str>> = names.iter().map(Value::as_str).collect();
                let joined = names.ok_or_else(|| type_error("sequence item: expected str"))?.join("; ");
                Value::Str(joined.trim_matches(|c| c == ';' || c == ' ').to_owned())
            }
            Value::Dict(_) if name.contains("name")? => name.index("name")?.clone(),
            _ => name,
        });
    }
    if author.contains("givenName")? && author.contains("familyName")? {
        let mut parts = Vec::new();
        for attr in AUTHOR_ATTRS {
            if author.contains(attr)? {
                parts.push(author.index(attr)?.as_str().ok_or_else(|| type_error("expected str instance"))?.to_owned());
            }
        }
        return Ok(Value::Str(parts.join(" ")));
    }
    Ok(Value::None)
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
    for author in as_list(&list_authors) {
        let author = match author {
            Value::Str(s) => Value::Dict(vec![("name".into(), Value::Str(s))]),
            other => other,
        };
        if !author.contains("@type")? || Value::List(as_list(author.index("@type")?)).contains("Person")? {
            if let Value::Str(name) = author_name(&author)? {
                set_author(metadata, &name)?;
            }
        }
    }
    Ok(())
}

/// `process_parent`.
fn process_parent(parents: &[Value], metadata: &mut Document) -> Result<()> {
    for content in parents.iter().filter(|c| c.truthy()) {
        let publisher = content.dict_get("publisher")?;
        if matches!(publisher, Value::Dict(_)) && is_plausible_sitename(metadata, &publisher.dict_get("name")?, None)? {
            metadata.sitename = publisher.index("name")?.clone();
        }
        if !content.contains("@type")? || !content.index("@type")?.truthy() {
            continue;
        }
        let types = as_list(content.index("@type")?);
        let recognized = types.iter().find(|t| t.as_str().is_some_and(|s| is_schema_type(&s.to_lowercase())));
        let content_type = recognized.unwrap_or(&types[0]).lower()?;
        let ct = content_type.as_str();
        if JSON_OGTYPE_SCHEMA.contains(&ct) && !metadata.pagetype.truthy() {
            metadata.pagetype = Value::Str(normalize_json(ct));
        }
        if JSON_PUBLISHER_SCHEMA.contains(&ct) {
            let mut candidate = content.dict_get("name")?;
            for key in ["legalName", "alternateName"] {
                if !candidate.truthy() {
                    candidate = content.dict_get(key)?;
                }
            }
            if is_plausible_sitename(metadata, &candidate, Some(ct))? {
                metadata.sitename = candidate;
            }
        } else if ct == "person" {
            if let Value::Str(name) = content.dict_get("name")? {
                if !name.starts_with("http") {
                    set_author(metadata, &name)?;
                }
            }
        } else if JSON_ARTICLE_SCHEMA.contains(&ct) {
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
                if content.contains("name")? && ct == "article" {
                    metadata.title = content.index("name")?.clone();
                } else if content.contains("headline")? {
                    metadata.title = content.index("headline")?.clone();
                }
            }
        }
    }
    Ok(())
}

/// `extract_json`: the schema.org blocks are collected first, then processed once.
pub fn extract_json(schema: &Value, metadata: &mut Document) -> Result<()> {
    let mut parents = Vec::new();
    for parent in as_list(schema) {
        let context = parent.dict_get("@context")?;
        let Some(context) = context.as_str().filter(|c| !c.is_empty()) else { continue };
        if JSON_SCHEMA_ORG.match_(context).is_none() {
            continue;
        }
        if parent.contains("@graph")? {
            parents.extend(as_list(parent.index("@graph")?));
        } else if parent.contains("@type")?
            && matches!(parent.index("@type")?, Value::Str(t) if t.to_lowercase().contains("liveblogposting"))
            && parent.contains("liveBlogUpdate")?
        {
            parents.extend(as_list(parent.index("liveBlogUpdate")?));
        } else {
            parents.push(parent);
        }
    }
    process_parent(&parents, metadata)
}

/// `extract_json_author`: names from the first non-empty group of each match.
fn extract_json_author(elemtext: &str, pattern: &Pattern) -> Result<Option<String>> {
    let mut text = elemtext.to_owned();
    let mut authors: Option<String> = None;
    while let Some(caps) = pattern.search(&text) {
        let name = (1..caps.len()).find_map(|i| group(&caps, i).filter(|g| !g.is_empty()));
        let Some(name) = name.filter(|n| n.contains(' ')) else { break };
        authors = normalize_authors(authors.as_deref(), name)?;
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
    trim(&HTML_STRIP_TAGS.sub(&s, ""))
}
