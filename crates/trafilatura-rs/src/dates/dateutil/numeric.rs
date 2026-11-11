//! `parser._parse_numeric_token` and its hour/minute/second helpers.
use super::ymd::{py_int, Dec, Val, Ymd};
use super::{ampm, hms, jump, month, Res, Stop, R};

pub(super) fn adjust_ampm(hour: i64, ampm: i64) -> i64 {
    match (hour, ampm) {
        (h, 1) if h < 12 => h + 12,
        (12, 0) => 0,
        (h, _) => h,
    }
}

/// `_parsems`: "I[.F]" seconds as (seconds, microseconds).
fn parsems(value: &str) -> R<(i64, i64)> {
    let Some((i, f)) = value.split_once('.') else { return Ok((py_int(value)?, 0)) };
    if f.contains('.') {
        return Err(Stop::Invalid);
    }
    let f = format!("{f:0<6}");
    Ok((py_int(i)?, py_int(&f[..6])?))
}

/// `_parse_min_sec`.
fn min_sec(value: &Dec) -> (Option<i64>, Option<i64>) {
    (Some(value.int), value.has_frac().then(|| value.frac_times_60()))
}

fn find_hms_idx(idx: usize, l: &[String]) -> Option<usize> {
    let n = l.len();
    if idx + 1 < n && hms(&l[idx + 1]).is_some() {
        Some(idx + 1)
    } else if idx + 2 < n && l[idx + 1] == " " && hms(&l[idx + 2]).is_some() {
        Some(idx + 2)
    } else if idx > 0 && hms(&l[idx - 1]).is_some() {
        Some(idx - 1)
    } else if 1 < idx && idx == n - 1 && l[idx - 1] == " " && hms(&l[idx - 2]).is_some() {
        Some(idx - 2)
    } else {
        None
    }
}

fn assign_hms(res: &mut Res, value_repr: &str, hms: i64) -> R<()> {
    let value = Dec::parse(value_repr)?;
    match hms {
        0 => {
            res.hour = Some(value.int);
            if value.has_frac() {
                res.minute = Some(value.frac_times_60());
            }
        }
        1 => (res.minute, res.second) = min_sec(&value),
        2 => {
            let (s, us) = parsems(value_repr)?;
            (res.second, res.microsecond) = (Some(s), Some(us));
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn parse_numeric_token(l: &[String], mut idx: usize, ymd: &mut Ymd, res: &mut Res) -> R<usize> {
    let value_repr = l[idx].as_str();
    let value = Dec::parse(value_repr)?;
    let len_li = value_repr.len();
    let n = l.len();
    let s = value_repr;
    if ymd.len() == 3
        && (len_li == 2 || len_li == 4)
        && res.hour.is_none()
        && (idx + 1 >= n || (l[idx + 1] != ":" && hms(&l[idx + 1]).is_none()))
    {
        res.hour = Some(py_int(&s[..2])?);
        if len_li == 4 {
            res.minute = Some(py_int(&s[2..])?);
        }
    } else if len_li == 6 || (len_li > 6 && s.find('.') == Some(6)) {
        if ymd.len() == 0 && !s.contains('.') {
            for part in [&s[..2], &s[2..4], &s[4..]] {
                ymd.append(Val::Str(part), None)?;
            }
        } else {
            res.hour = Some(py_int(&s[..2])?);
            res.minute = Some(py_int(&s[2..4])?);
            let (sec, us) = parsems(&s[4..])?;
            (res.second, res.microsecond) = (Some(sec), Some(us));
        }
    } else if matches!(len_li, 8 | 12 | 14) {
        ymd.append(Val::Str(&s[..4]), Some('Y'))?;
        ymd.append(Val::Str(&s[4..6]), None)?;
        ymd.append(Val::Str(&s[6..8]), None)?;
        if len_li > 8 {
            res.hour = Some(py_int(&s[8..10])?);
            res.minute = Some(py_int(&s[10..12])?);
            if len_li > 12 {
                res.second = Some(py_int(&s[12..])?);
            }
        }
    } else if let Some(hms_idx) = find_hms_idx(idx, l) {
        let hms_value = if hms_idx > idx {
            let h = hms(&l[hms_idx]);
            idx = hms_idx;
            h
        } else {
            hms(&l[hms_idx]).map(|h| h + 1)
        };
        if let Some(h) = hms_value {
            assign_hms(res, value_repr, h)?;
        }
    } else if idx + 2 < n && l[idx + 1] == ":" {
        res.hour = Some(value.int);
        let minute = Dec::parse(&l[idx + 2])?;
        (res.minute, res.second) = min_sec(&minute);
        if idx + 4 < n && l[idx + 3] == ":" {
            let (sec, us) = parsems(&l[idx + 4])?;
            (res.second, res.microsecond) = (Some(sec), Some(us));
            idx += 2;
        }
        idx += 2;
    } else if idx + 1 < n && ["-", "/", "."].contains(&l[idx + 1].as_str()) {
        let sep = &l[idx + 1];
        ymd.append(Val::Str(value_repr), None)?;
        if idx + 2 < n && !jump(&l[idx + 2]) {
            if super::ymd::all_digits(&l[idx + 2]) && !l[idx + 2].is_empty() {
                ymd.append(Val::Str(&l[idx + 2]), None)?;
            } else {
                let m = month(&l[idx + 2]).ok_or(Stop::Invalid)?;
                ymd.append(Val::Int(m), Some('M'))?;
            }
            if idx + 3 < n && &l[idx + 3] == sep {
                let next = l.get(idx + 4).ok_or(Stop::Invalid)?;
                match month(next) {
                    Some(m) => ymd.append(Val::Int(m), Some('M'))?,
                    None => ymd.append(Val::Str(next), None)?,
                }
                idx += 2;
            }
            idx += 1;
        }
        idx += 1;
    } else if idx + 1 >= n || jump(&l[idx + 1]) {
        match l.get(idx + 2).and_then(|t| ampm(t)) {
            Some(a) => {
                res.hour = Some(adjust_ampm(value.int, a));
                idx += 1;
            }
            None => ymd.append(Val::Num(value), None)?,
        }
        idx += 1;
    } else if let Some(a) = ampm(&l[idx + 1]).filter(|_| value.int < 24) {
        res.hour = Some(adjust_ampm(value.int, a));
        idx += 1;
    } else if ymd.could_be_day(&value) {
        ymd.append(Val::Num(value), None)?;
    } else {
        return Err(Stop::Invalid);
    }
    Ok(idx)
}
