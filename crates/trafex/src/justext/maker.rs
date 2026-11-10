//! core.ParagraphMaker: SAX handler that cuts the DOM into paragraphs.
use super::paragraph::{is_blank, Paragraph, PathInfo};
use super::sax::{saxify, ContentHandler};
use crate::dom::{Node, Result};

const PARAGRAPH_TAGS: &[&str] = &[
    "body", "blockquote", "caption", "center", "col", "colgroup", "dd", "div", "dl", "dt", "fieldset", "form",
    "legend", "optgroup", "option", "p", "pre", "table", "td", "textarea", "tfoot", "th", "thead", "tr", "ul", "li",
    "h1", "h2", "h3", "h4", "h5", "h6",
];

struct ParagraphMaker {
    path: PathInfo,
    paragraphs: Vec<Paragraph>,
    paragraph: Paragraph,
    link: bool,
    br: bool,
}

impl ParagraphMaker {
    fn start_new_paragraph(&mut self) {
        let next = Paragraph::new(&self.path);
        let mut done = std::mem::replace(&mut self.paragraph, next);
        // Python tests `self.paragraph and ...`: Paragraph defines __len__, so empty-text paragraphs are dropped.
        if done.contains_text() {
            done.finish();
            if done.length > 0 {
                self.paragraphs.push(done);
            }
        }
    }
}

impl ContentHandler for ParagraphMaker {
    fn start_element(&mut self, name: &str) {
        self.path.append(name);
        if PARAGRAPH_TAGS.contains(&name) || (name == "br" && self.br) {
            if name == "br" {
                // <br><br> separates paragraphs and does not count as a tag of either
                self.paragraph.tags_count -= 1;
            }
            self.start_new_paragraph();
        } else {
            self.br = name == "br";
            if self.br {
                self.paragraph.append_text(" ");
            } else if name == "a" {
                self.link = true;
            }
            self.paragraph.tags_count += 1;
        }
    }

    fn end_element(&mut self, name: &str) {
        self.path.pop();
        if PARAGRAPH_TAGS.contains(&name) {
            self.start_new_paragraph();
        }
        if name == "a" {
            self.link = false;
        }
    }

    fn end_document(&mut self) {
        self.start_new_paragraph();
    }

    fn characters(&mut self, content: &str) {
        if is_blank(content) {
            return;
        }
        let n = self.paragraph.append_text(content);
        if self.link {
            self.paragraph.chars_count_in_links += n;
        }
        self.br = false;
    }
}

/// `ParagraphMaker.make_paragraphs(root)` without `Paragraph::xpath` (nothing in extraction reads it).
pub fn make_paragraphs(root: Node) -> Result<Vec<Paragraph>> {
    paragraphs(root, false)
}

/// `make_paragraphs` with `Paragraph::xpath` filled, for parity dumps.
pub fn make_paragraphs_full(root: Node) -> Result<Vec<Paragraph>> {
    paragraphs(root, true)
}

fn paragraphs(root: Node, record_xpath: bool) -> Result<Vec<Paragraph>> {
    let mut path = PathInfo::default();
    path.record_xpath = record_xpath;
    let paragraph = Paragraph::new(&path);
    let mut maker = ParagraphMaker { path, paragraphs: Vec::new(), paragraph, link: false, br: false };
    saxify(root, &mut maker)?;
    Ok(maker.paragraphs)
}
