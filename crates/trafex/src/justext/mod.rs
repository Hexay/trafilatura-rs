//! Port of jusText 3.0.2 (reference/justext) as used by trafilatura/external.py.
mod classify;
mod maker;
mod paragraph;
pub mod sax;

pub use classify::{classify_paragraphs, get_next_neighbour, get_prev_neighbour, revise_paragraph_classification, Params};
pub use maker::make_paragraphs;
pub use paragraph::{is_blank, normalize_whitespace, Class, Paragraph, PathInfo};

use std::collections::HashSet;
use std::sync::LazyLock;
use crate::dom::{Node, Result};

/// A `define_stoplist` result: lower-cased words.
pub struct Stoplist(HashSet<String>);

/// `define_stoplist(JT_STOPLIST)`, exported by `harness/ref_justext.py stoplist`.
static JT_STOPLIST: LazyLock<Stoplist> =
    LazyLock::new(|| Stoplist(include_str!("../../data/stoplist.txt").lines().map(str::to_owned).collect()));

impl Stoplist {
    /// `define_stoplist(words)`.
    pub fn new<'a>(words: impl IntoIterator<Item = &'a str>) -> Stoplist {
        Stoplist(words.into_iter().map(str::to_lowercase).collect())
    }

    /// trafilatura's `JT_STOPLIST or jt_stoplist_init()`: the union of all jusText stoplists.
    pub fn jt() -> &'static Stoplist {
        &JT_STOPLIST
    }

    /// `word.lower() in stoplist`.
    pub fn contains_lower(&self, word: &str) -> bool {
        if word.chars().any(|c| c.is_uppercase() || !c.is_ascii()) {
            self.0.contains(&word.to_lowercase())
        } else {
            self.0.contains(word)
        }
    }
}

/// `custom_justext(tree, stoplist)` from trafilatura/external.py (`Paragraph::xpath` left empty).
pub fn custom_justext(tree: Node, stoplist: &Stoplist) -> Result<Vec<Paragraph>> {
    classify(make_paragraphs(tree)?, stoplist)
}

/// `custom_justext` with every Paragraph field filled, for parity dumps.
pub fn custom_justext_full(tree: Node, stoplist: &Stoplist) -> Result<Vec<Paragraph>> {
    classify(maker::make_paragraphs_full(tree)?, stoplist)
}

fn classify(mut paragraphs: Vec<Paragraph>, stoplist: &Stoplist) -> Result<Vec<Paragraph>> {
    let params = Params {
        length_low: 50,
        length_high: 200,
        stopwords_low: 0.1,
        stopwords_high: 0.2,
        max_link_density: 0.2,
        no_headings: true,
    };
    classify_paragraphs(&mut paragraphs, stoplist, &params);
    revise_paragraph_classification(&mut paragraphs, 200);
    Ok(paragraphs)
}
