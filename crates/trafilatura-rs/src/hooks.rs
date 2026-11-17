//! Leaves of metadata extraction that run in the embedding Python process (see docs/metadata-formats-scope.md):
//! dateparser and dateutil (large, locale- and clock-dependent) and courlan's URL helpers (per-document string
//! functions, exact by construction when called); for `target_language`, py3langid and jusText's per-language
//! stoplists. Without a provider each returns `Error::Unsupported`.
use crate::dom::{Error, Result};
use crate::justext::Stoplist;
use crate::py::datetime::DateTime;
use std::sync::Arc;

fn unsupported<T>(what: &str) -> Result<T> {
    Err(Error::Unsupported(what.to_owned()))
}

/// Results are `Err(Error::Value)` where the Python function raises ValueError (not caught on the way), and
/// `Err(Error::Unsupported)` for any other exception.
pub trait Hooks: Sync {
    /// `htmldate.extractors.dateutil_parse(s, fuzzy=False)`; `Ok(None)` where htmldate catches the exception
    /// (OverflowError, TypeError, ValueError).
    fn dateutil_parse(&self, s: &str) -> Result<Option<DateTime>> {
        let _ = s;
        unsupported("dateutil")
    }

    /// `_external_date(s) if has_date_cue(s, datetime(min_year, 1, 1), datetime(max_year, 1, 1)) else None`
    /// (htmldate.extractors; dateparser and its month names for every locale).
    fn external_date(&self, s: &str, min_year: i32, max_year: i32) -> Result<Option<DateTime>> {
        let _ = (s, min_year, max_year);
        unsupported("dateparser")
    }

    /// `courlan.is_valid_url(url)`.
    fn is_valid_url(&self, url: &str) -> Result<bool> {
        let _ = url;
        unsupported("courlan")
    }

    /// `courlan.get_base_url(url)`.
    fn get_base_url(&self, url: &str) -> Result<String> {
        let _ = url;
        unsupported("courlan")
    }

    /// trafilatura 2.3.1 `utils.safe_base_url(url)`: `courlan.get_base_url`, "" where it raises ValueError.
    fn safe_base_url(&self, url: &str) -> Result<String> {
        let _ = url;
        unsupported("courlan")
    }

    /// `validate_url(url)` then `normalize_url(parsed)` as in trafilatura's `extract_url`; None when invalid.
    fn validated_url(&self, url: &str) -> Result<Option<String>> {
        let _ = url;
        unsupported("courlan")
    }

    /// `courlan.extract_domain(url, fast=True)`.
    fn extract_domain(&self, url: &str) -> Result<Option<String>> {
        let _ = url;
        unsupported("courlan")
    }

    /// `courlan.fix_relative_urls(baseurl, url)`, for links `py::urllib` leaves to Python.
    fn fix_relative_urls(&self, baseurl: &str, url: &str) -> Result<String> {
        let _ = (baseurl, url);
        unsupported("courlan")
    }

    /// `urllib.parse.urljoin(base, url)`, for URLs `py::urllib` leaves to Python.
    fn urljoin(&self, base: &str, url: &str) -> Result<String> {
        let _ = (base, url);
        unsupported("urljoin")
    }

    /// trafilatura's `utils.language_classifier(temp_text, temp_comments)`: py3langid's label for the longer of the
    /// two, None when no detector is installed.
    fn language_classifier(&self, text: &str, comments: &str) -> Result<Option<String>> {
        let _ = (text, comments);
        unsupported("py3langid")
    }

    /// `get_stoplist(JUSTEXT_LANGUAGES[lang])` (jusText) for a `lang` in JUSTEXT_LANGUAGES, else None.
    fn justext_stoplist(&self, lang: &str) -> Result<Option<Arc<Stoplist>>> {
        let _ = lang;
        unsupported("justext stoplist")
    }
}

/// No Python available: every leaf is unsupported.
pub struct NoHooks;

impl Hooks for NoHooks {}
