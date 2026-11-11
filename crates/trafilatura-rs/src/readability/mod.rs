//! Port of trafilatura 1.11.0's trafilatura/readability_lxml.py and `external.try_readability`; `v23` is
//! trafilatura 2.3.1's fork.
mod prep;
mod regexes;
mod sanitize;
mod scoring;
pub mod v23;

use crate::dom::{fragment_fromstring, Ctx, Node, Result};
use crate::py;
use scoring::{link_density, Candidates};

/// `readability_lxml.Document`; not re-enterable, `summary` mutates the tree.
pub struct Document {
    doc: Node,
    min_text_length: usize,
    retry_length: usize,
}

impl Document {
    pub fn new(doc: Node, min_text_length: usize, retry_length: usize) -> Document {
        Document { doc, min_text_length, retry_length }
    }

    /// `Document.summary()`: the cleaned article serialized as XML.
    pub fn summary(&mut self, ctx: &Ctx) -> Result<String> {
        for elem in self.doc.iter(&["script", "style"]) {
            elem.drop_tree(ctx)?;
        }
        let mut ruthless = true;
        loop {
            if ruthless {
                prep::remove_unlikely_candidates(ctx, self.doc)?;
            }
            prep::transform_misused_divs_into_paragraphs(ctx, self.doc)?;
            let cands = scoring::score_paragraphs(self.doc, self.min_text_length);
            let article = match cands.best() {
                Some(best) => get_article(ctx, &cands, best)?,
                None if ruthless => {
                    ruthless = false;
                    continue;
                }
                None => self.doc.find("body").unwrap_or(self.doc),
            };
            let cleaned = sanitize::sanitize(ctx, article, &cands, self.min_text_length)?;
            // sanitize rebinds self.doc, so a retry works on the extracted article
            self.doc = article;
            if ruthless && py::len(&cleaned) < self.retry_length {
                ruthless = false;
                continue;
            }
            return Ok(cleaned);
        }
    }
}

fn new_p(ctx: &Ctx, text: &str) -> Result<Node> {
    let p = fragment_fromstring(ctx, "<p/>")?;
    p.set_text(Some(text))?;
    Ok(p)
}

fn get_article(ctx: &Ctx, cands: &Candidates, (best, best_score): (Node, f64)) -> Result<Node> {
    let scaled = best_score * 0.2;
    let threshold = if scaled > 10.0 { scaled } else { 10.0 };
    let output = fragment_fromstring(ctx, "<div/>")?;
    let siblings = match best.parent() {
        Some(parent) => parent.children(),
        None => vec![best],
    };
    for sibling in siblings {
        let append = if sibling == best || cands.get(sibling).is_some_and(|s| s >= threshold) {
            true
        } else if sibling.tag() == "p" {
            let density = link_density(sibling);
            let content = sibling.text().unwrap_or_default();
            let len = py::len(&content);
            (len > 80 && density < 0.25) || (len <= 80 && density == 0.0 && regexes::dot_space(&content))
        } else {
            false
        };
        if append {
            output.append(sibling)?;
        }
    }
    Ok(output)
}

/// `external.try_readability`: `fromstring_bytes(summary)` (None when that parse fails), `Element('div')` when
/// readability raises.
pub fn try_readability(ctx: &Ctx, tree: Node) -> Option<Node> {
    match Document::new(tree, 25, 250).summary(ctx) {
        Ok(summary) => crate::trafilatura::utils::fromstring_bytes(ctx, &summary),
        Err(_) => Some(ctx.element("div").expect("valid tag")),
    }
}
