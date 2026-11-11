//! Python `int(str)`.
use super::tables::DECIMAL_ZEROS;
use crate::dom::{Error, Result};

/// `unicodedata.decimal(c)`: the digit value of any Unicode decimal digit.
pub fn decimal_value(c: char) -> Option<u32> {
    if c.is_ascii_digit() {
        return Some(c as u32 - '0' as u32);
    }
    let cp = c as u32;
    let i = DECIMAL_ZEROS.partition_point(|&z| z <= cp);
    (i > 0 && cp - DECIMAL_ZEROS[i - 1] < 10).then(|| cp - DECIMAL_ZEROS[i - 1])
}

/// Python `int(s)` for str: surrounding whitespace, sign, Unicode decimal digits, single underscores between digits.
/// Saturates instead of growing unboundedly (only matters beyond i128).
pub fn py_int(s: &str) -> Result<i128> {
    let err = || Error::Value(format!("invalid literal for int() with base 10: {s:?}"));
    let body = super::strip(s);
    let (neg, digits) = match body.chars().next() {
        Some('-') => (true, &body[1..]),
        Some('+') => (false, &body[1..]),
        _ => (false, body),
    };
    let mut value: i128 = 0;
    let mut prev_digit = false;
    let mut any = false;
    for c in digits.chars() {
        if c == '_' {
            if !prev_digit {
                return Err(err());
            }
            prev_digit = false;
            continue;
        }
        let d = decimal_value(c).ok_or_else(err)?;
        value = value.saturating_mul(10).saturating_add(d as i128);
        prev_digit = true;
        any = true;
    }
    if !any || !prev_digit {
        return Err(err());
    }
    Ok(if neg { -value } else { value })
}
