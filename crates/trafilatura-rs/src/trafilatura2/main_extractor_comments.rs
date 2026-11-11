//! trafilatura 2.2.0 main_extractor.py `process_comments_node` and `extract_comments` (include_comments=True).
use super::htmlprocessing::{handle_textnode, prune_unwanted_nodes};
use super::settings::TAG_CATALOG;
use super::xml::delete_element;
use super::xpaths::{COMMENTS_DISCARD_XPATH, COMMENTS_XPATH};
use crate::dom::{Node, Result};
use crate::py;
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::main_extractor_extract::body_text;

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
        // see crate::trafilatura::main_extractor_comments: nodes move into comments_body one at a time
        for e in subtree.xpath_nodes(".//*")? {
            if let Some(p) = process_comments_node(env, e)? {
                comments_body.append(p)?;
            }
        }
        if !comments_body.is_empty() {
            delete_element(env.ctx, subtree, false)?;
            break;
        }
    }
    let text = body_text(comments_body);
    let n = py::len(&text);
    Ok((comments_body, text, n))
}
