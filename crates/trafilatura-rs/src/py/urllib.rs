//! `urllib.parse.urlsplit`/`urlunsplit`/`urljoin` on the inputs where CPython 3.10-3.14 agree. Each returns None
//! outside them (the caller asks Python): leading C0/space or tab/CR/LF (stripped since 3.11), brackets or a
//! non-ASCII netloc (validated, may raise), `;` in a path (3.14 stopped splitting params), an empty `?` or `#`
//! (kept since 3.14), a scheme not starting with an ASCII letter.

const USES_RELATIVE: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms", "prospero", "rtsp", "rtsps",
    "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];
const USES_NETLOC: &[&str] = &[
    "",
    "ftp",
    "http",
    "gopher",
    "nntp",
    "telnet",
    "imap",
    "wais",
    "file",
    "mms",
    "https",
    "shttp",
    "snews",
    "prospero",
    "rtsp",
    "rtsps",
    "rtspu",
    "rsync",
    "svn",
    "svn+ssh",
    "sftp",
    "nfs",
    "git",
    "git+ssh",
    "ws",
    "wss",
    "itms-services",
];

/// A `SplitResult`; query and fragment are None when their delimiter is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split<'a> {
    pub scheme: String,
    pub netloc: &'a str,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub fragment: Option<&'a str>,
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

/// `urlsplit(url)`.
pub fn urlsplit(url: &str) -> Option<Split<'_>> {
    if url.starts_with(|c: char| c <= ' ') || url.contains(['\t', '\r', '\n']) {
        return None;
    }
    let mut scheme = String::new();
    let mut rest = url;
    if let Some(i) = url.find(':').filter(|&i| i > 0) {
        if url[..i].chars().all(is_scheme_char) {
            if !url.starts_with(|c: char| c.is_ascii_alphabetic()) {
                return None;
            }
            scheme = url[..i].to_ascii_lowercase();
            rest = &url[i + 1..];
        }
    }
    let mut netloc = "";
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        netloc = &after[..end];
        rest = &after[end..];
        if netloc.contains(['[', ']']) || !netloc.is_ascii() {
            return None;
        }
    }
    let mut fragment = None;
    if let Some((r, f)) = rest.split_once('#') {
        rest = r;
        fragment = Some(f);
    }
    let mut query = None;
    if let Some((r, q)) = rest.split_once('?') {
        rest = r;
        query = Some(q);
    }
    Some(Split { scheme, netloc, path: rest, query, fragment })
}

/// `urlunsplit((scheme, netloc, path, query, fragment))`; empty query and fragment are dropped.
pub fn urlunsplit(scheme: &str, netloc: &str, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    let mut url = String::new();
    if !netloc.is_empty() {
        url.push_str("//");
        url.push_str(netloc);
        if !path.is_empty() && !path.starts_with('/') {
            url.push('/');
        }
    } else if path.starts_with("//")
        || (!scheme.is_empty() && USES_NETLOC.contains(&scheme) && (path.is_empty() || path.starts_with('/')))
    {
        url.push_str("//");
    }
    url.push_str(path);
    if !scheme.is_empty() {
        url = format!("{scheme}:{url}");
    }
    for (sep, part) in [('?', query), ('#', fragment)] {
        if let Some(p) = part.filter(|p| !p.is_empty()) {
            url.push(sep);
            url.push_str(p);
        }
    }
    url
}

fn has_empty_marker(s: &Split) -> bool {
    s.query == Some("") || s.fragment == Some("")
}

/// RFC 3986 dot-segment removal as `urljoin` does it.
fn resolve_path(base_path: &str, path: &str) -> String {
    let mut segments: Vec<&str> = if path.starts_with('/') {
        path.split('/').collect()
    } else {
        let mut base: Vec<&str> = base_path.split('/').collect();
        if base.last() != Some(&"") {
            base.pop();
        }
        let mut s: Vec<&str> = base.into_iter().chain(path.split('/')).collect();
        if s.len() > 2 {
            let last = s.pop().expect("len > 2");
            let mut kept: Vec<&str> = s.drain(1..).filter(|x| !x.is_empty()).collect();
            s.append(&mut kept);
            s.push(last);
        }
        s
    };
    let mut resolved: Vec<&str> = Vec::new();
    for &seg in &segments {
        match seg {
            ".." => {
                resolved.pop();
            }
            "." => {}
            _ => resolved.push(seg),
        }
    }
    if matches!(segments.pop(), Some("." | "..")) {
        resolved.push("");
    }
    let joined = resolved.join("/");
    if joined.is_empty() {
        "/".to_owned()
    } else {
        joined
    }
}

/// `urljoin(base, url)`.
pub fn urljoin(base: &str, url: &str) -> Option<String> {
    if base.is_empty() {
        return Some(url.to_owned());
    }
    if url.is_empty() {
        return Some(base.to_owned());
    }
    let b = urlsplit(base)?;
    let u = urlsplit(url)?;
    if b.scheme.is_empty() || has_empty_marker(&b) || has_empty_marker(&u) || b.path.contains(';') {
        return None;
    }
    let scheme = if u.scheme.is_empty() { b.scheme.as_str() } else { u.scheme.as_str() };
    if scheme != b.scheme || !USES_RELATIVE.contains(&scheme) {
        return Some(url.to_owned());
    }
    if u.path.contains(';') {
        return None;
    }
    if !u.netloc.is_empty() {
        return Some(urlunsplit(scheme, u.netloc, u.path, u.query, u.fragment));
    }
    if b.netloc.is_empty() {
        return None;
    }
    if u.path.is_empty() {
        if u.query.is_none() && u.fragment.is_none() && b.fragment.is_some() {
            return None;
        }
        let query = u.query.or(b.query);
        return Some(urlunsplit(scheme, b.netloc, b.path, query, u.fragment));
    }
    Some(urlunsplit(scheme, b.netloc, &resolve_path(b.path, u.path), u.query, u.fragment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3986_examples() {
        let base = "http://a/b/c/d?q";
        for (rel, want) in [
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d?q#s"),
            (".", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../..", "http://a/"),
            ("../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("g..", "http://a/b/c/g.."),
            ("./../g", "http://a/b/g"),
            ("g/../h", "http://a/b/c/h"),
            ("http:g", "http://a/b/c/g"),
            ("https://x/y", "https://x/y"),
            ("mailto:x@y", "mailto:x@y"),
        ] {
            assert_eq!(urljoin(base, rel).as_deref(), Some(want), "{rel}");
        }
        assert_eq!(urljoin("http://a", "b//c/./d").as_deref(), Some("http://a/b/c/d"));
        assert_eq!(urljoin(base, "g?"), None);
        assert_eq!(urljoin(base, "g;x"), None);
    }
}
