//! Python `str.title()` and the per-character case predicates it uses.
use super::case_tables::{TITLECASE_LETTERS, TITLE_SPECIAL};

/// `_PyUnicode_IsCased`: Lowercase, Uppercase or Lt.
pub fn is_cased(c: char) -> bool {
    c.is_lowercase() || c.is_uppercase() || TITLECASE_LETTERS.contains(&c)
}

/// `str.title()`, or None for strings with a capital sigma, whose lowercase depends on context (Final_Sigma).
pub fn title(s: &str) -> Option<String> {
    if s.contains('\u{3a3}') {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    let mut previous_is_cased = false;
    for c in s.chars() {
        if previous_is_cased {
            out.extend(c.to_lowercase());
        } else {
            match TITLE_SPECIAL.binary_search_by_key(&c, |&(k, _)| k) {
                Ok(i) => out.push_str(TITLE_SPECIAL[i].1),
                Err(_) => out.extend(c.to_uppercase()),
            }
        }
        previous_is_cased = is_cased(c);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::title;

    #[test]
    fn title_matches_python() {
        assert_eq!(title("hello wORLD o'neil").as_deref(), Some("Hello World O'Neil"));
        assert_eq!(title("ǆemal ßtraße ﬁsh 3rd").as_deref(), Some("ǅemal Sstraße Fish 3Rd"));
    }
}
