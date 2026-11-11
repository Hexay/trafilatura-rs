//! trafilatura 2.3.1 main_extractor.py driver: `prune_unwanted_sections`, `recover_wild_text`, `_extract`,
//! `extract_content`.
use super::htmlprocessing::{delete_by_link_density, link_density_test_tables, process_node, prune_unwanted_nodes};
use super::main_extractor::{elem_text, handle_textelem, PotentialTags};
use super::settings::{Focus, DEDUPE_SCAN_CAP, INLINE_FORMATTABLE, MIN_DUPLICATE_LENGTH, TAG_CATALOG};
use super::xml::delete_element;
use super::xpaths::{
    BODY_XPATH, DISCARD_IMAGE_ELEMENTS, OVERALL_DISCARD_XPATH, PRECISION_DISCARD_XPATH, TEASER_DISCARD_XPATH,
};
use crate::dom::{Node, Result};
use crate::py;
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::main_extractor_extract::body_text;
use crate::trafilatura::text::{text_chars_test, trim};
use std::collections::HashSet;

const NOT_AT_THE_END: &[&str] = &["head", "ref"];
/// Blocks rebuilt as new elements by `handle_textelem`, which leaves the source tail behind.
const REBUILT_BLOCKS: &[&str] = &["code", "quote", "list", "table"];
const SPACING_PROTECTED: &[&str] = &["code", "pre"];

pub type Extraction = (Node, String);

/// `prune_unwanted_sections`.
pub fn prune_unwanted_sections(
    env: &mut Env,
    tree: Node,
    potential_tags: &PotentialTags,
    keep_teasers: bool,
) -> Result<Node> {
    let ctx = env.ctx;
    let favor_precision = env.opts.focus == Focus::Precision;
    let mut tree = prune_unwanted_nodes(ctx, tree, &OVERALL_DISCARD_XPATH, true)?;
    if !potential_tags.contains("graphic") {
        tree = prune_unwanted_nodes(ctx, tree, &DISCARD_IMAGE_ELEMENTS, false)?;
    }
    if env.opts.focus != Focus::Recall {
        if !keep_teasers {
            tree = prune_unwanted_nodes(ctx, tree, &TEASER_DISCARD_XPATH, false)?;
        }
        if favor_precision {
            tree = prune_unwanted_nodes(ctx, tree, &PRECISION_DISCARD_XPATH, false)?;
        }
    }
    for _ in 0..2 {
        tree = delete_by_link_density(ctx, tree, "div", true, favor_precision)?;
        tree = delete_by_link_density(ctx, tree, "list", false, favor_precision)?;
        tree = delete_by_link_density(ctx, tree, "p", false, favor_precision)?;
    }
    if potential_tags.contains("table") || favor_precision {
        let boilerplate: Vec<Node> = tree.iter(&["table"]).filter(|&t| link_density_test_tables(t)).collect();
        for elem in boilerplate {
            delete_element(ctx, elem, false)?;
        }
    }
    if favor_precision {
        while let Some(last) = tree.child(-1).filter(|c| c.tag() == "head") {
            delete_element(ctx, last, false)?;
        }
        tree = delete_by_link_density(ctx, tree, "head", false, true)?;
        tree = delete_by_link_density(ctx, tree, "quote", false, true)?;
    }
    // after the link density tests, which need the refs
    let unwanted: &[&str] = if potential_tags.contains("ref") { &["span"] } else { &["span", "ref"] };
    tree.strip_tags(ctx, unwanted);
    Ok(tree)
}

/// `recover_wild_text`: handled wild elements appended unless they repeat what the body already holds.
pub fn recover_wild_text(
    env: &mut Env,
    tree: Node,
    result_body: Node,
    potential_tags: &PotentialTags,
    consumed: &HashSet<Node>,
) -> Result<Node> {
    let mut potential_tags = potential_tags.clone();
    let mut search_expr = String::from(".//code|.//p|.//quote|.//table|.//div[contains(@class, 'w3-code')]");
    if env.opts.focus == Focus::Recall {
        potential_tags.extend(["div", "lb"]);
        search_expr += "|.//div|.//lb|.//list";
    }
    let search_tree = prune_unwanted_sections(env, tree, &potential_tags, env.opts.fast)?;
    let subelems = search_tree.xpath_nodes(&search_expr)?;
    let elem_texts: Vec<String> = result_body.children().into_iter().map(elem_text).collect();
    let mut existing = elem_texts.iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join("\n");
    let mut existing_len = py::len(&existing);
    let mut existing_elems: HashSet<String> = elem_texts.into_iter().collect();
    let mut handled: HashSet<Node> = HashSet::new();
    for subelem in subelems {
        if consumed.contains(&subelem) || subelem.iter_ancestors(&[]).any(|a| handled.contains(&a)) {
            continue;
        }
        let Some(processed) = handle_textelem(env, subelem, &potential_tags)? else { continue };
        if processed == subelem {
            handled.insert(subelem);
        }
        let text = elem_text(processed);
        // image-only blocks have no text to compare, their sources are checked against the body
        let images: HashSet<Option<String>> = processed.iter(&["graphic"]).map(|g| g.get("src")).collect();
        if text.is_empty() && !images.is_empty() {
            let body_images: HashSet<Option<String>> = result_body.iter(&["graphic"]).map(|g| g.get("src")).collect();
            if images.is_subset(&body_images) {
                continue;
            }
        }
        let under_cap = existing_len <= DEDUPE_SCAN_CAP;
        // a copy carrying its source tail is a fragment of a text run: no length gate
        let fragment = match processed.tail() {
            Some(tail) if text_chars_test(Some(&tail)) => trim(&format!("{text}{tail}")),
            _ => String::new(),
        };
        let duplicate = existing_elems.contains(&text)
            || (under_cap
                && ((py::len(&text) > MIN_DUPLICATE_LENGTH && existing.contains(&text))
                    || (!fragment.is_empty() && existing.contains(&fragment))));
        if !text.is_empty() && duplicate {
            continue;
        }
        result_body.append(processed)?;
        if under_cap {
            let added = if fragment.is_empty() { &text } else { &fragment };
            existing.push('\n');
            existing.push_str(added);
            existing_len += 1 + py::len(added);
        }
        existing_elems.insert(text);
    }
    Ok(result_body)
}

/// Whether `el` now belongs to `result_body`'s document (`el.getroottree().getroot() is result_body`).
fn in_body(el: Node, result_body: Node) -> bool {
    el.getroot() == Some(result_body)
}

/// One BODY_XPATH frame's elements into `result_body`; the frame's own leading text as a paragraph, if any.
fn extract_frame(
    env: &mut Env,
    subtree: Node,
    result_body: Node,
    potential_tags: &PotentialTags,
) -> Result<Option<Node>> {
    let mut subelems = subtree.xpath_nodes(".//*")?;
    let tags: HashSet<&str> = subelems.iter().map(|e| e.tag()).collect();
    let inline_run = tags.iter().all(|t| *t == "lb" || INLINE_FORMATTABLE.contains(t));
    let lb_only = !tags.is_empty() && tags.iter().all(|t| *t == "lb");
    if lb_only || (subtree.tag() == "div" && potential_tags.contains("div") && inline_run) {
        subelems = vec![subtree];
    }
    let mut lead = None;
    if subelems != [subtree] && text_chars_test(subtree.text().as_deref()) {
        let p = env.ctx.element("p")?;
        p.set_text(subtree.text().as_deref())?;
        lead = process_node(env, p)?;
    }
    for elem in subelems {
        // an element moved into the body with an ancestor div is not revisited
        if in_body(elem, result_body) {
            continue;
        }
        let (tag, tail) = (elem.tag(), elem.tail());
        let processed = handle_textelem(env, elem, potential_tags)?;
        if let Some(p) = processed {
            result_body.append(p)?;
            if !in_body(elem, result_body) {
                elem.set_tag("done")?;
            }
        }
        let tail_text = tail.filter(|t| text_chars_test(Some(t)));
        if let Some(tail) = tail_text.filter(|_| REBUILT_BLOCKS.contains(&tag)) {
            if processed.is_none_or(|p| p.tail().is_none()) {
                let tail_elem = env.ctx.element("p")?;
                tail_elem.set_text(Some(&tail))?;
                if process_node(env, tail_elem)?.is_some() {
                    result_body.append(tail_elem)?;
                }
            }
        }
    }
    Ok(lead)
}

/// `_extract`: (result body, text, potential tags).
fn extract(env: &mut Env, tree: Node) -> Result<(Node, String, PotentialTags)> {
    let opts = env.opts;
    let mut potential_tags: PotentialTags = TAG_CATALOG.iter().copied().collect();
    if opts.tables {
        potential_tags.extend(["table", "td", "th", "tr"]);
    }
    if opts.images {
        potential_tags.insert("graphic");
    }
    if opts.links {
        potential_tags.insert("ref");
    }
    let result_body = env.ctx.element("body")?;
    for expr in BODY_XPATH {
        let Some(subtree) = tree.xpath_first(expr)? else { continue };
        let subtree = prune_unwanted_sections(env, subtree, &potential_tags, false)?;
        if subtree.is_empty() {
            continue;
        }
        let ptest = subtree.xpath_strings("//p//text()")?;
        let factor = if opts.focus == Focus::Precision { 1 } else { 3 };
        if ptest.is_empty() || py::len(&ptest.concat()) < opts.min_extracted_size * factor {
            potential_tags.insert("div");
        }
        let start = result_body.len();
        let lead = extract_frame(env, subtree, result_body, &potential_tags)?;
        while let Some(last) = result_body.child(-1).filter(|c| NOT_AT_THE_END.contains(&c.tag())) {
            delete_element(env.ctx, last, false)?;
        }
        if result_body.children().iter().filter(|e| e.tag() != "graphic").count() > 1 {
            if let Some(lead) = lead {
                result_body.insert(start as isize, lead)?;
            }
            break;
        }
    }
    let temp_text = body_text(result_body);
    Ok((result_body, temp_text, potential_tags))
}

/// `extract_content`: (body, text).
pub fn extract_content(env: &mut Env, cleaned_tree: Node) -> Result<Extraction> {
    let backup_tree = cleaned_tree.deep_copy(env.ctx);
    // source order before the main pass moves elements; the backup keeps it
    let order: Vec<Node> = cleaned_tree.iter(&[]).collect();
    let (mut result_body, mut temp_text, potential_tags) = extract(env, cleaned_tree)?;
    if result_body.is_empty() || py::len(&temp_text) < env.opts.min_extracted_size {
        let consumed: HashSet<Node> =
            order.iter().zip(backup_tree.iter(&[])).filter(|(e, _)| e.tag() == "done").map(|(_, twin)| twin).collect();
        result_body = recover_wild_text(env, backup_tree, result_body, &potential_tags, &consumed)?;
        temp_text = body_text(result_body);
    }
    let mut previous: Option<String> = None;
    for el in result_body.children() {
        let current = elem_text(el);
        if !current.is_empty() && previous.as_ref() == Some(&current) && py::len(&current) > MIN_DUPLICATE_LENGTH {
            delete_element(env.ctx, el, false)?;
        } else {
            previous = Some(current);
        }
    }
    result_body.strip_elements(env.ctx, &["done"], true);
    result_body.strip_tags(env.ctx, &["div"]);
    for linebreak in result_body.iter(&["lb"]) {
        let blank_tail = linebreak.tail().is_some_and(|t| !t.is_empty() && !text_chars_test(Some(&t)));
        if blank_tail && !linebreak.iter_ancestors(&[]).any(|a| SPACING_PROTECTED.contains(&a.tag())) {
            linebreak.set_tail(None)?;
        }
    }
    Ok((result_body, temp_text))
}
