//! `Document.sanitize`: prune headers, forms, non-video iframes and low-quality tables/lists/divs.
use super::scoring::{class_weight, link_density, text_length, Candidates};
use super::regexes::VIDEO;
use std::collections::HashSet;
use crate::dom::{Ctx, Method, Node, Result};

const CONDITIONAL: &str = "//table|//ul|//div|//aside|//header|//footer|//section";

pub fn sanitize(ctx: &Ctx, node: Node, cands: &Candidates, min_text_length: usize) -> Result<String> {
    for header in node.iter(&["h1", "h2", "h3", "h4", "h5", "h6"]) {
        if class_weight(header) < 0 || link_density(header) > 0.33 {
            header.drop_tree(ctx)?;
        }
    }
    for elem in node.iter(&["form", "textarea"]) {
        elem.drop_tree(ctx)?;
    }
    for elem in node.iter(&["iframe"]) {
        if elem.has_attr("src") && VIDEO.is_match(&elem.get("src").unwrap_or_default()) {
            elem.set_text(Some("VIDEO"))?;
        } else {
            elem.drop_tree(ctx)?;
        }
    }
    let mut allowed = HashSet::new();
    for elem in node.xpath_nodes(CONDITIONAL)?.into_iter().rev() {
        if allowed.contains(&elem) {
            continue;
        }
        let weight = class_weight(elem);
        let score = cands.get(elem).unwrap_or(0.0);
        if weight as f64 + score < 0.0 {
            elem.drop_tree(ctx)?;
        } else if elem.text_content().matches(',').count() < 10 && should_remove(elem, weight, min_text_length, &mut allowed) {
            elem.drop_tree(ctx)?;
        }
    }
    Ok(node.tostring(Method::Xml, true))
}

fn count(elem: Node, path: &str) -> i64 {
    elem.findall(path).len() as i64
}

/// The `to_remove` decision chain of the conditional cleaning loop.
fn should_remove(elem: Node, weight: i64, min_text_length: usize, allowed: &mut HashSet<Node>) -> bool {
    let p = count(elem, ".//p");
    let img = count(elem, ".//img");
    let li = count(elem, ".//li") - 100;
    let embed = count(elem, ".//embed");
    let input = count(elem, ".//input") - count(elem, ".//input[@type=\"hidden\"]");
    let content_length = text_length(elem);
    let density = link_density(elem);
    let short = content_length < min_text_length;
    if p != 0 && img as f64 > 1.0 + p as f64 * 1.3 {
        true
    } else if li > p && !matches!(elem.tag(), "ol" | "ul") {
        true
    } else if input as f64 > p as f64 / 3.0 {
        true
    } else if short && img == 0 {
        true
    } else if short && img > 2 {
        true
    } else if weight < 25 && density > 0.2 {
        true
    } else if weight >= 25 && density > 0.5 {
        true
    } else if (embed == 1 && content_length < 75) || embed > 1 {
        true
    } else if content_length == 0 {
        !rich_neighbourhood(elem, allowed)
    } else {
        false
    }
}

/// The "no content" exception: keep an empty element whose nearest non-empty siblings hold > 1000 chars.
fn rich_neighbourhood(elem: Node, allowed: &mut HashSet<Node>) -> bool {
    let mut siblings = Vec::new();
    if let Some(len) = elem.iter_siblings(&[], false).map(text_length).find(|&l| l > 0) {
        siblings.push(len);
    }
    let limit = siblings.len() + 1;
    for sib in elem.iter_siblings(&[], true) {
        let len = text_length(sib);
        if len > 0 {
            siblings.push(len);
            if siblings.len() >= limit {
                break;
            }
        }
    }
    if !siblings.is_empty() && siblings.iter().sum::<usize>() > 1000 {
        allowed.extend(elem.iter(&["table", "ul", "div", "section"]));
        return true;
    }
    false
}
