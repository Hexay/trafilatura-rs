//! CPython 3.12 `json.loads(s, strict=...)` (C scanner) producing full values, for baseline()'s JSON-LD and
//! Discourse preload walks. Only acceptance and the decoded strings/structure matter; numbers keep no value.

/// Stands in for a lone surrogate (`"\ud800"`): like it, this char is unprintable (dropped by
/// remove_control_characters), never part of an HTML5 entity name and never equal to other text.
pub const LONE_SURROGATE: char = '\u{10FFFF}';

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num,
    Str(String),
    List(Vec<Value>),
    /// Python dict: a repeated key keeps its first position and takes the last value.
    Dict(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

// CPython raises RecursionError (caught like any decode error) near its 1000-frame limit.
const MAX_DEPTH: usize = 990;
// sys.int_info.default_max_str_digits: longer int literals raise ValueError.
const MAX_INT_DIGITS: usize = 4300;

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
    strict: bool,
}

type Res<T> = Result<T, ()>;

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn lit(&mut self, word: &str) -> bool {
        let hit = self.src[self.i..].starts_with(word);
        if hit {
            self.i += word.len();
        }
        hit
    }

    fn value(&mut self, depth: usize) -> Res<Value> {
        match *self.s.get(self.i).ok_or(())? {
            b'"' => self.string().map(Value::Str),
            b'{' if depth < MAX_DEPTH => self.object(depth + 1),
            b'[' if depth < MAX_DEPTH => self.array(depth + 1),
            b'n' if self.lit("null") => Ok(Value::Null),
            b't' if self.lit("true") => Ok(Value::Bool(true)),
            b'f' if self.lit("false") => Ok(Value::Bool(false)),
            b'N' if self.lit("NaN") => Ok(Value::Num),
            b'I' if self.lit("Infinity") => Ok(Value::Num),
            b'-' if self.lit("-Infinity") => Ok(Value::Num),
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
        let int_digits = match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                1
            }
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(()),
        };
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
        if is_float {
            self.src[start..self.i].parse::<f64>().map_err(|_| ())?;
        } else if int_digits > MAX_INT_DIGITS {
            return Err(());
        }
        Ok(Value::Num)
    }

    fn hex4(&mut self) -> Res<u32> {
        let h = self.src.get(self.i..self.i + 4).ok_or(())?;
        if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(());
        }
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| ())
    }

    /// `scanstring`; strict mode rejects raw control characters.
    fn string(&mut self) -> Res<String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = self.src[self.i..].chars().next().ok_or(())?;
            self.i += c.len_utf8();
            match c {
                '"' => return Ok(out),
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
                            out.push(char::from_u32(u).unwrap_or(LONE_SURROGATE));
                        }
                        _ => return Err(()),
                    }
                }
                c if self.strict && (c as u32) < 0x20 => return Err(()),
                c => out.push(c),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Res<Value> {
        self.i += 1;
        self.ws();
        let mut items: Vec<(String, Value)> = Vec::new();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Value::Dict(items));
        }
        loop {
            if self.s.get(self.i) != Some(&b'"') {
                return Err(());
            }
            let key = self.string()?;
            self.ws();
            if self.s.get(self.i) != Some(&b':') {
                return Err(());
            }
            self.i += 1;
            self.ws();
            let v = self.value(depth)?;
            match items.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = v,
                None => items.push((key, v)),
            }
            self.ws();
            match self.s.get(self.i) {
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Dict(items));
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
        let mut items = Vec::new();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Value::List(items));
        }
        loop {
            items.push(self.value(depth)?);
            self.ws();
            match self.s.get(self.i) {
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::List(items));
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

/// `json.loads(s, strict=strict)`; `Err` for anything Python raises on.
pub fn loads(s: &str, strict: bool) -> Result<Value, ()> {
    if s.starts_with('\u{feff}') {
        return Err(());
    }
    let mut p = Parser { s: s.as_bytes(), src: s, i: 0, strict };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != s.len() {
        return Err(());
    }
    Ok(v)
}
