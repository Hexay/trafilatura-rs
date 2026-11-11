//! dateutil's `Decimal` token values and the `_ymd` list with `resolve_ymd(yearfirst=False, dayfirst=False)`.
use super::{Stop, R};

/// A finite non-negative `Decimal` as dateutil's tokens produce it: integer part and fraction digits.
#[derive(Clone, Copy, Debug)]
pub(super) struct Dec<'a> {
    pub int: i64,
    pub frac: &'a str,
}

impl<'a> Dec<'a> {
    /// `Decimal(token)`; tokens are digits, digits.digits, or something Decimal rejects (ValueError).
    pub fn parse(s: &'a str) -> R<Dec<'a>> {
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        let well_formed = !int.is_empty() && !frac.is_empty() == s.contains('.');
        if !(well_formed && all_digits(int) && all_digits(frac)) {
            let numeric_shape = s.bytes().all(|b| b.is_ascii_digit() || b == b'.');
            return Err(if numeric_shape { Stop::Defer } else { Stop::Invalid });
        }
        if frac.len() > 17 {
            return Err(Stop::Defer);
        }
        Ok(Dec { int: py_int(int)?, frac })
    }

    pub fn has_frac(&self) -> bool {
        self.frac.bytes().any(|b| b != b'0')
    }

    /// `int(60 * (value % 1))`.
    pub fn frac_times_60(&self) -> i64 {
        let digits = &self.frac[..self.frac.len().min(17)];
        let num: i128 = digits.parse().unwrap_or(0);
        (num * 60 / 10i128.pow(digits.len() as u32)) as i64
    }

    pub fn gt(&self, n: i64) -> bool {
        self.int > n || (self.int == n && self.has_frac())
    }

    pub fn le(&self, n: i64) -> bool {
        !self.gt(n)
    }
}

pub(super) fn all_digits(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

/// `int(s)` of a token slice (no signs, spaces or underscores occur); ValueError otherwise.
pub(super) fn py_int(s: &str) -> R<i64> {
    if s.is_empty() || !all_digits(s) {
        return Err(Stop::Invalid);
    }
    s.parse().map_err(|_| Stop::Defer)
}

pub(super) enum Val<'a> {
    Str(&'a str),
    Num(Dec<'a>),
    Int(i64),
}

#[derive(Default)]
pub(super) struct Ymd {
    pub vals: Vec<i64>,
    pub century_specified: bool,
    pub d: Option<usize>,
    pub m: Option<usize>,
    pub y: Option<usize>,
}

pub(super) fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Ymd {
    pub fn len(&self) -> usize {
        self.vals.len()
    }

    pub fn could_be_day(&self, v: &Dec) -> bool {
        if self.d.is_some() {
            return false;
        }
        let max = match (self.m, self.y) {
            (None, _) => 31,
            (Some(m), None) => days_in_month(2000, self.vals[m]),
            (Some(m), Some(y)) => days_in_month(self.vals[y], self.vals[m]),
        };
        v.int >= 1 && v.le(max)
    }

    pub fn append(&mut self, val: Val, label: Option<char>) -> R<()> {
        let mut label = label;
        let int = match val {
            Val::Str(s) => {
                if all_digits(s) && s.len() > 2 {
                    self.century(&mut label)?;
                }
                py_int(s)?
            }
            Val::Num(d) => {
                if d.gt(100) {
                    self.century(&mut label)?;
                }
                d.int
            }
            Val::Int(i) => {
                if i > 100 {
                    self.century(&mut label)?;
                }
                i
            }
        };
        self.vals.push(int);
        let idx = Some(self.vals.len() - 1);
        let slot = match label {
            Some('M') => &mut self.m,
            Some('D') => &mut self.d,
            Some('Y') => &mut self.y,
            _ => return Ok(()),
        };
        if slot.is_some() {
            return Err(Stop::Invalid);
        }
        *slot = idx;
        Ok(())
    }

    fn century(&mut self, label: &mut Option<char>) -> R<()> {
        self.century_specified = true;
        if !matches!(label, None | Some('Y')) {
            return Err(Stop::Invalid);
        }
        *label = Some('Y');
        Ok(())
    }

    /// `resolve_ymd(yearfirst=False, dayfirst=False)` as (year, month, day).
    pub fn resolve(&self) -> R<(Option<i64>, Option<i64>, Option<i64>)> {
        let v = &self.vals;
        let strids = [self.y, self.m, self.d];
        let known = strids.iter().flatten().count();
        if (v.len() == known && known > 0) || (v.len() == 3 && known == 2) {
            let mut idx = strids;
            if v.len() == 3 && known == 2 {
                let missing = (0..3).find(|i| !strids.contains(&Some(*i))).ok_or(Stop::Defer)?;
                *idx.iter_mut().find(|s| s.is_none()).ok_or(Stop::Defer)? = Some(missing);
            }
            let get = |i: Option<usize>| i.map(|i| v[i]);
            return Ok((get(idx[0]), get(idx[1]), get(idx[2])));
        }
        let (mut year, mut month, mut day) = (None, None, None);
        match v.len() {
            n if n > 3 => return Err(Stop::Invalid),
            1 | 2 if v.len() == 1 || self.m.is_some() => {
                let other = match self.m {
                    Some(m) => {
                        month = Some(v[m]);
                        v[(m + v.len() - 1) % v.len()]
                    }
                    None => v[0],
                };
                if v.len() > 1 || self.m.is_none() {
                    if other > 31 {
                        year = Some(other);
                    } else {
                        day = Some(other);
                    }
                }
            }
            2 => {
                if v[0] > 31 {
                    (year, month) = (Some(v[0]), Some(v[1]));
                } else if v[1] > 31 {
                    (month, year) = (Some(v[0]), Some(v[1]));
                } else {
                    (month, day) = (Some(v[0]), Some(v[1]));
                }
            }
            3 => {
                let (a, b, c) = (Some(v[0]), Some(v[1]), Some(v[2]));
                (year, month, day) = match self.m {
                    Some(0) if v[1] > 31 => (b, a, c),
                    Some(0) => (c, a, b),
                    Some(1) if v[0] > 31 => (a, b, c),
                    Some(1) => (c, b, a),
                    Some(2) if v[1] > 31 => (b, c, a),
                    Some(2) => (a, c, b),
                    _ if v[0] > 31 || self.y == Some(0) => (a, b, c),
                    _ if v[0] > 12 => (c, b, a),
                    _ => (c, a, b),
                };
            }
            _ => {}
        }
        Ok((year, month, day))
    }
}
