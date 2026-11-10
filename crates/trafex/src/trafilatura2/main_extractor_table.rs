//! trafilatura 2.2.0 main_extractor.py `handle_table` (full-width rows with colspan/rowspan placeholders) and
//! `handle_image`.
use super::htmlprocessing::{handle_textnode, process_node};
use super::main_extractor::{define_newelem, handle_lists, handle_textelem, iter_children, PotentialTags};
use super::settings::Focus;
use super::utils::is_image_file;
use crate::dom::{Ctx, Node, Result};
use crate::trafilatura::htmlprocessing::Env;
use crate::trafilatura::main_extractor_table::decimal_value;
use std::collections::{HashMap, HashSet};

const TABLE_ELEMS: &[&str] = &["td", "th"];
const INLINE_WRAP_TAGS: &[&str] = &["hi", "ref", "del"];
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
    value.chars().try_fold(0usize, |acc, c| {
        let v = acc * 10 + decimal_value(c).expect("decimal") as usize;
        if v >= MAX_SPAN { None } else { Some(v) }
    }).unwrap_or(MAX_SPAN)
}

fn row_has_content(row: Node) -> bool {
    row.children().iter().any(|c| c.text().is_some_and(|t| !t.is_empty()) || !c.is_empty())
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

/// `_finalize_row`.
fn finalize_row(ctx: &Ctx, newtable: Node, newrow: Node, map: &mut HashMap<usize, usize>, max_cols: usize) -> Result<()> {
    flush_rowspan_phantoms(ctx, map, newrow)?;
    while newrow.len() < max_cols {
        newrow.append(define_cell_type(ctx, false)?)?;
    }
    if row_has_content(newrow) {
        newtable.append(newrow)?;
    }
    Ok(())
}

/// `_fill_cell`: the source cell's content into the new `cell`.
fn fill_cell(env: &mut Env, new: Node, cell: Node, nested: &HashSet<Node>, ptags_with_div: &PotentialTags) -> Result<()> {
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
            if let Some(tail) = child.tail().filter(|t| child.tag() == "table" && !t.is_empty()) {
                match new.child(-1) {
                    Some(last) => last.set_tail(Some(&(last.tail().unwrap_or_default() + &tail)))?,
                    None => new.set_text(Some(&(new.text().unwrap_or_default() + &tail)))?,
                }
            }
            continue;
        }
        let tag = child.tag();
        let processed = if TABLE_ELEMS.contains(&tag) {
            child.set_tag("cell")?;
            handle_textnode(env, child, true, true)?
        } else if INLINE_WRAP_TAGS.contains(&tag) {
            let processed = handle_textnode(env, child, true, true)?;
            if processed.is_none() && !child.is_empty() {
                define_newelem(Some(child), new, true)?;
                for el in child.iter(&["*"]) {
                    el.set_tag("done")?;
                }
                continue;
            }
            processed
        } else if tag == "list" && env.opts.focus == Focus::Recall {
            if let Some(sub) = handle_lists(env, child)? {
                new.append(sub)?;
            }
            child.set_tag("done")?;
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
    let col_counts = table_elem.findall("tr").into_iter().map(|tr| {
        tr.children().into_iter().filter(|td| TABLE_ELEMS.contains(&td.tag())).map(|td| span(td, "colspan")).sum::<usize>()
    });
    let max_cols = col_counts.max().unwrap_or(0).min(MAX_SPAN);
    for caption in table_elem.findall("caption") {
        let text = crate::py::strip(&caption.itertext(true).join(" ")).to_owned();
        if !text.is_empty() {
            let row = ctx.element("row")?;
            let cell = define_cell_type(ctx, true)?;
            cell.set_text(Some(&text))?;
            row.append(cell)?;
            while row.len() < max_cols {
                row.append(define_cell_type(ctx, false)?)?;
            }
            newtable.append(row)?;
        }
        caption.set_tag("done")?;
    }
    let (mut header_row_emitted, mut row_has_th) = (false, false);
    let mut newrow = ctx.element("row")?;
    let mut rowspan_map: HashMap<usize, usize> = HashMap::new();
    for elem in iter_children(table_elem) {
        if !elem.is_element() {
            continue;
        }
        let cells: Vec<Node> = if elem.tag() == "tr" {
            if !newrow.is_empty() {
                finalize_row(ctx, newtable, newrow, &mut rowspan_map, max_cols)?;
                header_row_emitted = header_row_emitted || row_has_th;
            }
            newrow = ctx.element("row")?;
            row_has_th = false;
            flush_rowspan_phantoms(ctx, &mut rowspan_map, newrow)?;
            iter_children(elem).collect()
        } else if TABLE_ELEMS.contains(&elem.tag()) {
            vec![elem]
        } else {
            if elem.tag() != "table" {
                elem.set_tag("done")?;
            }
            continue;
        };
        for cell in cells {
            if !cell.is_element() || !TABLE_ELEMS.contains(&cell.tag()) {
                continue;
            }
            let is_header = cell.tag() == "th" && !header_row_emitted;
            row_has_th = row_has_th || is_header;
            flush_rowspan_phantoms(ctx, &mut rowspan_map, newrow)?;
            let new_child_elem = define_cell_type(ctx, is_header)?;
            let colspan = span(cell, "colspan");
            let rows = span(cell, "rowspan");
            if rows > 1 {
                for c in newrow.len()..newrow.len() + colspan {
                    rowspan_map.insert(c, rows - 1);
                }
            }
            fill_cell(env, new_child_elem, cell, &nested, &ptags_with_div)?;
            newrow.append(new_child_elem)?;
            for _ in 1..colspan {
                newrow.append(define_cell_type(ctx, is_header)?)?;
            }
            cell.set_tag("done")?;
        }
        elem.set_tag("done")?;
    }
    finalize_row(ctx, newtable, newrow, &mut rowspan_map, max_cols)?;
    Ok((!newtable.is_empty()).then_some(newtable))
}

/// `handle_image` (options.url is None in the ported configuration).
pub fn handle_image(ctx: &Ctx, element: Option<Node>) -> Result<Option<Node>> {
    let Some(element) = element else { return Ok(None) };
    let processed = ctx.element(element.tag())?;
    let direct = ["data-src", "src"].iter().map(|a| element.get(a).unwrap_or_default()).find(|s| is_image_file(Some(s)));
    let src = direct.or_else(|| {
        element.attrs().into_iter().find(|(k, v)| k.starts_with("data-src") && is_image_file(Some(v))).map(|(_, v)| v)
    });
    if let Some(src) = src {
        processed.set("src", Some(&src))?;
    }
    for attr in ["alt", "title"] {
        if let Some(v) = element.get(attr).filter(|v| !v.is_empty()) {
            processed.set(attr, Some(&v))?;
        }
    }
    let link = processed.get("src").unwrap_or_default();
    if !processed.has_attrs() || link.is_empty() {
        return Ok(None);
    }
    if !link.starts_with("http") {
        let fixed = match link.strip_prefix("//") {
            Some(rest) => format!("http://{rest}"),
            None => link,
        };
        processed.set("src", Some(&fixed))?;
    }
    processed.set_tail(element.tail().as_deref())?;
    Ok(Some(processed))
}
