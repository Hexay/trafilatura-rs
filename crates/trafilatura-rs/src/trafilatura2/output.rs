//! trafilatura 2.3.1 core.py `determine_returnstring` (with `_yaml_scalar`) and xml.py `build_json_output`, for
//! txt, markdown and json.
use super::xml::xmltotxt;
use crate::dom::{Ctx, Error, Node, Result};
use crate::py;
use crate::py::value::{dump_str, Value};
use crate::trafilatura::document::Document;
use crate::trafilatura::settings::{Format, Options};
use crate::trafilatura::text::normalize_unicode;

const YAML_RESERVED: &[&str] = &["true", "false", "yes", "no", "on", "off", "y", "n", "null", "none", "~"];

/// `_yaml_scalar`: plain when unambiguous, else the JSON string (a valid YAML double-quoted scalar).
fn yaml_scalar(value: &str) -> String {
    let plain = !value.is_empty()
        && value == py::strip(value)
        && value.chars().next().is_some_and(py::is_alpha)
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && !YAML_RESERVED.contains(&value.to_lowercase().as_str())
        && value.chars().all(|c| c >= ' ' && c != '\x7f');
    if plain {
        return value.to_owned();
    }
    let mut out = String::new();
    dump_str(value, &mut out);
    out
}

fn text_of(ctx: &Ctx, node: Option<Node>, formatting: bool) -> Result<String> {
    node.map_or(Ok(String::new()), |n| xmltotxt(ctx, n, formatting))
}

/// `";".join(v or [])`; a TypeError there escapes `extract()`, so it goes to the Python fallback to raise identically.
fn join_semicolon(v: &Value) -> Result<String> {
    let uncaught = || Error::Unsupported("uncaught TypeError in ';'.join".into());
    let items = match v {
        Value::List(items) => items.as_slice(),
        other if !other.truthy() => &[],
        _ => return Err(uncaught()),
    };
    let strs: Option<Vec<&str>> = items.iter().map(Value::as_str).collect();
    strs.map(|s| s.join(";")).ok_or_else(uncaught)
}

fn build_json_output(ctx: &Ctx, doc: &Document, with_metadata: bool) -> Result<String> {
    let text = Value::Str(text_of(ctx, doc.body, false)?);
    let comments = Value::Str(text_of(ctx, doc.commentsbody, false)?);
    let fields: Vec<(&str, Value)> = if with_metadata {
        // Document slot order; `text` and `comments` keep their slots
        vec![
            ("title", doc.title.clone()),
            ("author", doc.author.clone()),
            ("hostname", doc.hostname.clone()),
            ("date", doc.date.clone()),
            ("fingerprint", doc.fingerprint.clone()),
            ("id", doc.id.clone()),
            ("license", doc.license.clone()),
            ("comments", comments),
            ("raw_text", doc.raw_text.clone()),
            ("text", text),
            ("language", doc.language.clone()),
            ("image", doc.image.clone()),
            ("pagetype", doc.pagetype.clone()),
            ("filedate", doc.filedate.clone()),
            ("source", doc.url.clone()),
            ("source-hostname", doc.sitename.clone()),
            ("excerpt", doc.description.clone()),
            ("categories", Value::Str(join_semicolon(&doc.categories)?)),
            ("tags", Value::Str(join_semicolon(&doc.tags)?)),
        ]
    } else {
        vec![("text", text), ("comments", comments)]
    };
    let mut out = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        dump_str(k, &mut out);
        out.push_str(": ");
        v.dump(&mut out);
    }
    out.push('}');
    Ok(out)
}

/// `determine_returnstring`: the output string, NFC-normalized.
pub fn determine_returnstring(ctx: &Ctx, doc: &Document, opts: &Options) -> Result<String> {
    let out = match opts.format {
        Format::Json => build_json_output(ctx, doc, opts.with_metadata)?,
        Format::Txt | Format::Markdown => {
            let mut out = String::new();
            if opts.with_metadata {
                out.push_str("---\n");
                for (name, value) in doc.header_fields() {
                    if value.truthy() {
                        let rendered = value.as_str().map_or_else(|| value.to_py_str(), yaml_scalar);
                        out.push_str(&format!("{name}: {rendered}\n"));
                    }
                }
                out.push_str("---\n");
            }
            out.push_str(&text_of(ctx, doc.body, opts.formatting)?);
            let comments = text_of(ctx, doc.commentsbody, opts.formatting)?;
            py::strip(&format!("{out}\n{comments}")).to_owned()
        }
    };
    Ok(normalize_unicode(&out))
}
