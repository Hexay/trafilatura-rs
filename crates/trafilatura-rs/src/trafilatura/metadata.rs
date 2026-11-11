//! metadata.py `extract_metadata` and the JSON-LD, sitename and clean-up steps around it.
use super::core::Request;
use super::document::Document;
use super::json_metadata::{extract_json, extract_json_parse_error, normalize_json};
use super::meta_regexes::{JSON_MINIFY, META_URL};
use super::metadata_fields::{
    examine_title_element, extract_author, extract_catstags, extract_image, extract_license, extract_title, extract_url,
};
use super::metadata_meta::examine_meta;
use super::text::line_processing;
use super::xml_unescape::unescape;
use crate::dom::{Ctx, Error, Node, Result};
use crate::htmldate::{find_date, DateParams};
use crate::py::case::title;
use crate::py::json::{loads, LoadError};
use crate::py::pyre::group;
use crate::py::value::Value;

const MAX_LEN: usize = 10000;

/// `extract_meta_json`: may fail half-way with fields already set, like Python.
fn extract_meta_json(tree: Node, metadata: &mut Document) -> Result<()> {
    let scripts = tree.xpath_nodes(r#".//script[@type="application/ld+json" or @type="application/settings+json"]"#)?;
    for elem in scripts {
        let Some(text) = elem.text().filter(|t| !t.is_empty()) else { continue };
        let minified = JSON_MINIFY.sub_with(&text, 0, |caps| group(caps, 1).unwrap_or("").to_owned());
        let element_text = normalize_json(&minified);
        match loads(&element_text) {
            Ok(schema) => extract_json(&schema, metadata)?,
            Err(LoadError::Decode) => extract_json_parse_error(&element_text, metadata)?,
            Err(LoadError::Other) => return Err(Error::Value("json.loads".into())),
            Err(LoadError::Unsupported) => return Err(Error::Unsupported("json.loads".into())),
        }
    }
    Ok(())
}

/// `extract_sitename`.
fn extract_sitename(tree: Node) -> Result<Value> {
    let (_, first, second) = examine_title_element(tree)?;
    Ok([first, second].into_iter().flatten().find(|p| !p.is_empty() && p.contains('.')).map_or(Value::None, Value::Str))
}

/// The sitename fix-ups of `extract_metadata` (list/dict values, '@' prefix, capitalization). Python raises
/// uncaught IndexError/AttributeError on odd JSON-LD values; those go to the Python fallback to raise identically.
fn fix_sitename(sitename: &Value) -> Result<Value> {
    let uncaught = || Error::Unsupported("uncaught exception in sitename fix-up".into());
    let value = match sitename {
        Value::List(items) => items.first().cloned().ok_or_else(uncaught)?,
        Value::Dict(_) => Value::Str(sitename.repr()),
        other => other.clone(),
    };
    let Value::Str(s) = value else { return Err(uncaught()) };
    let s = s.trim_start_matches('@');
    if !s.contains('.') && s.chars().next().is_some_and(|c| !c.is_uppercase()) {
        return title(s).map(Value::Str).ok_or_else(|| Error::Unsupported("str.title() with a capital sigma".into()));
    }
    Ok(Value::str(s))
}

/// `Document.clean_and_trim`: cap, unescape and `line_processing` every str field.
pub(crate) fn clean_and_trim(doc: &mut Document) {
    for slot in doc.slots_mut() {
        if let Value::Str(s) = slot {
            let mut value = s.clone();
            if crate::py::len(&value) > MAX_LEN {
                value = format!("{}…", value.chars().take(MAX_LEN - 1).collect::<String>());
            }
            *slot = line_processing(&unescape(&value), false, false).map_or(Value::None, Value::Str);
        }
    }
}

fn as_value(s: Option<String>) -> Value {
    s.map_or(Value::None, Value::Str)
}

/// `extract_metadata(tree, url, date_params)` with an empty author blacklist.
pub fn extract_metadata(ctx: &Ctx, tree: Node, req: &Request) -> Result<Document> {
    let hooks = req.hooks;
    let max_date = req.max_date.ok_or_else(|| Error::Unsupported("max_date not given".into()))?;
    let mut metadata = examine_meta(tree, hooks)?;
    if metadata.author.as_str().is_some_and(|a| !a.is_empty() && !a.contains(' ')) {
        metadata.author = Value::None;
    }
    // Python swallows any exception here (bugs in json_metadata.py), keeping what was set before it
    if let Err(e @ Error::Unsupported(_)) = extract_meta_json(tree, &mut metadata) {
        return Err(e);
    }
    if metadata.title.is_none() {
        metadata.title = as_value(extract_title(tree)?);
    }
    if metadata.author.is_none() {
        metadata.author = as_value(extract_author(ctx, tree)?);
    }
    if metadata.url.is_none() {
        metadata.url = as_value(extract_url(tree, req.url, hooks)?);
    }
    if let Some(url) = metadata.url.as_str() {
        metadata.hostname = as_value(hooks.extract_domain(url)?);
    }
    if metadata.image.is_none() {
        metadata.image = as_value(extract_image(tree)?);
    }
    let params = DateParams { original_date: true, extensive_search: true, max_date: max_date.to_owned() };
    metadata.date = as_value(find_date(ctx, tree, metadata.url.as_str(), &params, hooks)?);
    if metadata.sitename.is_none() {
        metadata.sitename = extract_sitename(tree)?;
    }
    if !metadata.sitename.is_none() {
        metadata.sitename = fix_sitename(&metadata.sitename)?;
    } else if let Some(url) = metadata.url.as_str().filter(|u| !u.is_empty()) {
        if let Some(m) = META_URL.match_(url) {
            metadata.sitename = as_value(group(&m, 1).map(str::to_owned));
        }
    }
    if !metadata.categories.truthy() {
        metadata.categories = Value::List(extract_catstags("category", tree)?.into_iter().map(Value::Str).collect());
    }
    if !metadata.tags.truthy() {
        metadata.tags = Value::List(extract_catstags("tag", tree)?.into_iter().map(Value::Str).collect());
    }
    metadata.license = as_value(extract_license(tree)?);
    metadata.filedate = Value::str(max_date);
    clean_and_trim(&mut metadata);
    Ok(metadata)
}
