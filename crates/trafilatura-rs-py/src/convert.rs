//! Rust results as Python objects. Documents leave `py.detach` as (slot, `Value`) pairs (nodes aren't Send); trees
//! travel as nested lists plus their `etree.tostring`.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use trafilatura_rs::dom::{Error, Method, Node, Result};
use trafilatura_rs::py::value::Value;
use trafilatura_rs::trafilatura::document::Document;

pub type Fields = Vec<(&'static str, Value)>;

pub fn value<'py>(py: Python<'py>, v: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        Value::None => py.None().into_bound(py),
        Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        Value::Int(digits) => py.import("builtins")?.getattr("int")?.call1((digits,))?,
        Value::Float(f) => f.into_pyobject(py)?.into_any(),
        Value::Str(s) => s.into_pyobject(py)?.into_any(),
        Value::List(items) => {
            PyList::new(py, items.iter().map(|i| value(py, i)).collect::<PyResult<Vec<_>>>()?)?.into_any()
        }
        Value::Dict(items) => {
            let d = PyDict::new(py);
            for (k, v) in items {
                d.set_item(k, value(py, v)?)?;
            }
            d.into_any()
        }
    })
}

pub fn dict<'py>(py: Python<'py>, fields: &Fields) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, v) in fields {
        d.set_item(k, value(py, v)?)?;
    }
    Ok(d)
}

/// The metadata slots of `doc` (every slot but the bodies and the `text`/`comments` that bare_extraction fills),
/// plus `date_url`.
pub fn metadata_fields(doc: &Document) -> Fields {
    let mut fields: Fields = doc.header_fields().into_iter().map(|(k, v)| (k, v.clone())).collect();
    fields.extend([
        ("raw_text", doc.raw_text.clone()),
        ("language", doc.language.clone()),
        ("image", doc.image.clone()),
        ("pagetype", doc.pagetype.clone()),
        ("filedate", doc.filedate.clone()),
        ("date_url", doc.date_url.clone()),
    ]);
    fields
}

fn opt(s: Option<String>) -> Value {
    s.map_or(Value::None, Value::Str)
}

/// `[tag, [[name, value], ...], text, tail, [children]]`; None when the subtree holds comments or PIs.
fn element(node: Node) -> Option<Value> {
    if !node.is_element() {
        return None;
    }
    let attrs = node.attrs().into_iter().map(|(k, v)| Value::List(vec![Value::Str(k), Value::Str(v)])).collect();
    let children = node.children().into_iter().map(element).collect::<Option<Vec<_>>>()?;
    Some(Value::List(vec![
        Value::str(node.tag()),
        Value::List(attrs),
        opt(node.text()),
        opt(node.tail()),
        Value::List(children),
    ]))
}

/// A tree as `[element, its etree.tostring]` for python/trafilatura_rs/_bare.py to rebuild and check.
pub fn tree(node: Option<Node>) -> Result<Value> {
    let Some(node) = node else { return Ok(Value::None) };
    let spec = element(node).ok_or_else(|| Error::Unsupported("comment or PI in an output tree".into()))?;
    Ok(Value::List(vec![spec, Value::Str(node.tostring(Method::Xml, true))]))
}
