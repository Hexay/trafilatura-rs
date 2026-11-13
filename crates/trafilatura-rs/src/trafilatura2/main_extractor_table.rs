//! trafilatura 2.3.1 main_extractor.py `handle_table` (full-width rows with colspan/rowspan placeholders) and
//! `handle_image`.
use super::htmlprocessing::{handle_textnode, process_node};
use super::main_extractor::{define_newelem, handle_lists, handle_textelem, iter_children, PotentialTags};
use super::main_extractor_lists::append_block;
use super::settings::{Focus, INLINE_CONSUMING};
use super::utils::image_src;
use super::xml::separates_inline;
use crate::dom::{Ctx, Node, Result};
use crate::py::int::decimal_value;
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::links::{join, Urls};
use crate::trafilatura::text::text_chars_test;
use std::collections::{HashMap, HashSet};

const TABLE_ELEMS: &[&str] = &["td", "th"];
const MAX_SPAN: usize = 100;

fn define_cell_type(ctx: &Ctx, is_header: bool) -> Result<Node> {
    let cell = ctx.element("cell")?;
    if is_header {
        cell.set("role", Some("head"))?;
    }
    Ok(cell)
}

/// `_span`: col/rowspan when all-decimal (Python `str.isdecimal`), capped at MAX_SPAN, else 1.
fn span(cell: Node, attr: &str) -> usize {
    let value = cell.get(attr).unwrap_or_else(|| "1".into());
    if value.is_empty() || !value.chars().all(|c| decimal_value(c).is_some()) {
        return 1;
    }
    value
        .chars()
        .try_fold(0usize, |acc, c| {
            let v = acc * 10 + decimal_value(c).expect("decimal") as usize;
            if v >= MAX_SPAN {
                None
            } else {
                Some(v)
            }
        })
        .unwrap_or(MAX_SPAN)
}

/// `_flush_rowspan_phantoms`.
fn flush_rowspan_phantoms(ctx: &Ctx, rowspan_map: &mut HashMap<usize, usize>, newrow: Node) -> Result<()> {
    loop {
        let col = newrow.len();
        let Some(left) = rowspan_map.get_mut(&col) else { return Ok(()) };
        *left -= 1;
        if *left == 0 {
            rowspan_map.remove(&col);
        }
        newrow.append(define_cell_type(ctx, false)?)?;
    }
}

/// `_finalize_row`: trailing rowspan placeholders, padding to width, appended unless all cells are empty.
fn finalize_row(
    ctx: &Ctx,
    newtable: Node,
    newrow: Node,
    map: &mut HashMap<usize, usize>,
    max_cols: usize,
) -> Result<()> {
    flush_rowspan_phantoms(ctx, map, newrow)?;
    while newrow.len() < max_cols {
        newrow.append(define_cell_type(ctx, false)?)?;
    }
    if newrow.children().iter().any(|c| c.text().is_some_and(|t| !t.is_empty()) || !c.is_empty()) {
        newtable.append(newrow)?;
    }
    Ok(())
}

/// `_fill_cell`: the source cell's content into the new `cell`.
fn fill_cell(
    env: &mut Env,
    new: Node,
    cell: Node,
    nested: &HashSet<Node>,
    ptags_with_div: &PotentialTags,
) -> Result<()> {
    if cell.is_empty() {
        if let Some(pc) = process_node(env, cell)? {
            new.set_text(pc.text().as_deref())?;
            new.set_tail(pc.tail().as_deref())?;
        }
        return Ok(());
    }
    new.set_text(cell.text().as_deref())?;
    new.set_tail(cell.tail().as_deref())?;
    cell.set_tag("done")?;
    for child in cell.iter_descendants(&[]) {
        if !child.is_element() || child.tag() == "done" {
            continue;
        }
        if nested.contains(&child) {
            // nested tables are left to the main loop, their tail moves to the cell holding them
            let outermost =
                child.tag() == "table" && child.iter_ancestors(&["table"]).next().is_some_and(|t| !nested.contains(&t));
            if outermost {
                append_block(new, None, child)?;
                child.set_tail(None)?;
            }
            continue;
        }
        if separates_inline(child) && !text_chars_test(child.tail().as_deref()) {
            new.append(env.ctx.element("lb")?)?;
            child.set_tag("done")?;
            continue;
        }
        let tag = child.tag();
        let processed = if TABLE_ELEMS.contains(&tag) {
            child.set_tag("cell")?;
            handle_textnode(env, child, true, true)?
        } else if INLINE_CONSUMING.contains(&tag) {
            // inline wrappers without direct text (<ref><hi>x</hi></ref>) are carried whole
            match handle_textnode(env, child, true, true)? {
                None if !child.is_empty() => Some(child),
                processed => processed,
            }
        } else if tag == "list" {
            let processed = handle_lists(env, child)?;
            append_block(new, processed.filter(|_| env.opts.focus == Focus::Recall), child)?;
            continue;
        } else {
            handle_textelem(env, child, ptags_with_div)?
        };
        define_newelem(processed, new, true)?;
        child.set_tag("done")?;
    }
    Ok(())
}

/// `handle_table`.
pub fn handle_table(env: &mut Env, table_elem: Node, potential_tags: &PotentialTags) -> Result<Option<Node>> {
    let ctx = env.ctx;
    let newtable = ctx.element("table")?;
    let mut ptags_with_div = potential_tags.clone();
    ptags_with_div.insert("div");
    table_elem.strip_tags(ctx, &["thead", "tbody", "tfoot"]);
    let nested: HashSet<Node> = table_elem.iter_descendants(&["table"]).flat_map(|t| t.iter(&[])).collect();
    // direct children only, nested tables are left to the main loop; orphan cells join the current row
    let mut rows: Vec<Vec<Node>> = vec![Vec::new()];
    let mut captions: Vec<String> = Vec::new();
    let mut max_cols = 0;
    for elem in iter_children(table_elem) {
        let tag = elem.tag();
        if tag == "tr" {
            let cells: Vec<Node> = elem.children().into_iter().filter(|c| TABLE_ELEMS.contains(&c.tag())).collect();
            max_cols = max_cols.max(cells.iter().map(|&c| span(c, "colspan")).sum::<usize>());
            rows.push(cells);
        } else if TABLE_ELEMS.contains(&tag) {
            rows.last_mut().expect("first row").push(elem);
            continue;
        } else if tag == "caption" {
            captions.push(crate::py::strip(&elem.itertext(true).join(" ")).to_owned());
            for sub in elem.iter_descendants(&["*"]) {
                if sub.tag() == "graphic" {
                    sub.set_tail(None)?;
                } else {
                    sub.set_tag("done")?;
                }
            }
        } else if !elem.is_element() || tag == "table" {
            continue;
        }
        elem.set_tag("done")?;
    }
    let max_cols = max_cols.min(MAX_SPAN);
    for caption in captions.into_iter().filter(|c| !c.is_empty()) {
        let cell = define_cell_type(ctx, true)?;
        cell.set_text(Some(&caption))?;
        let row = ctx.element("row")?;
        row.append(cell)?;
        finalize_row(ctx, newtable, row, &mut HashMap::new(), max_cols)?;
    }
    let mut header_row_emitted = false;
    let mut rowspan_map: HashMap<usize, usize> = HashMap::new();
    for cells in rows {
        let newrow = ctx.element("row")?;
        let mut row_has_th = false;
        for cell in cells {
            let is_header = cell.tag() == "th" && !header_row_emitted;
            row_has_th = row_has_th || is_header;
            flush_rowspan_phantoms(ctx, &mut rowspan_map, newrow)?;
            let new_child_elem = define_cell_type(ctx, is_header)?;
            let colspan = span(cell, "colspan");
            let rowspan = span(cell, "rowspan");
            if rowspan > 1 {
                for c in newrow.len()..newrow.len() + colspan {
                    rowspan_map.insert(c, rowspan - 1);
                }
            }
            fill_cell(env, new_child_elem, cell, &nested, &ptags_with_div)?;
            newrow.append(new_child_elem)?;
            for _ in 1..colspan {
                newrow.append(define_cell_type(ctx, is_header)?)?;
            }
            cell.set_tag("done")?;
        }
        finalize_row(ctx, newtable, newrow, &mut rowspan_map, max_cols)?;
        header_row_emitted = header_row_emitted || row_has_th;
    }
    Ok((!newtable.is_empty()).then_some(newtable))
}

/// `handle_image`, relative sources joined to `options.url` (`urls.page`).
pub fn handle_image(ctx: &Ctx, urls: &Urls, element: Node) -> Result<Option<Node>> {
    let Some(link) = image_src(element) else { return Ok(None) };
    let link = match (urls.page, link.strip_prefix("//")) {
        _ if link.starts_with("http") => link,
        (Some(page), _) => join(urls.hooks, page, &link)?,
        (None, Some(rest)) => format!("http://{rest}"),
        (None, None) => link,
    };
    let processed = ctx.element(element.tag())?;
    processed.set("src", Some(&link))?;
    for attr in ["alt", "title"] {
        if let Some(v) = element.get(attr).filter(|v| !v.is_empty()) {
            processed.set(attr, Some(&v))?;
        }
    }
    processed.set_tail(element.tail().as_deref())?;
    Ok(Some(processed))
}
