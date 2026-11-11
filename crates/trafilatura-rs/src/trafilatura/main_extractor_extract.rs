//! main_extractor.py driver: `prune_unwanted_sections`, `recover_wild_text`, `_extract`, `extract_content`.
use super::htmlprocessing::{
    attribute_error, delete_by_link_density, link_density_test_tables, prune_unwanted_nodes, Env,
};
use super::main_extractor::{handle_textelem, PotentialTags, TAG_CATALOG};
use super::settings::Focus;
use super::xpaths::{
    BODY_XPATH, DISCARD_IMAGE_ELEMENTS, OVERALL_DISCARD_XPATH, PAYWALL_DISCARD_XPATH, PRECISION_DISCARD_XPATH,
    TEASER_DISCARD_XPATH,
};
use crate::dom::{Node, Result};
use crate::py;

const NOT_AT_THE_END: &[&str] = &["head", "ref"];

fn remove_last(env: &Env, tree: Node) -> Result<()> {
    let last = tree.child(-1).expect("non-empty");
    last.parent().ok_or_else(|| attribute_error("'NoneType' object has no attribute 'remove'"))?.remove(env.ctx, last)
}

/// `prune_unwanted_sections`.
pub fn prune_unwanted_sections(env: &mut Env, tree: Node, potential_tags: &PotentialTags) -> Result<Node> {
    let ctx = env.ctx;
    let favor_precision = env.opts.focus == Focus::Precision;
    let mut tree = prune_unwanted_nodes(ctx, tree, &OVERALL_DISCARD_XPATH, true)?;
    tree = prune_unwanted_nodes(ctx, tree, &PAYWALL_DISCARD_XPATH, false)?;
    if !potential_tags.contains("graphic") {
        tree = prune_unwanted_nodes(ctx, tree, &DISCARD_IMAGE_ELEMENTS, false)?;
    }
    if env.opts.focus != Focus::Recall {
        tree = prune_unwanted_nodes(ctx, tree, &TEASER_DISCARD_XPATH, false)?;
        if favor_precision {
            tree = prune_unwanted_nodes(ctx, tree, &PRECISION_DISCARD_XPATH, false)?;
        }
    }
    tree = delete_by_link_density(ctx, tree, "div", true, favor_precision)?;
    tree = delete_by_link_density(ctx, tree, "list", false, favor_precision)?;
    tree = delete_by_link_density(ctx, tree, "p", false, favor_precision)?;
    if favor_precision {
        while tree.child(-1).is_some_and(|c| c.tag() == "head") {
            remove_last(env, tree)?;
        }
        tree = delete_by_link_density(ctx, tree, "head", false, false)?;
        tree = delete_by_link_density(ctx, tree, "quote", false, false)?;
    }
    Ok(tree)
}

/// `recover_wild_text`.
pub fn recover_wild_text(
    env: &mut Env,
    tree: Node,
    result_body: Node,
    potential_tags: &mut PotentialTags,
) -> Result<Node> {
    let mut search_expr =
        String::from(".//blockquote|.//code|.//p|.//pre|.//q|.//quote|.//table|.//div[contains(@class, 'w3-code')]");
    if env.opts.focus == Focus::Recall {
        potential_tags.extend(["div", "lb"]);
        search_expr += "|.//div|.//lb|.//list";
    }
    let search_tree = prune_unwanted_sections(env, tree, potential_tags)?;
    if !potential_tags.contains("ref") {
        search_tree.strip_tags(env.ctx, &["a", "ref", "span"]);
    } else {
        search_tree.strip_tags(env.ctx, &["span"]);
    }
    for e in search_tree.xpath_nodes(&search_expr)? {
        if let Some(processed) = handle_textelem(env, e, potential_tags)? {
            result_body.append(processed)?;
        }
    }
    Ok(result_body)
}

pub(crate) fn body_text(body: Node) -> String {
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
        let subtree = prune_unwanted_sections(env, subtree, &potential_tags)?;
        if potential_tags.contains("table") || opts.focus == Focus::Precision {
            for elem in subtree.iter(&["table"]) {
                if link_density_test_tables(elem) {
                    elem.parent()
                        .ok_or_else(|| attribute_error("'NoneType' object has no attribute 'remove'"))?
                        .remove(env.ctx, elem)?;
                }
            }
        }
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
        while result_body.child(-1).is_some_and(|c| NOT_AT_THE_END.contains(&c.tag())) {
            remove_last(env, result_body)?;
        }
        if result_body.len() > 1 {
            break;
        }
    }
    let temp_text = body_text(result_body);
    Ok((result_body, temp_text, potential_tags))
}

/// `extract_content`: (body, text, text length).
pub fn extract_content(env: &mut Env, cleaned_tree: Node) -> Result<(Node, String, usize)> {
    // Rebuilding this backup lazily was measured slower: the wild-text fallback runs on most pages.
    let backup_tree = cleaned_tree.deep_copy(env.ctx);
    let (mut result_body, mut temp_text, mut potential_tags) = extract(env, cleaned_tree)?;
    if result_body.is_empty() || py::len(&temp_text) < env.opts.min_extracted_size {
        result_body = recover_wild_text(env, backup_tree, result_body, &mut potential_tags)?;
        temp_text = body_text(result_body);
    }
    result_body.strip_elements(env.ctx, &["done"], true);
    result_body.strip_tags(env.ctx, &["div"]);
    let n = py::len(&temp_text);
    Ok((result_body, temp_text, n))
}
