//! trafilatura 2.3.1's readability_lxml.py fork: one pass (no ruthless retry), lowercased unlikely-candidate test,
//! descendant-based div-to-p, conditional cleaning over the article subtree only.
use super::regexes::{OK_MAYBE_CANDIDATE_LOWER, UNLIKELY_CANDIDATES_LOWER, VIDEO};
use super::scoring::{class_weight, link_density, score_paragraphs, text_length, Candidates};
use super::{get_article, new_p};
use crate::dom::{Ctx, Method, Node, Result};
use crate::py;
use std::collections::HashMap;

const DIV_TO_P_PREFIXES: &[&str] =
    &["address", "article", "aside", "audio", "blockquote", "dl", "div", "img", "ol", "p", "table", "ul"];
const CONDITIONAL: &[&str] = &["table", "ul", "div", "aside", "header", "footer", "section"];
const TEXT_CLEAN_ELEMS: &[&str] = &["p", "img", "li", "a", "embed", "input"];

/// `Document(doc, min_text_length).summary()`: the cleaned article serialized as XML.
pub fn summary(ctx: &Ctx, doc: Node, min_text_length: usize) -> Result<String> {
    for elem in doc.iter(&["script", "style", "fencedframe"]).collect::<Vec<_>>() {
        elem.drop_tree(ctx)?;
    }
    remove_unlikely_candidates(ctx, doc)?;
    transform_misused_divs_into_paragraphs(ctx, doc)?;
    let cands = score_paragraphs(doc, min_text_length);
    let article = match cands.best() {
        Some(best) => get_article(ctx, &cands, best)?,
        None => doc.find("body").unwrap_or(doc),
    };
    sanitize(ctx, article, &cands, min_text_length)
}

fn remove_unlikely_candidates(ctx: &Ctx, doc: Node) -> Result<()> {
    let mut verdicts: HashMap<String, bool> = HashMap::new();
    for elem in doc.findall(".//*") {
        let attrs: Vec<String> =
            [elem.get("class"), elem.get("id")].into_iter().flatten().filter(|a| !a.is_empty()).collect();
        let attrs = attrs.join(" ");
        if py::len(&attrs) < 2 || matches!(elem.tag(), "body" | "html") {
            continue;
        }
        let unlikely = *verdicts.entry(attrs).or_insert_with_key(|a| {
            let low = a.to_lowercase();
            UNLIKELY_CANDIDATES_LOWER.is_match(&low) && !OK_MAYBE_CANDIDATE_LOWER.is_match(&low)
        });
        if unlikely {
            elem.drop_tree(ctx)?;
        }
    }
    Ok(())
}

fn transform_misused_divs_into_paragraphs(ctx: &Ctx, doc: Node) -> Result<()> {
    for elem in doc.findall(".//div") {
        let has_block = elem.iter_descendants(&["*"]).any(|e| DIV_TO_P_PREFIXES.iter().any(|p| e.tag().starts_with(p)));
        if !has_block && (elem.find(".//a").is_none() || !elem.xpath_strings("text()[normalize-space()]")?.is_empty()) {
            elem.set_tag("p")?;
        }
    }
    for elem in doc.findall(".//div") {
        if let Some(text) = elem.text().filter(|t| !py::strip(t).is_empty()) {
            let p = new_p(ctx, &text)?;
            elem.set_text(None)?;
            elem.insert(0, p)?;
        }
        for (pos, child) in elem.children().into_iter().enumerate().rev() {
            if let Some(tail) = child.tail().filter(|t| !py::strip(t).is_empty()) {
                let p = new_p(ctx, &tail)?;
                child.set_tail(None)?;
                elem.insert(pos as isize + 1, p)?;
            }
            if child.tag() == "br" {
                child.drop_tree(ctx)?;
            }
        }
    }
    Ok(())
}

fn sanitize(ctx: &Ctx, node: Node, cands: &Candidates, min_text_length: usize) -> Result<String> {
    for header in node.iter(&["h1", "h2", "h3", "h4", "h5", "h6"]).collect::<Vec<_>>() {
        if class_weight(header) < 0 || link_density(header) > 0.33 {
            header.drop_tree(ctx)?;
        }
    }
    for elem in node.iter(&["form", "textarea"]).collect::<Vec<_>>() {
        elem.drop_tree(ctx)?;
    }
    for elem in node.iter(&["iframe"]).collect::<Vec<_>>() {
        if elem.has_attr("src") && VIDEO.is_match(&elem.get("src").unwrap_or_default()) {
            elem.set_text(Some("VIDEO"))?;
        } else {
            elem.drop_tree(ctx)?;
        }
    }
    for elem in node.iter(CONDITIONAL).collect::<Vec<_>>().into_iter().rev() {
        let weight = class_weight(elem);
        let score = cands.get(elem).unwrap_or(0.0);
        if weight as f64 + score < 0.0
            || (elem.text_content().matches(',').count() < 10 && should_remove(elem, weight, min_text_length))
        {
            elem.drop_tree(ctx)?;
        }
    }
    Ok(node.tostring(Method::Xml, true))
}

/// The `to_remove` decision chain of the conditional cleaning loop.
fn should_remove(elem: Node, weight: i64, min_text_length: usize) -> bool {
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for e in elem.iter_descendants(TEXT_CLEAN_ELEMS) {
        *counts.entry(e.tag()).or_default() += 1;
    }
    let n = |k: &str| counts.get(k).copied().unwrap_or(0);
    let (p, img, li, embed) = (n("p"), n("img"), n("li") - 100, n("embed"));
    let input = n("input") - elem.findall(".//input[@type=\"hidden\"]").len() as i64;
    let content_length = text_length(elem);
    let density = link_density(elem);
    let short = content_length < min_text_length;
    if (p != 0 && img as f64 > 1.0 + p as f64 * 1.3)
        || (li > p && !matches!(elem.tag(), "ol" | "ul"))
        || input as f64 > p as f64 / 3.0
        || (short && (img == 0 || img > 2))
        || density > if weight >= 25 { 0.5 } else { 0.2 }
        || (embed == 1 && content_length < 75)
        || embed > 1
    {
        return true;
    }
    if content_length == 0 {
        let nearest = |preceding| elem.iter_siblings(&[], preceding).map(text_length).find(|&l| l > 0).unwrap_or(0);
        return nearest(false) + nearest(true) <= 1000;
    }
    false
}
