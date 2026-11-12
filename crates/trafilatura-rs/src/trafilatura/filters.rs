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

/// `str.lower()`, where Rust's mapping is known to agree: cased letters outside long-stable blocks (case pairs keep
/// being added after Unicode 15.1, Python 3.13's) are left to Python. Both apply the same Final_Sigma rule to `Σ`, so
/// it is ported only where its context's Cased/Case_Ignorable properties cannot differ between Unicode versions.
fn py_lower(s: &str) -> Result<String> {
    let old_block = |c: u32| c < 0x1c80 || (0x1e00..0x2000).contains(&c) || (0xff21..0xff3b).contains(&c);
    let stable = |c: char| old_block(c as u32) || c.to_lowercase().eq(std::iter::once(c));
    let sigmas_known = s
        .match_indices('\u{3a3}')
        .all(|(i, _)| sigma_side(s[..i].chars().rev()).is_some() && sigma_side(s[i + 2..].chars()).is_some());
    if s.chars().all(stable) && sigmas_known {
        Ok(s.to_lowercase())
    } else {
        Err(Error::Unsupported("str.lower() outside the stable range".into()))
    }
}

/// Final_Sigma's scan away from a `Σ`: skip Case_Ignorable, then whether a Cased letter follows. None when a character
/// on the way is not plainly one or the other.
fn sigma_side(mut chars: impl Iterator<Item = char>) -> Option<bool> {
    let cased_letter = |c: char| {
        let c32 = c as u32;
        let block = c.is_ascii()
            || (0xc0..0x250).contains(&c32)
            || (0x386..0x530).contains(&c32)
            || (0x1f00..0x2000).contains(&c32);
        block && c.is_alphabetic() && (c.is_lowercase() || c.is_uppercase())
    };
    loop {
        match chars.next() {
            None => return Some(false),
            Some('\'' | '.' | ':' | '^' | '`') => continue,
            Some(c) if cased_letter(c) => return Some(true),
            Some(c) if c.is_ascii() || c.is_whitespace() => return Some(false),
            Some(_) => return None,
        }
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
    use super::{check_authors, py_lower};

    #[test]
    fn check_authors_matches_python() {
        let bl = ["admin".to_owned(), "մեսսի".to_owned(), "οδος".to_owned()];
        assert_eq!(check_authors(" Admin ;Jane Doe; ", &bl).unwrap().as_deref(), Some("Jane Doe"));
        assert_eq!(check_authors("ADMIN", &bl).unwrap(), None);
        assert_eq!(check_authors("Ավելացրեց; Մեսսի", &bl).unwrap().as_deref(), Some("Ավելացրեց"));
        assert_eq!(check_authors("ΟΔΟΣ; ΑΝΔΡΕΑΣ", &bl).unwrap().as_deref(), Some("ΑΝΔΡΕΑΣ"));
    }

    #[test]
    fn final_sigma_matches_python() {
        let cases = [("ΑΝΕΣΤΗ ΑΛΕΞΑΝΔΡΑ", "ανεστη αλεξανδρα"), ("Σ", "σ"), ("ΑΣ.Β", "ασ.β"), ("ΑΣ ΚΑΙ", "ας και")];
        for (s, lower) in cases {
            assert_eq!(py_lower(s).unwrap(), lower);
        }
        assert!(py_lower("ΑΣ\u{301}").is_err());
    }
}
