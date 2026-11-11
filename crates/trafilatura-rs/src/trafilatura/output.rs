//! core.py `determine_returnstring` and xml.py `build_json_output` for txt, markdown and json.
use super::document::Document;
use super::settings::{Format, Options};
use super::text::normalize_unicode;
use super::xml::xmltotxt;
use crate::dom::{Error, Node, Result};
use crate::py;
use crate::py::value::{dump_str, Value};

fn body_text(body: Option<Node>, formatting: bool) -> Result<String> {
    xmltotxt(body.expect("extracted body"), formatting)
}

/// `';'.join(v)`; a TypeError there escapes `extract()`, so it goes to the Python fallback to raise identically.
fn join_semicolon(v: &Value) -> Result<String> {
    let uncaught = || Error::Unsupported("uncaught TypeError in ';'.join".into());
    let Value::List(items) = v else { return Err(uncaught()) };
    let strs: Option<Vec<&str>> = items.iter().map(Value::as_str).collect();
    strs.map(|s| s.join(";")).ok_or_else(uncaught)
}

fn build_json_output(doc: &Document, with_metadata: bool) -> Result<String> {
    let text = Value::Str(body_text(doc.body, false)?);
    let comments = doc.commentsbody.map(|c| xmltotxt(c, false)).transpose()?.map(Value::Str);
    let fields: Vec<(&str, Value)> = if with_metadata {
        // Document slot order; `comments` keeps its slot even when filled from commentsbody.
        vec![
            ("title", doc.title.clone()),
            ("author", doc.author.clone()),
            ("hostname", doc.hostname.clone()),
            ("date", doc.date.clone()),
            ("fingerprint", doc.fingerprint.clone()),
            ("id", doc.id.clone()),
            ("license", doc.license.clone()),
            ("comments", comments.unwrap_or_default()),
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
        [("text", Some(text)), ("comments", comments)].into_iter().filter_map(|(k, v)| v.map(|v| (k, v))).collect()
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
pub fn determine_returnstring(doc: &Document, opts: &Options) -> Result<String> {
    let out = match opts.format {
        Format::Json => build_json_output(doc, opts.with_metadata)?,
        Format::Txt | Format::Markdown => {
            let mut out = String::new();
            if opts.with_metadata {
                out.push_str("---\n");
                for (name, value) in doc.header_fields() {
                    if value.truthy() {
                        out.push_str(&format!("{name}: {}\n", value.to_py_str()));
                    }
                }
                out.push_str("---\n");
            }
            out.push_str(&body_text(doc.body, opts.formatting)?);
            if let Some(comments) = doc.commentsbody {
                out = py::strip(&format!("{out}\n{}", xmltotxt(comments, opts.formatting)?)).to_owned();
            }
            out
        }
    };
    Ok(normalize_unicode(&out))
}
