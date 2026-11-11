//! main_extractor.py `process_comments_node` and `extract_comments` (include_comments=True).
use super::htmlprocessing::{attribute_error, handle_textnode, prune_unwanted_nodes, Env};
use super::main_extractor::TAG_CATALOG;
use super::main_extractor_extract::body_text;
use super::xpaths::{COMMENTS_DISCARD_XPATH, COMMENTS_XPATH};
use crate::dom::{Node, Result};
use crate::py;

/// `process_comments_node`.
fn process_comments_node(env: &mut Env, elem: Node) -> Result<Option<Node>> {
    if !TAG_CATALOG.contains(&elem.tag()) {
        return Ok(None);
    }
    let processed = handle_textnode(env, elem, true, false)?;
    if let Some(p) = processed {
        p.clear_attrs();
    }
    Ok(processed)
}

/// `extract_comments`: (comments body, text, text length). The comments section found is removed from `tree`.
pub fn extract_comments(env: &mut Env, tree: Node) -> Result<(Node, String, usize)> {
    let comments_body = env.ctx.element("body")?;
    for expr in COMMENTS_XPATH {
        let Some(subtree) = tree.xpath_first(expr)? else { continue };
        let subtree = prune_unwanted_nodes(env.ctx, subtree, &COMMENTS_DISCARD_XPATH, false)?;
        subtree.strip_tags(env.ctx, &["a", "ref", "span"]);
        // Python extends lazily: each processed node moves into comments_body before the next one is processed.
        for e in subtree.xpath_nodes(".//*")? {
            if let Some(p) = process_comments_node(env, e)? {
                comments_body.append(p)?;
            }
        }
        if !comments_body.is_empty() {
            subtree
                .parent()
                .ok_or_else(|| attribute_error("'NoneType' object has no attribute 'remove'"))?
                .remove(env.ctx, subtree)?;
            break;
        }
    }
    let text = body_text(comments_body);
    let n = py::len(&text);
    Ok((comments_body, text, n))
}
