//! main_extractor.py `handle_table`.
use super::htmlprocessing::{handle_textnode, process_node, truthy, Env};
use super::main_extractor::{define_newelem, handle_lists, handle_textelem, PotentialTags};
use super::settings::Focus;
use crate::dom::{Node, Result};
use crate::py::int::py_int;

const TABLE_ELEMS: &[&str] = &["td", "th"];
const TABLE_ALL: &[&str] = &["td", "th", "hi"];

fn new_row(ctx: &crate::dom::Ctx, span: &Option<String>) -> Result<Node> {
    let row = ctx.element("row")?;
    if let Some(s) = span {
        row.set("span", Some(s))?;
    }
    Ok(row)
}

/// `handle_table`.
pub fn handle_table(env: &mut Env, table_elem: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    let ctx = env.ctx;
    let newtable = ctx.element("table")?;
    table_elem.strip_tags(ctx, &["thead", "tbody", "tfoot"]);
    let mut max_cols: i128 = 0;
    for tr in table_elem.iter(&["tr"]) {
        let mut sum: i128 = 0;
        for td in tr.iter(TABLE_ELEMS) {
            sum = sum.saturating_add(match td.get("colspan") {
                Some(v) => py_int(&v)?,
                None => 1,
            });
        }
        max_cols = max_cols.max(sum);
    }
    let mut seen_header_row = false;
    let mut seen_header = false;
    let span = (max_cols > 1).then(|| max_cols.to_string());
    let mut newrow = new_row(ctx, &span)?;
    for subelement in table_elem.iter_descendants(&[]) {
        let tag = subelement.tag();
        if tag == "tr" {
            if !newrow.is_empty() {
                newtable.append(newrow)?;
                newrow = new_row(ctx, &span)?;
                seen_header_row = seen_header_row || seen_header;
            }
        } else if TABLE_ELEMS.contains(&tag) {
            let is_header = tag == "th" && !seen_header_row;
            seen_header = seen_header || is_header;
            let new_child_elem = ctx.element("cell")?;
            if is_header {
                new_child_elem.set("role", Some("head"))?;
            }
            if subelement.is_empty() {
                if let Some(cell) = process_node(env, subelement)? {
                    new_child_elem.set_text(cell.text().as_deref())?;
                    new_child_elem.set_tail(cell.tail().as_deref())?;
                }
            } else {
                new_child_elem.set_text(subelement.text().as_deref())?;
                new_child_elem.set_tail(subelement.tail().as_deref())?;
                subelement.set_tag("done")?;
                for child in subelement.iter_descendants(&[]) {
                    let ctag = child.tag();
                    let processed = if TABLE_ALL.contains(&ctag) {
                        if TABLE_ELEMS.contains(&ctag) {
                            child.set_tag("cell")?;
                        }
                        handle_textnode(env, child, true, true)?
                    } else if ctag == "list" && env.opts.focus == Focus::Recall {
                        if let Some(sub) = handle_lists(env, child)? {
                            new_child_elem.append(sub)?;
                        }
                        None
                    } else {
                        let mut tags = potential_tags.clone();
                        tags.insert("div");
                        handle_textelem(env, child, &tags)?
                    };
                    define_newelem(processed, new_child_elem)?;
                    child.set_tag("done")?;
                }
            }
            if truthy(&new_child_elem.text()) || !new_child_elem.is_empty() {
                newrow.append(new_child_elem)?;
            }
        } else if tag == "table" {
            break;
        }
        subelement.set_tag("done")?;
    }
    newrow.pop_attr("span");
    if !newrow.is_empty() {
        newtable.append(newrow)?;
    }
    Ok((!newtable.is_empty()).then_some(newtable))
}
