//! trafilatura 2.3.1 main_extractor.py `extract_comments` (include_comments=True).
use super::htmlprocessing::{handle_textnode, prune_unwanted_nodes};
use super::settings::TAG_CATALOG;
use super::xml::delete_element;
use super::xpaths::{COMMENTS_DISCARD_XPATH, COMMENTS_XPATH};
use crate::dom::{Node, Result};
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::main_extractor_extract::body_text;

/// `extract_comments`: (comments body, text). The comments section found is removed from `tree`.
pub fn extract_comments(env: &mut Env, tree: Node) -> Result<(Node, String)> {
    let comments_body = env.ctx.element("body")?;
    for expr in COMMENTS_XPATH {
        let Some(subtree) = tree.xpath_first(expr)? else { continue };
        let subtree = prune_unwanted_nodes(env.ctx, subtree, &COMMENTS_DISCARD_XPATH, false)?;
        subtree.strip_tags(env.ctx, &["ref", "span"]);
        // see crate::trafilatura::main_extractor_comments: nodes move into comments_body one at a time
        for elem in subtree.xpath_nodes(".//*")? {
            if !TAG_CATALOG.contains(&elem.tag()) {
                continue;
            }
            if let Some(processed) = handle_textnode(env, elem, true, false)? {
                processed.clear_attrs();
                comments_body.append(processed)?;
            }
        }
        if !comments_body.is_empty() {
            delete_element(env.ctx, subtree, false)?;
            break;
        }
    }
    let text = body_text(comments_body);
    Ok((comments_body, text))
}
