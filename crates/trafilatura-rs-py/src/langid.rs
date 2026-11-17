//! The `target_language` leaves behind `PyHooks`: py3langid's classify natively (`trafilatura_rs::langid`) while
//! `trafilatura_rs._langid.model()` vouches for the installed identifier, and the jusText stoplists.
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use trafilatura_rs::justext::Stoplist;
use trafilatura_rs::langid::Model;

/// The identifier arrays a model was read from (identity-checked on every call) and the model, None when unsound.
struct Cached {
    sources: Vec<Py<PyAny>>,
    model: Option<Arc<Model>>,
}

static MODEL: Mutex<Option<Cached>> = Mutex::new(None);

fn bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(obj.call_method0("tobytes")?.cast_into::<PyBytes>()?.as_bytes().to_vec())
}

/// An `array.array` of unsigned ints (py3langid's `_to_array`) as u32s; None when one doesn't fit.
fn uints(obj: &Bound<'_, PyAny>) -> PyResult<Option<Vec<u32>>> {
    let raw = bytes(obj)?;
    Ok(match obj.getattr("itemsize")?.extract::<usize>()? {
        2 => Some(raw.as_chunks::<2>().0.iter().map(|c| u32::from(u16::from_ne_bytes(*c))).collect()),
        4 => Some(raw.as_chunks::<4>().0.iter().map(|c| u32::from_ne_bytes(*c)).collect()),
        8 => raw.as_chunks::<8>().0.iter().map(|c| u32::try_from(u64::from_ne_bytes(*c)).ok()).collect(),
        _ => None,
    })
}

fn read_model(arrays: &[Bound<'_, PyAny>]) -> PyResult<Option<Model>> {
    let [ptc, pc, classes, nextmove, row, output] = arrays else { return Ok(None) };
    let ptc = bytes(ptc)?.as_chunks::<2>().0.iter().map(|c| u16::from_ne_bytes(*c)).collect();
    let pc = bytes(pc)?.as_chunks::<4>().0.iter().map(|c| f32::from_ne_bytes(*c)).collect();
    let (Some(nextmove), Some(row)) = (uints(nextmove)?, uints(row)?) else { return Ok(None) };
    Ok(Model::new(ptc, pc, classes.extract()?, nextmove, row, output.extract()?))
}

fn model(py: Python<'_>) -> PyResult<Option<Arc<Model>>> {
    let arrays = py.import("trafilatura_rs._langid")?.call_method0("model")?;
    if arrays.is_none() {
        return Ok(None);
    }
    let arrays: Vec<Bound<'_, PyAny>> = arrays.extract()?;
    let same = |c: &Cached| c.sources.iter().zip(&arrays).all(|(a, b)| a.bind(py).is(b));
    if let Some(c) = MODEL.lock().expect("langid lock").as_ref().filter(|c| same(c)) {
        return Ok(c.model.clone());
    }
    // not under the lock: reading runs Python
    let model = read_model(&arrays)?.map(Arc::new);
    let sources = arrays.iter().map(|a| a.clone().unbind()).collect();
    *MODEL.lock().expect("langid lock") = Some(Cached { sources, model: model.clone() });
    Ok(model)
}

/// `LanguageIdentifier._encode(text)`: ASCII natively, else in Python.
fn encode(py: Python<'_>, text: &str) -> PyResult<Vec<u8>> {
    if text.is_ascii() {
        let upper = text.bytes().any(|b| b.is_ascii_uppercase()) && !text.bytes().any(|b| b.is_ascii_lowercase());
        return Ok(if upper { text.to_ascii_lowercase() } else { text.to_owned() }.into_bytes());
    }
    let ident = py.import("py3langid.langid")?.getattr("LanguageIdentifier")?;
    Ok(ident.call_method1("_encode", (text,))?.cast_into::<PyBytes>()?.as_bytes().to_vec())
}

/// `py3langid.classify(text)[0]` natively, or None to run Python.
pub(crate) fn classify(py: Python<'_>, text: &str) -> Option<String> {
    let native = || -> PyResult<Option<String>> {
        let Some(model) = model(py)? else { return Ok(None) };
        let text = encode(py, text)?;
        Ok(model.classify(&text).map(str::to_owned))
    };
    native().ok().flatten()
}

/// Stoplists by `JUSTEXT_LANGUAGES` name, for the `get_stoplist` function they were read with.
type Stoplists = Option<(Py<PyAny>, HashMap<String, Arc<Stoplist>>)>;
static STOPLISTS: Mutex<Stoplists> = Mutex::new(None);

/// `define_stoplist(get_stoplist(JUSTEXT_LANGUAGES[lang]))` for a `lang` in JUSTEXT_LANGUAGES, else None.
pub(crate) fn justext_stoplist(py: Python<'_>, lang: &str) -> PyResult<Option<Arc<Stoplist>>> {
    let external = py.import("trafilatura.external")?;
    let languages = external.getattr("JUSTEXT_LANGUAGES")?;
    if !languages.contains(lang)? {
        return Ok(None);
    }
    let name: String = languages.get_item(lang)?.extract()?;
    let get_stoplist = external.getattr("get_stoplist")?;
    let cached = |cache: &Stoplists| {
        cache.as_ref().filter(|(f, _)| f.bind(py).is(&get_stoplist)).and_then(|(_, lists)| lists.get(&name).cloned())
    };
    if let Some(list) = cached(&STOPLISTS.lock().expect("stoplist lock")) {
        return Ok(Some(list));
    }
    // not under the lock: reading runs Python
    let words = get_stoplist.call1((&name,))?;
    let words: HashSet<String> = py.import("justext.core")?.getattr("define_stoplist")?.call1((words,))?.extract()?;
    let list = Arc::new(Stoplist::from_lowercased(words));
    let mut cache = STOPLISTS.lock().expect("stoplist lock");
    if !cache.as_ref().is_some_and(|(f, _)| f.bind(py).is(&get_stoplist)) {
        *cache = Some((get_stoplist.clone().unbind(), HashMap::new()));
    }
    cache.as_mut().expect("just set").1.insert(name, list.clone());
    Ok(Some(list))
}

/// Differential tests: the native `py3langid.classify(text)[0]`, or "defer".
#[pyfunction]
pub(crate) fn _native_language(py: Python<'_>, text: &str) -> String {
    classify(py, text).unwrap_or_else(|| "defer".into())
}
