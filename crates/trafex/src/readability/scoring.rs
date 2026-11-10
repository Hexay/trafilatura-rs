//! Candidate scoring: `score_paragraphs`, `class_weight`, `score_node`, `get_link_density`, `select_best_candidate`.
use super::regexes::{NEGATIVE, POSITIVE};
use std::collections::HashMap;
use crate::dom::Node;
use crate::py;

/// The `candidates` dict: element identity -> score, in insertion order.
#[derive(Default)]
pub struct Candidates {
    order: Vec<(Node, f64)>,
    index: HashMap<Node, usize>,
}

impl Candidates {
    pub fn get(&self, n: Node) -> Option<f64> {
        self.index.get(&n).map(|&i| self.order[i].1)
    }

    fn add(&mut self, n: Node, delta: f64) {
        self.order[self.index[&n]].1 += delta;
    }

    fn insert_new(&mut self, n: Node, score: f64) {
        self.index.insert(n, self.order.len());
        self.order.push((n, score));
    }

    /// `sorted(values, key=score, reverse=True)[0]`: the stable sort keeps the first maximal entry.
    pub fn best(&self) -> Option<(Node, f64)> {
        let mut best: Option<(Node, f64)> = None;
        for &(n, s) in &self.order {
            if best.is_none_or(|(_, b)| s > b) {
                best = Some((n, s));
            }
        }
        best
    }
}

/// `text_length`.
pub fn text_length(n: Node) -> usize {
    n.text_content_trim_len()
}

pub fn link_density(n: Node) -> f64 {
    let total = match text_length(n) {
        0 => 1,
        t => t,
    };
    let links: usize = n.findall(".//a").into_iter().map(text_length).sum();
    links as f64 / total as f64
}

pub fn class_weight(n: Node) -> i64 {
    let mut weight = 0;
    for attr in [n.get("class"), n.get("id")].into_iter().flatten().filter(|a| !a.is_empty()) {
        if NEGATIVE.is_match(&attr) {
            weight -= 25;
        }
        if POSITIVE.is_match(&attr) {
            weight += 25;
        }
    }
    weight
}

fn score_node(n: Node) -> f64 {
    let mut score = class_weight(n);
    score += match n.tag().to_lowercase().as_str() {
        "div" | "article" => 5,
        "pre" | "td" | "blockquote" => 3,
        "address" | "ol" | "ul" | "dl" | "dd" | "dt" | "li" | "form" | "aside" => -3,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "th" | "header" | "footer" | "nav" => -5,
        _ => 0,
    };
    score as f64
}

pub fn score_paragraphs(doc: Node, min_text_length: usize) -> Candidates {
    let mut cands = Candidates::default();
    for elem in doc.iter(&["p", "pre", "td"]) {
        let Some(parent) = elem.parent() else { continue };
        let grand_parent = parent.parent();
        let text = elem.text_content_trimmed();
        let len = py::len(&text);
        if len < min_text_length {
            continue;
        }
        for node in [Some(parent), grand_parent].into_iter().flatten() {
            if cands.get(node).is_none() {
                cands.insert_new(node, score_node(node));
            }
        }
        let commas = text.matches(',').count() + 1;
        let score = (1 + commas) as f64 + (len as f64 / 100.0).min(3.0);
        cands.add(parent, score);
        if let Some(gp) = grand_parent {
            cands.add(gp, score / 2.0);
        }
    }
    for entry in &mut cands.order {
        entry.1 *= 1.0 - link_density(entry.0);
    }
    cands
}
