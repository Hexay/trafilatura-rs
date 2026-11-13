//! output_format="csv": xml.py `xmltocsv`'s row (tab-delimited, `csv.QUOTE_MINIMAL`, "\r\n" terminator).
use super::document::Document;
use crate::dom::{Error, Result};
use crate::py::value::Value;

const NULL: &str = "null";

fn field(value: &Value) -> Result<String> {
    match value {
        Value::Str(s) if !s.is_empty() => Ok(s.clone()),
        _ if !value.truthy() => Ok(NULL.to_owned()),
        _ => Err(Error::Unsupported("non-str metadata in xmltocsv".into())),
    }
}

/// `csv.writer(output, delimiter="\t", quoting=csv.QUOTE_MINIMAL).writerow(fields)`.
fn writerow(fields: &[String]) -> Result<String> {
    if fields.iter().any(|f| f.contains('\0')) {
        return Err(Error::Unsupported("NUL in a csv field".into()));
    }
    let quoted: Vec<String> =
        fields
            .iter()
            .map(|f| {
                if f.contains(['\t', '"', '\r', '\n']) {
                    format!("\"{}\"", f.replace('"', "\"\""))
                } else {
                    f.clone()
                }
            })
            .collect();
    Ok(quoted.join("\t") + "\r\n")
}

/// The `xmltocsv` row for `posttext` / `commentstext` (both versions map empty values to "null").
pub fn csv_output(doc: &Document, posttext: &str, commentstext: &str) -> Result<String> {
    let text = |s: &str| if s.is_empty() { NULL.to_owned() } else { s.to_owned() };
    let fields = [
        field(&doc.url)?,
        field(&doc.id)?,
        field(&doc.fingerprint)?,
        field(&doc.hostname)?,
        field(&doc.title)?,
        field(&doc.image)?,
        field(&doc.date)?,
        text(posttext),
        text(commentstext),
        field(&doc.license)?,
        field(&doc.pagetype)?,
    ];
    writerow(&fields)
}
