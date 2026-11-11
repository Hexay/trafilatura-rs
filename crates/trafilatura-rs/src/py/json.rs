//! Python `json.loads(s)` (C scanner, strict=True): where it succeeds, what it builds, and whether it fails with
//! JSONDecodeError or another exception.
use super::value::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    /// json.JSONDecodeError.
    Decode,
    /// Another exception: ValueError for > 4300-digit ints.
    Other,
    /// Input whose outcome depends on Python's stack depth (RecursionError) or a lone surrogate `\uD800`.
    Unsupported,
}

type Res<T> = Result<T, LoadError>;

/// Below Python's default recursion limit minus the frames under json.loads.
const MAX_DEPTH: usize = 900;
const MAX_INT_DIGITS: usize = 4300;

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    pos: usize,
    depth: usize,
    strict: bool,
}

pub fn loads(s: &str) -> Res<Value> {
    loads_with(s, true)
}

/// `json.loads(s, strict=strict)`: strict=False accepts raw control characters inside strings.
pub fn loads_with(s: &str, strict: bool) -> Res<Value> {
    if s.starts_with('\u{feff}') {
        return Err(LoadError::Decode);
    }
    let mut p = Parser { s: s.as_bytes(), src: s, pos: 0, depth: 0, strict };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.pos != p.s.len() {
        return Err(LoadError::Decode);
    }
    Ok(v)
}

impl Parser<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.s[self.pos..].starts_with(lit.as_bytes()) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Res<Value> {
        match self.s.get(self.pos) {
            Some(b'"') => {
                self.pos += 1;
                self.string().map(Value::Str)
            }
            Some(b'{') => self.nested(Self::object),
            Some(b'[') => self.nested(Self::array),
            Some(b'n') if self.eat("null") => Ok(Value::None),
            Some(b't') if self.eat("true") => Ok(Value::Bool(true)),
            Some(b'f') if self.eat("false") => Ok(Value::Bool(false)),
            Some(b'N') if self.eat("NaN") => Ok(Value::Float(f64::NAN)),
            Some(b'I') if self.eat("Infinity") => Ok(Value::Float(f64::INFINITY)),
            Some(b'-') if self.s[self.pos..].starts_with(b"-Infinity") => {
                self.pos += 9;
                Ok(Value::Float(f64::NEG_INFINITY))
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(LoadError::Decode),
        }
    }

    fn nested(&mut self, f: fn(&mut Self) -> Res<Value>) -> Res<Value> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(LoadError::Unsupported);
        }
        self.pos += 1;
        let v = f(self);
        self.depth -= 1;
        v
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.s.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }

    /// `(-?(?:0|[1-9]\d*))(\.\d+)?([eE][-+]?\d+)?`; a dangling fraction or exponent is not part of the number.
    fn number(&mut self) -> Res<Value> {
        let start = self.pos;
        self.eat("-");
        match self.s.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(LoadError::Decode),
        }
        let int_end = self.pos;
        let mut is_float = false;
        if self.s.get(self.pos) == Some(&b'.') && matches!(self.s.get(self.pos + 1), Some(b'0'..=b'9')) {
            self.pos += 1;
            self.digits();
            is_float = true;
        }
        if matches!(self.s.get(self.pos), Some(b'e' | b'E')) {
            let save = self.pos;
            self.pos += 1;
            if matches!(self.s.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                self.pos = save;
            } else {
                is_float = true;
            }
        }
        let text = &self.src[start..self.pos];
        if is_float {
            return Ok(Value::Float(text.parse().expect("float literal")));
        }
        let (neg, digits) = text.strip_prefix('-').map_or((false, text), |d| (true, d));
        if int_end - start - usize::from(neg) > MAX_INT_DIGITS {
            return Err(LoadError::Other);
        }
        Ok(Value::Int(if neg && digits != "0" { format!("-{digits}") } else { digits.to_owned() }))
    }

    fn hex4(&mut self) -> Res<u32> {
        let h = self.src.get(self.pos..self.pos + 4).ok_or(LoadError::Decode)?;
        if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(LoadError::Decode);
        }
        self.pos += 4;
        Ok(u32::from_str_radix(h, 16).expect("hex"))
    }

    /// After the opening quote.
    fn string(&mut self) -> Res<String> {
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(&b) = self.s.get(self.pos) {
                if b == b'"' || b == b'\\' || (b < 0x20 && self.strict) {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(&self.src[start..self.pos]);
            match self.s.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let esc = *self.s.get(self.pos).ok_or(LoadError::Decode)?;
                    self.pos += 1;
                    let c = match esc {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode_escape()?,
                        _ => return Err(LoadError::Decode),
                    };
                    out.push(c);
                }
                // strict=True: raw control characters, or the end of input
                _ => return Err(LoadError::Decode),
            }
        }
    }

    fn unicode_escape(&mut self) -> Res<char> {
        let hi = self.hex4()?;
        if (0xD800..0xDC00).contains(&hi) && self.s[self.pos..].starts_with(b"\\u") {
            let save = self.pos;
            self.pos += 2;
            let lo = self.hex4()?;
            if (0xDC00..0xE000).contains(&lo) {
                return Ok(char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)).expect("pair"));
            }
            self.pos = save;
        }
        char::from_u32(hi).ok_or(LoadError::Unsupported)
    }

    fn object(&mut self) -> Res<Value> {
        let mut items: Vec<(String, Value)> = Vec::new();
        self.ws();
        if self.eat("}") {
            return Ok(Value::Dict(items));
        }
        loop {
            if !self.eat("\"") {
                return Err(LoadError::Decode);
            }
            let key = self.string()?;
            self.ws();
            if !self.eat(":") {
                return Err(LoadError::Decode);
            }
            self.ws();
            let value = self.value()?;
            match items.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => items.push((key, value)),
            }
            self.ws();
            if self.eat("}") {
                return Ok(Value::Dict(items));
            }
            if !self.eat(",") {
                return Err(LoadError::Decode);
            }
            self.ws();
        }
    }

    fn array(&mut self) -> Res<Value> {
        let mut items = Vec::new();
        self.ws();
        if self.eat("]") {
            return Ok(Value::List(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            if self.eat("]") {
                return Ok(Value::List(items));
            }
            if !self.eat(",") {
                return Err(LoadError::Decode);
            }
            self.ws();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_json_edges() {
        assert_eq!(loads(" [1, 2.5e1, -0, NaN] ").map(|v| v.repr()), Ok("[1, 25.0, 0, nan]".into()));
        assert_eq!(loads(r#"{"a": 1, "a": 2, "b": "é😀"}"#).map(|v| v.repr()), Ok("{'a': 2, 'b': 'é😀'}".into()));
        assert_eq!(loads("[1,]"), Err(LoadError::Decode));
        assert_eq!(loads("01"), Err(LoadError::Decode));
        assert_eq!(loads("\"a\tb\""), Err(LoadError::Decode));
        assert_eq!(loads("1."), Err(LoadError::Decode));
        assert_eq!(loads(r#""\ud800""#), Err(LoadError::Unsupported));
    }
}
