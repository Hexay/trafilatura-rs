//! Per-process memo of htmldate's `_external_date(s)` (dateparser). With htmldate's parser (STRICT_PARSING,
//! PARSERS custom-formats + absolute-time, try_previous_locales off) a result depends on the string and the wall
//! clock only, and on the clock only through a two-digit year flipping a century when `now` passes the date it would
//! read as (`_parser._correct_for_time_frame`). So entries live for one local day and results near that flip are not
//! kept; the memo also starts over when `_external_date` or the parser object is replaced (e.g. patched in tests).
use crate::hooks::to_datetime;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;
use trafilatura_rs::py::datetime::DateTime;

const CAPACITY: usize = 1 << 16;
/// Days around today (and today a century on) covering the parse time zone's offset and DST.
const MARGIN: i64 = 3;
const CENTURY_DAYS: [i64; 2] = [36524, 36525];

struct Memo {
    function: Py<PyAny>,
    parser: Py<PyAny>,
    day: i64,
    results: HashMap<String, Option<DateTime>>,
}

static MEMO: Mutex<Option<Memo>> = Mutex::new(None);

fn stable(result: &Bound<'_, PyAny>, today: i64) -> PyResult<bool> {
    if result.is_none() {
        return Ok(true);
    }
    let day: i64 = result.call_method0("toordinal")?.extract()?;
    Ok((day - today).abs() > MARGIN && CENTURY_DAYS.iter().all(|c| (day + c - today).abs() > MARGIN))
}

/// `function(s)` (htmldate's `_external_date`), memoized.
pub(crate) fn external_date(py: Python<'_>, function: &Bound<'_, PyAny>, s: &str) -> PyResult<Option<DateTime>> {
    let today: i64 =
        py.import("datetime")?.getattr("date")?.call_method0("today")?.call_method0("toordinal")?.extract()?;
    let parser = py.import("htmldate.extractors")?.getattr("get_external_parser")?.call0()?;
    let current = |m: &Memo| m.day == today && m.function.bind(py).is(function) && m.parser.bind(py).is(&parser);
    if let Some(m) = MEMO.lock().expect("memo lock").as_ref().filter(|m| current(m)) {
        if let Some(hit) = m.results.get(s) {
            return Ok(*hit);
        }
    }
    let result = function.call1((s,))?;
    let date = to_datetime(&result)?;
    if stable(&result, today)? {
        let mut memo = MEMO.lock().expect("memo lock");
        if !memo.as_ref().is_some_and(|m| current(m) && m.results.len() < CAPACITY) {
            *memo = Some(Memo {
                function: function.clone().unbind(),
                parser: parser.unbind(),
                day: today,
                results: HashMap::new(),
            });
        }
        memo.as_mut().expect("just set").results.insert(s.to_owned(), date);
    }
    Ok(date)
}
