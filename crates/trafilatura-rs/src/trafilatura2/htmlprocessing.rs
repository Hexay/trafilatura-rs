//! trafilatura 2.2.0 htmlprocessing.py: tree cleaning, pruning, link density and text-node processing.
use super::settings::{Focus, Options, CUT_EMPTY_ELEMS, MANUALLY_CLEANED, MANUALLY_STRIPPED};
use super::utils::{is_image_element, textfilter, trim_or_none};
use super::xml::delete_element;
use crate::dom::{Ctx, Node, Result};
use crate::py;
use crate::trafilatura::deduplication::duplicate_test;
use crate::trafilatura::htmlprocessing::{truthy, Env};

const PRESERVE_IMG_CLEANING: &[&str] = &["figure", "picture", "source"];
/// utils.LINK_FARM_RATIO: link text above this share of the text is a link farm.
const LINK_FARM_RATIO: f64 = 0.9;

/// `tree_cleaning`.
pub fn tree_cleaning(ctx: &Ctx, tree: Node, opts: &Options) -> Result<Node> {
    let mut cleaning_list: Vec<&str> = MANUALLY_CLEANED.to_vec();
    let mut stripping_list: Vec<&str> = MANUALLY_STRIPPED.to_vec();
    if !opts.tables {
        cleaning_list.extend(["table", "td", "th", "tr"]);
    } else {
        for elem in tree.xpath_nodes(".//figure[descendant::table]")? {
            elem.set_tag("div")?;
        }
        for elem in tree.xpath_nodes(r#".//table[@role="presentation" or @role="none"]"#)? {
            elem.set_tag("div")?;
        }
    }
    if opts.images {
        cleaning_list.retain(|e| !PRESERVE_IMG_CLEANING.contains(e));
        stripping_list.retain(|&t| t != "img");
    }
    tree.strip_tags(ctx, &stripping_list);
    let tcopy = (opts.focus == Focus::Recall && tree.find(".//p").is_some()).then(|| tree.deep_copy(ctx));
    for expression in &cleaning_list {
        for element in tree.iter(&[expression]) {
            delete_element(ctx, element, true)?;
        }
    }
    let tree = match tcopy {
        Some(copy) if tree.find(".//p").is_none() => copy,
        _ => tree,
    };
    prune_html(ctx, tree, opts.focus)
}

/// `prune_html`: empty CUT_EMPTY_ELEMS go, their tails kept except in precision mode.
pub fn prune_html(ctx: &Ctx, tree: Node, focus: Focus) -> Result<Node> {
    let tails = focus != Focus::Precision;
    for element in tree.xpath_nodes(".//processing-instruction()|.//*[not(node())]")? {
        if element.is_element() && CUT_EMPTY_ELEMS.contains(&element.tag()) {
            delete_element(ctx, element, tails)?;
        }
    }
    Ok(tree)
}

/// `prune_unwanted_nodes`.
pub fn prune_unwanted_nodes(ctx: &Ctx, tree: Node, nodelist: &[&str], with_backup: bool) -> Result<Node> {
    let backup = with_backup.then(|| (py::len(&tree.text_content()), tree.deep_copy(ctx)));
    for expression in nodelist {
        for subtree in tree.xpath_nodes(expression)? {
            delete_element(ctx, subtree, true)?;
        }
    }
    if let Some((old_len, backup)) = backup {
        let new_len = py::len(&tree.text_content());
        return Ok(if new_len as f64 > old_len as f64 / 7.0 { tree } else { backup });
    }
    Ok(tree)
}

/// `collect_link_info`: (lengths, count, short elements). Callers only use the text list's size, so each link's
/// trimmed text is measured, not built.
fn collect_link_info(links: &[Node]) -> (usize, usize, usize) {
    let lens: Vec<usize> = links.iter().map(|l| l.text_content_trim_len()).filter(|&n| n > 0).collect();
    (lens.iter().sum(), lens.len(), lens.iter().filter(|&&n| n < 10).count())
}

/// `link_density_test(element, trim(element.text_content()))` -> (result, whether the returned list is non-empty).
/// The text's length is only read once links exist, so it is computed there into `elemlen`.
pub fn link_density_test(element: Node, elemlen: &mut Option<usize>, favor_precision: bool) -> (bool, bool) {
    let links = element.findall(".//ref");
    if links.is_empty() || element.find(".//graphic").is_some() {
        return (false, false);
    }
    let elemlen = *elemlen.get_or_insert_with(|| element.text_content_trim_len());
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
        let dense = elemnum == 0
            || linklen as f64 > elemlen as f64 * 0.8
            || (elemnum > 1 && shortelems as f64 / elemnum as f64 > 0.8);
        return (dense, elemnum > 0);
    }
    if links.len() > 4 {
        let (linklen, elemnum, _) = collect_link_info(&links);
        if linklen as f64 > elemlen as f64 * LINK_FARM_RATIO && linklen < 100 * elemnum {
            return (true, elemnum > 0);
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
        let mut elemlen = None;
        let (result, has_list) = link_density_test(elem, &mut elemlen, favor_precision);
        // A non-empty list implies the length was computed.
        let len = || elemlen.expect("computed before any list is built");
        let backtrack = backtracking && has_list && 0 < len() && len() < len_threshold && elem.len() >= depth_threshold;
        if result || backtrack {
            if tagname == "p" && elem.parent().is_some_and(|p| matches!(p.tag(), "item" | "td" | "th")) {
                continue;
            }
            deletions.push(elem);
        }
    }
    let mut seen = std::collections::HashSet::new();
    for elem in deletions {
        if seen.insert(elem) {
            delete_element(ctx, elem, true)?;
        }
    }
    Ok(subtree)
}

/// `handle_textnode`.
pub fn handle_textnode(env: &mut Env, elem: Node, comments_fix: bool, preserve_spaces: bool) -> Result<Option<Node>> {
    if elem.tag() == "graphic" && is_image_element(elem) {
        return Ok(Some(elem));
    }
    if elem.tag() == "done" || (elem.is_empty() && !truthy(&elem.text()) && !truthy(&elem.tail())) {
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
    if elem.tag() != "lb" && !truthy(&elem.text()) && truthy(&elem.tail()) {
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
