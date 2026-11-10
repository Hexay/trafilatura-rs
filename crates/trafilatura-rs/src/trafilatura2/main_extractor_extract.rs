//! trafilatura 2.2.0 main_extractor.py driver: `prune_unwanted_sections`, `recover_wild_text`, `_extract`,
//! `extract_content`.
use super::htmlprocessing::{delete_by_link_density, link_density_test_tables, prune_unwanted_nodes};
use super::main_extractor::{elem_text, handle_textelem, PotentialTags};
use super::settings::{Focus, DEDUPE_SCAN_CAP, MIN_DUPLICATE_LENGTH, TAG_CATALOG};
use super::xml::delete_element;
use super::xpaths::{
    BODY_XPATH, DISCARD_IMAGE_ELEMENTS, OVERALL_DISCARD_XPATH, PRECISION_DISCARD_XPATH, TEASER_DISCARD_XPATH,
};
use crate::dom::{Node, Result};
use crate::py;
use crate::trafilatura::htmlprocessing::Env;
use std::collections::HashSet;

const NOT_AT_THE_END: &[&str] = &["head", "ref"];

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
    Ok(tree)
}

/// `recover_wild_text`: handled wild elements appended unless they repeat what the body already holds.
pub fn recover_wild_text(env: &mut Env, tree: Node, result_body: Node, potential_tags: &PotentialTags) -> Result<Node> {
    let mut potential_tags = potential_tags.clone();
    let mut search_expr = String::from(".//code|.//p|.//quote|.//table|.//div[contains(@class, 'w3-code')]");
    if env.opts.focus == Focus::Recall {
        potential_tags.extend(["div", "lb"]);
        search_expr += "|.//div|.//lb|.//list";
    }
    let search_tree = prune_unwanted_sections(env, tree, &potential_tags, env.opts.fast)?;
    let unwanted: &[&str] = if potential_tags.contains("ref") { &["span"] } else { &["a", "ref", "span"] };
    search_tree.strip_tags(env.ctx, unwanted);
    let subelems = search_tree.xpath_nodes(&search_expr)?;
    let elem_texts: Vec<String> = result_body.children().into_iter().map(elem_text).collect();
    let mut existing = elem_texts.iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join("\n");
    let mut existing_len = py::len(&existing);
    let mut existing_elems: HashSet<String> = elem_texts.into_iter().collect();
    for subelem in subelems {
        let Some(processed) = handle_textelem(env, subelem, &potential_tags)? else { continue };
        let text = elem_text(processed);
        let under_cap = existing_len <= DEDUPE_SCAN_CAP;
        let text_len = py::len(&text);
        if !text.is_empty()
            && (existing_elems.contains(&text)
                || (text_len > MIN_DUPLICATE_LENGTH && under_cap && existing.contains(&text)))
        {
            continue;
        }
        result_body.append(processed)?;
        if under_cap {
            existing.push('\n');
            existing.push_str(&text);
            existing_len += 1 + text_len;
        }
        existing_elems.insert(text);
    }
    Ok(result_body)
}

fn body_text(body: Node) -> String {
    py::strip(&body.itertext(true).join(" ")).to_owned()
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
        if !potential_tags.contains("ref") {
            subtree.strip_tags(env.ctx, &["ref"]);
        }
        if !potential_tags.contains("span") {
            subtree.strip_tags(env.ctx, &["span"]);
        }
        let mut subelems = subtree.xpath_nodes(".//*")?;
        if !subelems.is_empty() && subelems.iter().all(|e| e.tag() == "lb") {
            subelems = vec![subtree];
        }
        let mut processed = Vec::new();
        for e in subelems {
            if let Some(p) = handle_textelem(env, e, &potential_tags)? {
                processed.push(p);
            }
        }
        result_body.extend(&processed)?;
        while let Some(last) = result_body.child(-1).filter(|c| NOT_AT_THE_END.contains(&c.tag())) {
            delete_element(env.ctx, last, false)?;
        }
        if result_body.children().iter().filter(|e| e.tag() != "graphic").count() > 1 {
            break;
        }
    }
    let temp_text = body_text(result_body);
    Ok((result_body, temp_text, potential_tags))
}

/// `extract_content`: (body, text, text length).
pub fn extract_content(env: &mut Env, cleaned_tree: Node) -> Result<(Node, String, usize)> {
    let backup_tree = cleaned_tree.deep_copy(env.ctx);
    let (mut result_body, mut temp_text, potential_tags) = extract(env, cleaned_tree)?;
    if result_body.is_empty() || py::len(&temp_text) < env.opts.min_extracted_size {
        result_body = recover_wild_text(env, backup_tree, result_body, &potential_tags)?;
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
    let n = py::len(&temp_text);
    Ok((result_body, temp_text, n))
}
