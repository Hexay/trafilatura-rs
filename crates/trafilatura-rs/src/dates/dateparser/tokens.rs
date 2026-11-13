//! dateparser.parser's `tokenizer` and the per-token reading of `_parser` (`_parse`, `_is_range`, the unset
//! numbers), with STRICT_DATE_ORDER "none".
use super::parser::{Stop, R};
use super::strptime::{name, number, Num};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Comp {
    Day,
    Month,
    Year,
}

/// A `_token_<part>` attribute: a (token, type) pair, or the bare token string the unset numbers leave.
#[derive(Clone, Debug)]
pub(super) enum Tok {
    Pair(String, u8),
    Str(String),
}

impl Tok {
    /// `_token_<part>[0]`
    fn first(&self) -> String {
        match self {
            Tok::Pair(t, _) => t.clone(),
            Tok::Str(s) => s.chars().next().map(String::from).unwrap_or_default(),
        }
    }
}

/// `tokenizer(ds).tokenize()`: runs of digits and colons (0), ASCII letters (1), anything else (2).
pub(super) fn tokenize(ds: &str) -> R<Vec<(String, u8)>> {
    let kind = |c: char| match c {
        '0'..='9' | ':' => 0,
        c if c.is_ascii_alphabetic() => 1,
        _ => 2,
    };
    let mut out: Vec<(String, u8)> = Vec::new();
    for c in ds.chars() {
        match out.last_mut() {
            Some((t, k)) if *k == kind(c) => t.push(c),
            _ => out.push((c.to_string(), kind(c))),
        }
    }
    // an empty string raises IndexError
    if out.is_empty() {
        return Err(Stop::Defer);
    }
    Ok(out)
}

pub(super) struct Fields {
    pub tokens: Vec<(String, u8)>,
    /// (token, type, index in `tokens`) of the digit and letter tokens
    pub filtered: Vec<(String, u8, usize)>,
    date_order: String,
    ordered: Vec<Comp>,
    pub day: Option<i64>,
    pub month: Option<i64>,
    pub year: Option<i64>,
    /// `_token_weekday` is set
    pub weekday: bool,
    pub time: Option<String>,
    pub token_day: Option<Tok>,
    pub token_month: Option<Tok>,
    pub token_year: Option<Tok>,
    pub weekday_modifier: Option<String>,
    unset: Vec<(String, u8)>,
    auto_order: Vec<Comp>,
}

fn order_of(date_order: &str) -> Vec<Comp> {
    date_order
        .chars()
        .map(|c| match c {
            'D' => Comp::Day,
            'M' => Comp::Month,
            _ => Comp::Year,
        })
        .collect()
}

impl Fields {
    pub fn new(tokens: Vec<(String, u8)>, date_order: &str) -> Fields {
        let filtered: Vec<(String, u8, usize)> =
            tokens.iter().enumerate().filter(|(_, t)| t.1 <= 1).map(|(i, t)| (t.0.clone(), t.1, i)).collect();
        let mut numbers: Vec<&str> = filtered
            .iter()
            .filter(|t| t.1 == 0 && !t.0.is_empty() && t.0.bytes().all(|b| b.is_ascii_digit()))
            .map(|t| t.0.as_str())
            .collect();
        numbers.sort_by_key(|n| n.len());
        let year_and_one =
            !filtered.iter().any(|t| t.1 == 1) && numbers.len() == 2 && numbers[0].len() <= 2 && numbers[1].len() == 4;
        Fields {
            ordered: if year_and_one { vec![Comp::Month, Comp::Day, Comp::Year] } else { order_of(date_order) },
            tokens,
            filtered,
            date_order: date_order.to_owned(),
            day: None,
            month: None,
            year: None,
            weekday: false,
            time: None,
            token_day: None,
            token_month: None,
            token_year: None,
            weekday_modifier: None,
            unset: Vec::new(),
            auto_order: Vec::new(),
        }
    }

    fn slot(&mut self, c: Comp) -> (&mut Option<i64>, &mut Option<Tok>) {
        match c {
            Comp::Day => (&mut self.day, &mut self.token_day),
            Comp::Month => (&mut self.month, &mut self.token_month),
            Comp::Year => (&mut self.year, &mut self.token_year),
        }
    }

    fn value(&self, c: Comp) -> Option<i64> {
        match c {
            Comp::Day => self.day,
            Comp::Month => self.month,
            Comp::Year => self.year,
        }
    }

    fn set(&mut self, c: Comp, token: &str, kind: u8, v: i64) {
        let (value, tok) = self.slot(c);
        *value = Some(v);
        *tok = Some(Tok::Pair(token.to_owned(), kind));
    }

    fn try_directives(&mut self, token: &str, comps: &[Comp], skip_year: bool, skip_y2: bool) -> R<bool> {
        for &c in comps {
            if skip_year && c == Comp::Year {
                continue;
            }
            let directives: &[Num] = match c {
                Comp::Day => &[Num::Day],
                Comp::Month => &[Num::Month],
                Comp::Year => &[Num::Year2, Num::Year4],
            };
            for &d in directives {
                if skip_y2 && d == Num::Year2 {
                    continue;
                }
                let Some(v) = number(token, d) else { continue };
                let reread = match self.value(c) {
                    None | Some(0) => None,
                    Some(_) => match self.slot(c).1.clone() {
                        Some(Tok::Pair(prev, kind)) => Some((prev, kind)),
                        _ => return Err(Stop::Defer),
                    },
                };
                match reread {
                    None => {}
                    Some((prev, 0)) if number(&prev, d).is_none() => self.unset.push((prev, 0)),
                    Some(_) => continue,
                }
                self.auto_order.push(c);
                self.set(c, token, 0, v);
                return Ok(c == Comp::Year);
            }
        }
        Err(Stop::Fail)
    }

    /// `parse_number(token, skip_component)`: whether it read the year.
    pub fn parse_number(&mut self, token: &str, skip_year: bool) -> R<bool> {
        let comps = if skip_year && self.day.is_none() && self.month.is_none() && !self.date_order.starts_with('Y') {
            vec![Comp::Month, Comp::Day, Comp::Year]
        } else {
            self.ordered.clone()
        };
        let year_at = self.ordered.iter().position(|&c| c == Comp::Year).expect("a year in the order");
        if self.ordered[year_at + 1..].iter().any(|&c| self.value(c).is_some()) {
            match self.try_directives(token, &comps, skip_year, true) {
                Err(Stop::Fail) => {}
                read => return read,
            }
        }
        self.try_directives(token, &comps, skip_year, false)
    }

    /// `_month_number_becomes_day`, whose errors `parse_alpha` suppresses (what it changed stays changed).
    fn month_number_becomes_day(&mut self) -> bool {
        if self.day.is_some_and(|d| d != 0) {
            let Some(i) = self.auto_order.iter().position(|&c| c == Comp::Day) else { return false };
            self.auto_order.remove(i);
        }
        let Some(i) = self.auto_order.iter().position(|&c| c == Comp::Month) else { return false };
        self.auto_order[i] = Comp::Day;
        true
    }

    /// `parse_alpha(token)`: weekday and month names.
    pub fn parse_alpha(&mut self, token: &str) -> R<bool> {
        for weekday in [true, false] {
            for full in [true, false] {
                let Some(v) = name(token, weekday, full) else { continue };
                if weekday {
                    if !self.weekday {
                        self.weekday = true;
                        return Ok(false);
                    }
                    continue;
                }
                let Some(prev) = self.month.filter(|&m| m != 0) else {
                    self.set(Comp::Month, token, 1, v as i64);
                    return Ok(false);
                };
                let day_free = self.day.is_none_or(|d| d == 0);
                let range = || {
                    let (Some(m), Some(d)) = (&self.token_month, &self.token_day) else { return false };
                    self.is_range(&m.first(), &d.first())
                };
                if (day_free || range()) && self.month_number_becomes_day() {
                    self.token_day = self.token_month.take();
                    self.set(Comp::Month, token, 1, v as i64);
                    self.day = Some(prev);
                    return Ok(false);
                }
            }
        }
        Err(Stop::Fail)
    }

    /// `_is_range(first, last)`
    fn is_range(&self, first: &str, last: &str) -> bool {
        let t = &self.tokens;
        let dash =
            |i: isize| i >= 0 && (i as usize) < t.len() && matches!(t[i as usize].0.trim_start_matches('.'), "-" | "–");
        (0..t.len().saturating_sub(2)).any(|i| {
            let j = i as isize;
            t[i].0 == first && dash(j + 1) && t[i + 2].0 == last && !dash(j - 1) && !dash(j + 3)
        })
    }

    /// The end of `__init__`: numbers displaced from a part fill the parts still missing.
    pub fn assign_unset(&mut self) -> R<()> {
        let unknown: Vec<Comp> =
            [Comp::Year, Comp::Month, Comp::Day].into_iter().filter(|&c| self.value(c).is_none()).collect();
        let day = self.token_day.as_ref().map(Tok::first);
        let unset: Vec<String> = self
            .unset
            .iter()
            .filter(|u| !day.as_ref().is_some_and(|d| self.is_range(d, &u.0)))
            .map(|u| u.0.clone())
            .collect();
        if unset.len() > unknown.len() {
            return Err(Stop::Fail);
        }
        for &c in &unknown {
            for token in &unset {
                let v = token.parse::<i64>().map_err(|_| Stop::Defer)?;
                let (value, tok) = self.slot(c);
                *value = Some(v);
                *tok = Some(Tok::Str(token.clone()));
            }
        }
        Ok(())
    }
}
