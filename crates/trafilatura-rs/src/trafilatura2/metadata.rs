//! trafilatura 2.3.1 metadata.py `extract_metadata` with the JSON-LD and sitename steps around it.
use super::json_metadata::{extract_json, extract_json_parse_error, normalize_json};
use super::meta_regexes::JSON_MINIFY;
use super::metadata_fields::{extract_author, extract_catstags, extract_license, extract_title, extract_url};
use super::metadata_meta::examine_meta;
use crate::dom::{Ctx, Error, Node, Result};
use crate::htmldate::find_date;
use crate::py::case::title;
use crate::py::json::{loads_with, LoadError};
use crate::py::pyre::group;
use crate::py::value::Value;
use crate::trafilatura::core::Request;
use crate::trafilatura::document::Document;
use crate::trafilatura::meta_regexes::META_URL;
use crate::trafilatura::metadata::{blacklist_author, clean_and_trim, date_params};
use crate::trafilatura::metadata_fields::examine_title_element;
use crate::trafilatura::settings::Options;

/// `extract_meta_json`: may fail half-way with fields already set, like Python.
fn extract_meta_json(tree: Node, metadata: &mut Document) -> Result<()> {
    let scripts = tree.xpath_nodes(r#".//script[@type="application/ld+json" or @type="application/settings+json"]"#)?;
    for elem in scripts {
        let Some(text) = elem.text().filter(|t| !t.is_empty()) else { continue };
        let minified = JSON_MINIFY.sub_with(&text, 0, |caps| group(caps, 1).unwrap_or("").to_owned());
        let element_text = normalize_json(&minified);
        match loads_with(&element_text, false) {
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

/// The sitename fix-ups of `extract_metadata` ('@' prefix, capitalization). A non-str sitename raises an uncaught
/// AttributeError; that goes to the Python fallback to raise identically.
fn fix_sitename(sitename: &Value) -> Result<Value> {
    let Value::Str(s) = sitename else {
        return Err(Error::Unsupported("uncaught exception in sitename fix-up".into()));
    };
    let s = s.trim_start_matches('@');
    if !s.is_empty() && !s.contains('.') && s.chars().next().is_some_and(|c| !c.is_uppercase()) {
        return title(s).map(Value::Str).ok_or_else(|| Error::Unsupported("str.title() with a capital sigma".into()));
    }
    Ok(Value::str(s))
}

fn as_value(s: Option<String>) -> Value {
    s.map_or(Value::None, Value::Str)
}

/// `extract_metadata(tree, url, date_params, author_blacklist)`.
pub fn extract_metadata(ctx: &Ctx, tree: Node, req: &Request, opts: &Options) -> Result<Document> {
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
    if !metadata.title.truthy() {
        metadata.title = as_value(extract_title(ctx, tree)?);
    }
    blacklist_author(&mut metadata.author, &opts.author_blacklist)?;
    if !metadata.author.truthy() {
        metadata.author = as_value(extract_author(ctx, tree)?);
    }
    blacklist_author(&mut metadata.author, &opts.author_blacklist)?;
    if !metadata.url.truthy() {
        metadata.url = as_value(extract_url(tree, req.url, hooks)?);
    }
    if let Some(url) = metadata.url.as_str().filter(|u| !u.is_empty()) {
        metadata.hostname = as_value(hooks.extract_domain(url)?);
    }
    metadata.date = as_value(find_date(ctx, tree, metadata.url.as_str(), &date_params(opts, max_date), hooks)?);
    if !metadata.sitename.truthy() {
        metadata.sitename = extract_sitename(tree)?;
    }
    if metadata.sitename.truthy() {
        metadata.sitename = fix_sitename(&metadata.sitename)?;
    } else if let Some(url) = metadata.url.as_str().filter(|u| !u.is_empty()) {
        if let Some(m) = META_URL.match_(url) {
            metadata.sitename = as_value(group(&m, 1).map(str::to_owned));
        }
    }
    if !metadata.categories.truthy() {
        metadata.categories =
            Value::List(extract_catstags(ctx, "category", tree)?.into_iter().map(Value::Str).collect());
    }
    if !metadata.tags.truthy() {
        metadata.tags = Value::List(extract_catstags(ctx, "tag", tree)?.into_iter().map(Value::Str).collect());
    }
    metadata.license = as_value(extract_license(ctx, tree)?);
    metadata.filedate = Value::str(max_date);
    clean_and_trim(&mut metadata);
    Ok(metadata)
}
