//! `bare_extraction`'s per-call filters, identical in 1.11 and 2.3.1: the URL blacklist and `only_with_metadata`
//! checks after metadata, `check_authors`, and the `prune_xpath` pass.
use super::document::Document;
use super::settings::Options;
use crate::dom::{Error, Node, Result};
use crate::py;

/// The checks right after `extract_metadata`: Python raises ValueError (the call returns None).
pub fn check_metadata(doc: &Document, opts: &Options) -> Result<()> {
    if doc.url.as_str().is_some_and(|url| opts.url_blacklist.iter().any(|b| b == url)) {
        return Err(Error::Value("blacklisted URL".into()));
    }
    if opts.only_with_metadata && !(doc.date.truthy() && doc.title.truthy() && doc.url.truthy()) {
        return Err(Error::Value("no metadata".into()));
    }
    Ok(())
}

/// `str.lower()`, where Rust's mapping is known to agree: Final_Sigma and cased letters outside long-stable blocks
/// (case pairs keep being added after Unicode 15.1, Python 3.13's) are left to Python.
fn py_lower(s: &str) -> Result<String> {
    let old_block = |c: u32| c < 0x1c80 || (0x1e00..0x2000).contains(&c) || (0xff21..0xff3b).contains(&c);
    let stable = |c: char| c != '\u{3a3}' && (old_block(c as u32) || c.to_lowercase().eq(std::iter::once(c)));
    if s.chars().all(stable) {
        Ok(s.to_lowercase())
    } else {
        Err(Error::Unsupported("str.lower() outside the stable range".into()))
    }
}

/// `check_authors(authors, author_blacklist)` with an already-lowercased blacklist; both versions call it only on
/// a non-empty author, and for "" it would return "".
pub fn check_authors(authors: &str, blacklist: &[String]) -> Result<Option<String>> {
    let mut kept = Vec::new();
    for author in authors.split(';') {
        let author = py::strip(author);
        let lower = py_lower(author)?;
        if !blacklist.contains(&lower) {
            kept.push(author);
        }
    }
    Ok((!kept.is_empty()).then(|| kept.join("; ").trim_matches([';', ' ']).to_owned()))
}

/// `prune_unwanted_nodes(tree, [XPath(x) for x in prune_xpath])` with the version's node removal. Expressions whose
/// results Python would not simply remove (errors, strings, attributes, the root) are left to Python.
pub fn prune_xpath(tree: Node, opts: &Options, mut remove: impl FnMut(Node) -> Result<()>) -> Result<()> {
    for expr in &opts.prune_xpath {
        let nodes = tree
            .xpath_user_elements(expr)
            .ok_or_else(|| Error::Unsupported(format!("prune_xpath {expr:?} selects more than elements")))?;
        for node in nodes {
            remove(node)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_authors;

    #[test]
    fn check_authors_matches_python() {
        let bl = ["admin".to_owned(), "մեսսի".to_owned()];
        assert_eq!(check_authors(" Admin ;Jane Doe; ", &bl).unwrap().as_deref(), Some("Jane Doe"));
        assert_eq!(check_authors("ADMIN", &bl).unwrap(), None);
        assert_eq!(check_authors("Ավելացրեց; Մեսսի", &bl).unwrap().as_deref(), Some("Ավելացրեց"));
        assert!(check_authors("ΟΔΟΣ", &bl).is_err());
    }
}
