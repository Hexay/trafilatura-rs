//! trafilatura 2.3.1 htmlprocessing.py: tree cleaning (with MathML and form handling), pruning, link density and
//! text-node processing.
use super::settings::{Focus, Options, CUT_EMPTY_ELEMS, MANUALLY_CLEANED, MANUALLY_STRIPPED};
use super::utils::{image_src, textfilter, trim_or_none};
use super::xml::{delete_element, separates_inline};
use crate::dom::{Ctx, Node, Result};
use crate::py;
use crate::trafilatura::deduplication::duplicate_test;
use crate::trafilatura::htmlprocessing::{truthy, Env};
use crate::trafilatura::text::trim;
use std::collections::HashSet;

const PRESERVE_IMG_CLEANING: &[&str] = &["figure", "picture", "source"];
/// utils.LINK_FARM_RATIO: link text above this share of the text is a link farm.
const LINK_FARM_RATIO: f64 = 0.9;
const TEX_ANNOTATION_XPATH: &str = r#".//*[local-name()="annotation"][@encoding="application/x-tex"]"#;

/// `recover_math`: MathML becomes its LaTeX source, written into the tail that survives the `math` deletion.
fn recover_math(tree: Node) -> Result<()> {
    for element in tree.iter(&["math"]) {
        let latex = match element.xpath_first(TEX_ANNOTATION_XPATH)? {
            Some(annotation) => trim(&annotation.text().unwrap_or_default()),
            None => trim(&element.get("alttext").unwrap_or_default()),
        };
        if latex.is_empty() {
            continue;
        }
        let (open, close) =
            if element.get("display").as_deref() == Some("block") { ("\\[", "\\]") } else { ("\\(", "\\)") };
        element.set_tail(Some(&format!("{open}{latex}{close}{}", element.tail().unwrap_or_default())))?;
    }
    Ok(())
}

/// `_handle_forms`: a form holding most of the text is a layout wrapper (demoted to div), others go.
fn handle_forms(ctx: &Ctx, tree: Node) -> Result<()> {
    let forms: Vec<Node> = tree.iter(&["form"]).collect();
    if forms.is_empty() {
        return Ok(());
    }
    let total = py::len(&tree.text_content());
    for form in forms {
        if total > 0 && py::len(&form.text_content()) as f64 > total as f64 / 2.0 {
            form.set_tag("div")?;
        } else {
            delete_element(ctx, form, true)?;
        }
    }
    Ok(())
}

/// `tree_cleaning`.
pub fn tree_cleaning(ctx: &Ctx, tree: Node, opts: &Options) -> Result<Node> {
    recover_math(tree)?;
    let mut cleaning_list: Vec<&str> = MANUALLY_CLEANED.to_vec();
    let mut stripping_list: Vec<&str> = MANUALLY_STRIPPED.to_vec();
    let clean_forms = cleaning_list.contains(&"form");
    cleaning_list.retain(|&t| t != "form");
    if !opts.tables {
        cleaning_list.extend(["table", "td", "th", "tr"]);
    } else {
        for elem in tree.iter(&["figure", "table"]) {
            let layout = if elem.tag() == "figure" {
                elem.find(".//table").is_some()
            } else {
                matches!(elem.get("role").as_deref(), Some("presentation" | "none"))
            };
            if layout {
                elem.set_tag("div")?;
            }
        }
    }
    if opts.images {
        cleaning_list.retain(|e| !PRESERVE_IMG_CLEANING.contains(e));
        stripping_list.retain(|&t| t != "img");
    }
    tree.strip_tags(ctx, &stripping_list);
    // recall: undo the deletions if they remove every paragraph (copied only if no paragraph can survive)
    let copy_needed = opts.focus == Focus::Recall
        && tree.find(".//p").is_some()
        && (cleaning_list.contains(&"p")
            || !tree.iter(&["p"]).any(|p| p.iter_ancestors(&cleaning_list).next().is_none_or(|a| a == tree)));
    let tcopy = copy_needed.then(|| tree.deep_copy(ctx));
    for expression in &cleaning_list {
        for element in tree.iter(&[expression]) {
            delete_element(ctx, element, true)?;
        }
    }
    let tree = match tcopy {
        Some(copy) if tree.find(".//p").is_none() => copy,
        _ => tree,
    };
    if clean_forms {
        handle_forms(ctx, tree)?;
    }
    prune_html(ctx, tree, opts.focus)
}

/// `prune_html`: empty CUT_EMPTY_ELEMS go, their tails kept except in precision mode.
pub fn prune_html(ctx: &Ctx, tree: Node, focus: Focus) -> Result<Node> {
    let tails = focus != Focus::Precision;
    let empty: Vec<Node> =
        tree.iter_descendants(CUT_EMPTY_ELEMS).filter(|e| e.text().is_none() && e.is_empty()).collect();
    for element in empty {
        delete_element(ctx, element, tails)?;
    }
    Ok(tree)
}

/// `prune_unwanted_nodes`. With `with_backup`, nothing is pruned when that would leave a seventh of the text or
/// less (all expressions are evaluated on the unpruned tree).
pub fn prune_unwanted_nodes(ctx: &Ctx, tree: Node, nodelist: &[&str], with_backup: bool) -> Result<Node> {
    if !with_backup {
        for expression in nodelist {
            for subtree in tree.xpath_nodes(expression)? {
                delete_element(ctx, subtree, true)?;
            }
        }
        return Ok(tree);
    }
    let mut seen = HashSet::new();
    let mut matches = Vec::new();
    for expression in nodelist {
        matches.extend(tree.xpath_nodes(expression)?.into_iter().filter(|&e| seen.insert(e)));
    }
    let outer: Vec<Node> = matches.into_iter().filter(|e| !e.iter_ancestors(&[]).any(|a| seen.contains(&a))).collect();
    let old_len = py::len(&tree.text_content());
    let pruned: usize = outer.iter().map(|e| py::len(&e.text_content())).sum();
    if (old_len as f64 - pruned as f64) > old_len as f64 / 7.0 {
        for elem in outer {
            delete_element(ctx, elem, true)?;
        }
    }
    Ok(tree)
}

/// `collect_link_info`: (lengths, count, short elements). Callers only use the text list's size, so each link's
/// trimmed text is measured, not built.
fn collect_link_info(links: &[Node]) -> (usize, usize, usize) {
    let lens: Vec<usize> = links.iter().map(|l| l.text_content_trim_len()).filter(|&n| n > 0).collect();
    (lens.iter().sum(), lens.len(), lens.iter().filter(|&&n| n < 10).count())
}

/// `is_paragraph_listing`: every link alone in its paragraph (a document listing, not a farm).
fn is_paragraph_listing(links: &[Node]) -> bool {
    links.iter().all(|l| l.parent().is_some_and(|p| p.tag() == "p" && p.findall(".//ref").len() <= 1))
}

/// `link_density_test` -> (boilerplate, short and linked).
pub fn link_density_test(element: Node, favor_precision: bool) -> (bool, bool) {
    let links = element.findall(".//ref");
    if links.is_empty() || element.find(".//graphic").is_some() {
        return (false, false);
    }
    let elemlen = element.text_content_trim_len();
    if links.len() == 1 {
        let threshold = if favor_precision { 10 } else { 100 };
        let link_len = links[0].text_content_trim_len();
        if link_len > threshold && link_len as f64 > elemlen as f64 * 0.9 {
            return (true, false);
        }
    }
    let limitlen = match (element.tag() == "p", element.next().is_none()) {
        (true, true) => 60,
        (true, false) => 30,
        (false, true) => 300,
        (false, false) => 100,
    };
    if elemlen < limitlen {
        let (linklen, elemnum, shortelems) = collect_link_info(&links);
        if elemnum == 0 {
            return (true, false);
        }
        let dense = linklen as f64 > elemlen as f64 * 0.8 || (elemnum > 1 && shortelems as f64 / elemnum as f64 > 0.8);
        return (dense, true);
    }
    if links.len() > 4 {
        let (linklen, elemnum, _) = collect_link_info(&links);
        if linklen as f64 > elemlen as f64 * LINK_FARM_RATIO && linklen < 100 * elemnum && !is_paragraph_listing(&links)
        {
            return (true, false);
        }
    }
    (false, false)
}

/// `link_density_test_tables`.
pub fn link_density_test_tables(element: Node) -> bool {
    let links = element.findall(".//ref");
    if links.is_empty() {
        return false;
    }
    let elemlen = element.text_content_trim_len();
    if elemlen < 200 {
        return false;
    }
    let (linklen, _, _) = collect_link_info(&links);
    let factor = if elemlen < 1000 { 0.8 } else { 0.5 };
    linklen as f64 > factor * elemlen as f64
}

/// `delete_by_link_density`.
pub fn delete_by_link_density(
    ctx: &Ctx,
    subtree: Node,
    tagname: &str,
    backtracking: bool,
    favor_precision: bool,
) -> Result<Node> {
    let mut deletions: Vec<Node> = Vec::new();
    let len_threshold = if favor_precision { 200 } else { 100 };
    let depth_threshold = if favor_precision { 1 } else { 3 };
    for elem in subtree.iter(&[tagname]) {
        let (result, short_with_links) = link_density_test(elem, favor_precision);
        let backtrack = backtracking
            && short_with_links
            && elem.len() >= depth_threshold
            && elem.text_content_trim_len() < len_threshold;
        if result || backtrack {
            if tagname == "p" && elem.parent().is_some_and(|p| matches!(p.tag(), "item" | "td" | "th")) {
                continue;
            }
            deletions.push(elem);
        }
    }
    let mut seen = HashSet::new();
    for elem in deletions {
        if seen.insert(elem) {
            delete_element(ctx, elem, true)?;
        }
    }
    Ok(subtree)
}

/// `handle_textnode`.
pub fn handle_textnode(env: &mut Env, elem: Node, comments_fix: bool, preserve_spaces: bool) -> Result<Option<Node>> {
    if elem.tag() == "graphic" && image_src(elem).is_some() {
        return Ok(Some(elem));
    }
    if elem.tag() == "done"
        || (elem.is_empty()
            && !truthy(&elem.text())
            && !truthy(&elem.tail())
            && (comments_fix || !separates_inline(elem)))
    {
        return Ok(None);
    }
    if !comments_fix && elem.tag() == "lb" {
        if !preserve_spaces {
            elem.set_tail(trim_or_none(elem.tail()).as_deref())?;
        }
        return Ok(Some(elem));
    }
    if !truthy(&elem.text()) && elem.is_empty() {
        let tail = elem.tail();
        elem.set_text(tail.as_deref())?;
        elem.set_tail(Some(""))?;
        if comments_fix && elem.tag() == "lb" {
            elem.set_tag("p")?;
        }
    }
    if !preserve_spaces {
        elem.set_text(trim_or_none(elem.text()).as_deref())?;
        if truthy(&elem.tail()) {
            elem.set_tail(trim_or_none(elem.tail()).as_deref())?;
        }
    }
    if (!truthy(&elem.text()) && textfilter(elem)) || (env.opts.dedup && duplicate_test(elem, env.opts, env.lru)) {
        return Ok(None);
    }
    Ok(Some(elem))
}

/// `process_node`.
pub fn process_node(env: &mut Env, elem: Node) -> Result<Option<Node>> {
    if elem.tag() == "done" || (elem.is_empty() && !truthy(&elem.text()) && !truthy(&elem.tail())) {
        return Ok(None);
    }
    let (text, tail) = (trim_or_none(elem.text()), trim_or_none(elem.tail()));
    elem.set_text(text.as_deref())?;
    elem.set_tail(tail.as_deref())?;
    if elem.tag() != "lb" && !truthy(&elem.text()) && truthy(&elem.tail()) && elem.is_empty() {
        let tail = elem.tail();
        elem.set_text(tail.as_deref())?;
        elem.set_tail(None)?;
    }
    if (truthy(&elem.text()) || truthy(&elem.tail()))
        && (textfilter(elem) || (env.opts.dedup && duplicate_test(elem, env.opts, env.lru)))
    {
        return Ok(None);
    }
    Ok(Some(elem))
}
