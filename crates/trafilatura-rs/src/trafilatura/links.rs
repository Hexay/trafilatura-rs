//! include_links / include_images URL handling: courlan's `get_base_url` and `fix_relative_urls` and urllib's
//! `urljoin`, native on the inputs `py::urllib` covers and called back into Python (`Hooks`) otherwise.
use super::xml_unescape::unescape;
use crate::dom::Result;
use crate::hooks::{Hooks, NoHooks};
use crate::py::urllib::{urljoin, urlsplit, urlunsplit};

/// The URLs extraction resolves links and images against.
#[derive(Clone, Copy)]
pub struct Urls<'a> {
    pub hooks: &'a dyn Hooks,
    /// `options.url`.
    pub page: Option<&'a str>,
    /// `options.url or document.url`, which the main `convert_tags` call receives.
    pub convert: Option<&'a str>,
}

impl Default for Urls<'_> {
    fn default() -> Self {
        Urls { hooks: &NoHooks, page: None, convert: None }
    }
}

/// `courlan.get_base_url(url)`; `Err(Error::Value)` where it raises ValueError.
pub fn get_base_url(hooks: &dyn Hooks, url: &str) -> Result<String> {
    let unescaped = unescape(url);
    match urlsplit(&unescaped) {
        Some(s) if s.scheme.is_empty() => Ok(s.netloc.to_owned()),
        Some(s) => Ok(format!("{}://{}", s.scheme, s.netloc)),
        None => hooks.get_base_url(url),
    }
}

fn fix_natively(baseurl: &str, url: &str) -> Option<String> {
    let base_netloc = urlsplit(baseurl)?.netloc;
    let split = urlsplit(url)?;
    if !split.netloc.is_empty() && split.netloc != base_netloc {
        if !split.scheme.is_empty() {
            return Some(url.to_owned());
        }
        return Some(urlunsplit("http", split.netloc, split.path, split.query, split.fragment));
    }
    urljoin(baseurl, url)
}

/// `courlan.fix_relative_urls(baseurl, url)`; `Err(Error::Value)` where it raises ValueError.
pub fn fix_relative_urls(hooks: &dyn Hooks, baseurl: &str, url: &str) -> Result<String> {
    if url.starts_with('{') {
        return Ok(url.to_owned());
    }
    fix_natively(baseurl, url).map_or_else(|| hooks.fix_relative_urls(baseurl, url), Ok)
}

/// `urllib.parse.urljoin(base, url)`.
pub fn join(hooks: &dyn Hooks, base: &str, url: &str) -> Result<String> {
    urljoin(base, url).map_or_else(|| hooks.urljoin(base, url), Ok)
}
