//! Python objects as `json.loads` builds them (the values JSON-LD metadata can carry), with `str()`, `repr()`,
//! truthiness and `json.dumps(ensure_ascii=False)`.
use super::is_printable;
use crate::dom::{Error, Result};

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    #[default]
    None,
    Bool(bool),
    /// Decimal digits with an optional leading '-', canonical (no "-0", no leading zeros).
    Int(String),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    /// Insertion-ordered; a repeated key keeps its first position (Python dict assignment).
    Dict(Vec<(String, Value)>),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// Python truthiness.
    pub fn truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(s) => s != "0",
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(v) => !v.is_empty(),
            Value::Dict(v) => !v.is_empty(),
        }
    }

    /// `d[key]` lookup on a dict (None for a missing key or a non-dict).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Value::None => "NoneType",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::List(_) => "list",
            Value::Dict(_) => "dict",
        }
    }

    /// `needle in self`: dict keys, list items, substrings; TypeError for other containers.
    pub fn contains(&self, needle: &str) -> Result<bool> {
        match self {
            Value::Dict(items) => Ok(items.iter().any(|(k, _)| k == needle)),
            Value::List(items) => Ok(items.iter().any(|v| v.as_str() == Some(needle))),
            Value::Str(s) => Ok(s.contains(needle)),
            other => Err(Error::Type(format!("argument of type '{}' is not iterable", other.type_name()))),
        }
    }

    /// `self[key]` with a str key: dicts only (KeyError when missing), TypeError otherwise.
    pub fn index(&self, key: &str) -> Result<&Value> {
        match self {
            Value::Dict(_) => self.get(key).ok_or_else(|| Error::Runtime(format!("KeyError: {key:?}"))),
            other => Err(Error::Type(format!("'{}' object is not subscriptable by str", other.type_name()))),
        }
    }

    /// `self.get(key)`: dicts only, AttributeError otherwise.
    pub fn dict_get(&self, key: &str) -> Result<Value> {
        match self {
            Value::Dict(_) => Ok(self.get(key).cloned().unwrap_or(Value::None)),
            other => Err(attribute_error(other, "get")),
        }
    }

    /// `str.lower()` (AttributeError on non-str).
    pub fn lower(&self) -> Result<String> {
        self.as_str().map(str::to_lowercase).ok_or_else(|| attribute_error(self, "lower"))
    }

    /// `str.startswith(prefix)` (AttributeError on non-str).
    pub fn startswith(&self, prefix: &str) -> Result<bool> {
        self.as_str().map(|s| s.starts_with(prefix)).ok_or_else(|| attribute_error(self, "startswith"))
    }

    /// `iter(self)`: list items, dict keys, str characters; TypeError otherwise.
    pub fn iter(&self) -> Result<Vec<Value>> {
        match self {
            Value::List(items) => Ok(items.clone()),
            Value::Dict(items) => Ok(items.iter().map(|(k, _)| Value::Str(k.clone())).collect()),
            Value::Str(s) => Ok(s.chars().map(|c| Value::Str(c.to_string())).collect()),
            other => Err(Error::Type(format!("'{}' object is not iterable", other.type_name()))),
        }
    }

    /// `len(self)`.
    pub fn py_len(&self) -> Result<usize> {
        match self {
            Value::Str(s) => Ok(s.chars().count()),
            Value::List(v) => Ok(v.len()),
            Value::Dict(v) => Ok(v.len()),
            other => Err(Error::Type(format!("object of type '{}' has no len()", other.type_name()))),
        }
    }

    /// Python `str(value)`.
    pub fn to_py_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            other => other.repr(),
        }
    }

    /// Python `repr(value)`.
    pub fn repr(&self) -> String {
        match self {
            Value::None => "None".into(),
            Value::Bool(b) => if *b { "True" } else { "False" }.into(),
            Value::Int(s) => s.clone(),
            Value::Float(f) => float_repr(*f),
            Value::Str(s) => str_repr(s),
            Value::List(v) => format!("[{}]", v.iter().map(Value::repr).collect::<Vec<_>>().join(", ")),
            Value::Dict(items) => {
                let parts: Vec<String> = items.iter().map(|(k, v)| format!("{}: {}", str_repr(k), v.repr())).collect();
                format!("{{{}}}", parts.join(", "))
            }
        }
    }

    /// `json.dumps(value, ensure_ascii=False)` appended to `out`.
    pub fn dump(&self, out: &mut String) {
        match self {
            Value::None => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(s) => out.push_str(s),
            Value::Float(f) if f.is_nan() => out.push_str("NaN"),
            Value::Float(f) if f.is_infinite() => out.push_str(if *f > 0.0 { "Infinity" } else { "-Infinity" }),
            Value::Float(f) => out.push_str(&float_repr(*f)),
            Value::Str(s) => dump_str(s, out),
            Value::List(v) => {
                out.push('[');
                for (i, item) in v.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.dump(out);
                }
                out.push(']');
            }
            Value::Dict(items) => {
                out.push('{');
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    dump_str(k, out);
                    out.push_str(": ");
                    v.dump(out);
                }
                out.push('}');
            }
        }
    }
}

fn attribute_error(v: &Value, attr: &str) -> Error {
    Error::Runtime(format!("AttributeError: '{}' object has no attribute '{attr}'", v.type_name()))
}

/// JSON string literal as `json.dumps(ensure_ascii=False)` writes it.
pub fn dump_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Python `repr(str)`.
pub fn str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_ascii() || is_printable(c) => out.push(c),
            c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push(quote);
    out
}

/// Python `repr(float)`: shortest round-trip digits, positional for exponents -4..16, else `d.ddde+XX`.
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.into();
    }
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').expect("exponent");
    let exp: i32 = exp.parse().expect("exponent digits");
    let (sign, mantissa) = mantissa.strip_prefix('-').map_or(("", mantissa), |m| ("-", m));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let body = if (-4..16).contains(&exp) {
        let point = exp + 1;
        if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        }
    } else {
        let frac = if digits.len() > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{frac}e{}{:02}", &digits[..1], if exp < 0 { '-' } else { '+' }, exp.abs())
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_repr_matches_python() {
        for (f, want) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e16, "1e+16"),
            (1.5e-5, "1.5e-05"),
            (0.0001, "0.0001"),
            (123456789012345.6, "123456789012345.6"),
            (-2.5, "-2.5"),
            (1e22, "1e+22"),
        ] {
            assert_eq!(float_repr(f), want);
        }
    }

    #[test]
    fn str_repr_matches_python() {
        assert_eq!(str_repr("a'b"), "\"a'b\"");
        assert_eq!(str_repr("a'b\""), "'a\\'b\"'");
        assert_eq!(str_repr("é\u{a0}\u{200b}"), "'é\\xa0\\u200b'");
    }
}
