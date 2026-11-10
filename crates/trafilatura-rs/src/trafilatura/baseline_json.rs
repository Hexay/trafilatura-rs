//! Just enough of CPython's `json.loads` (C scanner, strict=True) to evaluate
//! `json.loads(text).get("articleBody")` in baseline() with Python's acceptance rules and value truthiness.

/// A decoded JSON value, reduced to what baseline() inspects.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Truthiness of the int/float.
    Number(bool),
    /// Decoded string; `lone_surrogate` marks strings Python could hold but never encode to UTF-8.
    Str {
        s: String,
        lone_surrogate: bool,
    },
    List(bool),
    /// Only the last "articleBody" member is kept (Python dicts keep the last duplicate).
    Dict {
        non_empty: bool,
        article_body: Option<Box<Value>>,
    },
}

// CPython raises RecursionError (an Exception, caught by baseline's except) near its 1000-frame limit.
const MAX_DEPTH: usize = 990;

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

type Res<T> = Result<T, ()>;

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn lit(&mut self, word: &str) -> bool {
        if self.src[self.i..].starts_with(word) {
            self.i += word.len();
            return true;
        }
        false
    }

    fn value(&mut self, depth: usize) -> Res<Value> {
        let c = *self.s.get(self.i).ok_or(())?;
        match c {
            b'"' => self.string().map(|(s, lone_surrogate)| Value::Str { s, lone_surrogate }),
            b'{' if depth < MAX_DEPTH => self.object(depth + 1),
            b'[' if depth < MAX_DEPTH => self.array(depth + 1),
            b'n' if self.lit("null") => Ok(Value::Null),
            b't' if self.lit("true") => Ok(Value::Bool(true)),
            b'f' if self.lit("false") => Ok(Value::Bool(false)),
            b'N' if self.lit("NaN") => Ok(Value::Number(true)),
            b'I' if self.lit("Infinity") => Ok(Value::Number(true)),
            b'-' if self.lit("-Infinity") => Ok(Value::Number(true)),
            _ => self.number(),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Res<Value> {
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.s.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(()),
        }
        let int_end = self.i;
        let mut is_float = false;
        if self.s.get(self.i) == Some(&b'.') && self.s.get(self.i + 1).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
            self.digits();
            is_float = true;
        }
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            let save = self.i;
            self.i += 1;
            if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                self.i = save;
            } else {
                is_float = true;
            }
        }
        let text = &self.src[start..self.i];
        Ok(Value::Number(if is_float {
            text.parse::<f64>().map_err(|_| ())? != 0.0
        } else {
            self.src[start..int_end].bytes().any(|b| (b'1'..=b'9').contains(&b))
        }))
    }

    fn hex4(&mut self) -> Res<u32> {
        let h = self.src.get(self.i..self.i + 4).ok_or(())?;
        if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(());
        }
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| ())
    }

    /// `scanstring`: (decoded, contains a lone surrogate).
    fn string(&mut self) -> Res<(String, bool)> {
        self.i += 1;
        let mut out = String::new();
        let mut lone = false;
        loop {
            let c = self.src[self.i..].chars().next().ok_or(())?;
            self.i += c.len_utf8();
            match c {
                '"' => return Ok((out, lone)),
                '\\' => {
                    let e = *self.s.get(self.i).ok_or(())?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut u = self.hex4()?;
                            if (0xD800..=0xDBFF).contains(&u) && self.src[self.i..].starts_with("\\u") {
                                let save = self.i;
                                self.i += 2;
                                let low = self.hex4()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    u = 0x10000 + (((u - 0xD800) << 10) | (low - 0xDC00));
                                } else {
                                    self.i = save;
                                }
                            }
                            match char::from_u32(u) {
                                Some(ch) => out.push(ch),
                                None => {
                                    lone = true;
                                    out.push('\u{FFFD}');
                                }
                            }
                        }
                        _ => return Err(()),
                    }
                }
                c if (c as u32) < 0x20 => return Err(()),
                c => out.push(c),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Res<Value> {
        self.i += 1;
        self.ws();
        let mut article_body = None;
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Value::Dict { non_empty: false, article_body });
        }
        loop {
            if self.s.get(self.i) != Some(&b'"') {
                return Err(());
            }
            let (key, _) = self.string()?;
            self.ws();
            if self.s.get(self.i) != Some(&b':') {
                return Err(());
            }
            self.i += 1;
            self.ws();
            let v = self.value(depth)?;
            if key == "articleBody" {
                article_body = Some(Box::new(v));
            }
            self.ws();
            match self.s.get(self.i) {
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Dict { non_empty: true, article_body });
                }
                Some(b',') => {
                    self.i += 1;
                    self.ws();
                }
                _ => return Err(()),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Res<Value> {
        self.i += 1;
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Value::List(false));
        }
        loop {
            self.value(depth)?;
            self.ws();
            match self.s.get(self.i) {
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::List(true));
                }
                Some(b',') => {
                    self.i += 1;
                    self.ws();
                }
                _ => return Err(()),
            }
        }
    }
}

/// `json.loads(s)`; `Err` for anything Python raises on.
pub fn loads(s: &str) -> Result<Value, ()> {
    if s.starts_with('\u{feff}') {
        return Err(());
    }
    let mut p = Parser { s: s.as_bytes(), src: s, i: 0 };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != s.len() {
        return Err(());
    }
    Ok(v)
}
