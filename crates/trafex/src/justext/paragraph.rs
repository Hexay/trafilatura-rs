//! justext/paragraph.py, justext/utils.py text helpers and core.PathInfo.
use crate::py;
use std::collections::HashMap;

/// `class_type` / `cf_class` values; `Unset` is Python's initial `""`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Class {
    Unset,
    Short,
    NearGood,
    Good,
    Bad,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Unset => "",
            Class::Short => "short",
            Class::NearGood => "neargood",
            Class::Good => "good",
            Class::Bad => "bad",
        }
    }
}

/// `normalize_whitespace`: each run of Python whitespace becomes "\n" if it holds "\n" or "\r", else " ".
pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run: Option<bool> = None;
    for c in text.chars() {
        if py::is_space(c) {
            let newline = c == '\n' || c == '\r';
            run = Some(run.unwrap_or(false) || newline);
            continue;
        }
        if let Some(nl) = run.take() {
            out.push(if nl { '\n' } else { ' ' });
        }
        out.push(c);
    }
    if let Some(nl) = run {
        out.push(if nl { '\n' } else { ' ' });
    }
    out
}

pub fn is_blank(s: &str) -> bool {
    s.chars().all(py::is_space)
}

#[derive(Clone, Debug)]
pub struct Paragraph {
    pub dom_path: String,
    pub xpath: String,
    pub text_nodes: Vec<String>,
    pub chars_count_in_links: usize,
    pub tags_count: isize,
    pub class_type: Class,
    pub cf_class: Class,
    pub heading: bool,
    /// The `text` property, fixed once the paragraph is complete (see `finish`).
    pub text: String,
    /// `len(paragraph)`: code points of `text`.
    pub length: usize,
}

impl Paragraph {
    pub fn new(path: &PathInfo) -> Paragraph {
        Paragraph {
            dom_path: path.dom(),
            xpath: if path.record_xpath { path.xpath() } else { String::new() },
            text_nodes: Vec::new(),
            chars_count_in_links: 0,
            tags_count: 0,
            class_type: Class::Unset,
            cf_class: Class::Unset,
            heading: false,
            text: String::new(),
            length: 0,
        }
    }

    pub(crate) fn finish(&mut self) {
        self.text = normalize_whitespace(py::strip(&self.text_nodes.concat()));
        self.length = py::len(&self.text);
    }

    pub fn is_boilerplate(&self) -> bool {
        self.class_type != Class::Good
    }

    /// `HEADINGS_PATTERN = \bh\d\b` searched in `dom_path`.
    pub fn is_heading(&self) -> bool {
        static H: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"h\d").unwrap());
        let s = &self.dom_path;
        H.find_iter(s).any(|m| {
            let before = s[..m.start()].chars().next_back();
            let after = s[m.end()..].chars().next();
            !before.is_some_and(py::is_word) && !after.is_some_and(py::is_word)
        })
    }

    pub fn contains_text(&self) -> bool {
        !self.text_nodes.is_empty()
    }

    pub fn append_text(&mut self, text: &str) -> usize {
        let t = normalize_whitespace(text);
        let n = py::len(&t);
        self.text_nodes.push(t);
        n
    }

    pub fn words_count(&self) -> usize {
        py::split(&self.text).count()
    }

    pub fn stopwords_count(&self, stoplist: &super::Stoplist) -> usize {
        py::split(&self.text).filter(|w| stoplist.contains_lower(w)).count()
    }

    pub fn stopwords_density(&self, stoplist: &super::Stoplist) -> f64 {
        let words = self.words_count();
        if words == 0 {
            return 0.0;
        }
        self.stopwords_count(stoplist) as f64 / words as f64
    }

    pub fn links_density(&self) -> f64 {
        if self.length == 0 {
            return 0.0;
        }
        self.chars_count_in_links as f64 / self.length as f64
    }
}

/// core.PathInfo: the open-element stack with per-parent sibling counters for `xpath`. Extraction never reads
/// `xpath`, so the counters and the string are only kept when `record_xpath` is set (parity dumps).
#[derive(Default)]
pub struct PathInfo {
    elements: Vec<(String, usize, HashMap<String, usize>)>,
    pub record_xpath: bool,
}

impl PathInfo {
    pub fn dom(&self) -> String {
        self.elements.iter().map(|e| e.0.as_str()).collect::<Vec<_>>().join(".")
    }

    pub fn xpath(&self) -> String {
        let parts: Vec<String> = self.elements.iter().map(|e| format!("{}[{}]", e.0, e.1)).collect();
        format!("/{}", parts.join("/"))
    }

    pub fn append(&mut self, tag: &str) {
        if !self.record_xpath {
            self.elements.push((tag.to_owned(), 0, HashMap::new()));
            return;
        }
        let order = match self.elements.last_mut() {
            Some(parent) => {
                let n = parent.2.entry(tag.to_owned()).or_insert(0);
                *n += 1;
                *n
            }
            None => 1,
        };
        self.elements.push((tag.to_owned(), order, HashMap::new()));
    }

    pub fn pop(&mut self) {
        self.elements.pop();
    }
}
