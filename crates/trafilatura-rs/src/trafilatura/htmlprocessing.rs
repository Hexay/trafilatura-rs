//! trafilatura/htmlprocessing.py: tree cleaning, pruning, link density and text-node processing.
use super::deduplication::{duplicate_test, Lru};
use super::links::Urls;
use super::settings::{Focus, Options};
use super::text::{textfilter, trim};
use super::xml::delete_element;
use crate::dom::{Ctx, Error, Node, Result};

/// What trafilatura threads through as `options` (plus the dedup LRU Python keeps module-global).
pub struct Env<'a> {
    pub ctx: &'a Ctx,
    pub opts: &'a Options,
    pub lru: &'a mut Lru,
    pub urls: Urls<'a>,
}

pub(crate) fn truthy(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !s.is_empty())
}

/// `trim(x)` where x may be None.
pub(crate) fn trim_opt(s: Option<String>) -> Option<String> {
    s.map(|s| trim(&s))
}

pub(crate) fn attribute_error(what: &str) -> Error {
    Error::Runtime(format!("AttributeError: {what}"))
}

pub const CUT_EMPTY_ELEMS: &[&str] = &[
    "article",
    "b",
    "blockquote",
    "dd",
    "div",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "i",
    "li",
    "main",
    "p",
    "pre",
    "q",
    "section",
    "span",
    "strong",
];

pub const MANUALLY_CLEANED: &[&str] = &[
    "aside", "embed", "footer", "form", "head", "iframe", "menu", "object", "script", "applet", "audio", "canvas",
    "figure", "map", "picture", "svg", "video", "area", "blink", "button", "datalist", "dialog", "frame", "frameset",
    "fieldset", "link", "input", "ins", "label", "legend", "marquee", "math", "menuitem", "nav", "noscript",
    "optgroup", "option", "output", "param", "progress", "rp", "rt", "rtc", "select", "source", "style", "track",
    "textarea", "time", "use",
];

pub const MANUALLY_STRIPPED: &[&str] = &[
    "abbr", "acronym", "address", "bdi", "bdo", "big", "cite", "data", "dfn", "font", "hgroup", "img", "ins", "mark",
    "meta", "ruby", "small", "tbody", "template", "tfoot", "thead",
];

const PRESERVE_IMG_CLEANING: &[&str] = &["figure", "picture", "source"];

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
    }
    if opts.images {
        cleaning_list.retain(|e| !PRESERVE_IMG_CLEANING.contains(e));
        if let Some(i) = stripping_list.iter().position(|&t| t == "img") {
            stripping_list.remove(i);
        }
    }
    tree.strip_tags(ctx, &stripping_list);
    let tcopy = (opts.focus == Focus::Recall && tree.find(".//p").is_some()).then(|| tree.deep_copy(ctx));
    for expression in &cleaning_list {
        for element in tree.iter(&[expression]) {
            delete_element(ctx, element)?;
        }
    }
    let tree = match tcopy {
        Some(copy) if tree.find(".//p").is_none() => copy,
        _ => tree,
    };
    prune_html(ctx, tree)
}

/// `prune_html`.
pub fn prune_html(ctx: &Ctx, tree: Node) -> Result<Node> {
    for element in tree.xpath_nodes("//processing-instruction()|//*[not(node())]")? {
        if element.is_element() && CUT_EMPTY_ELEMS.contains(&element.tag()) {
            delete_element(ctx, element)?;
        }
    }
    Ok(tree)
}

/// `prune_unwanted_nodes`.
pub fn prune_unwanted_nodes(ctx: &Ctx, tree: Node, nodelist: &[&str], with_backup: bool) -> Result<Node> {
    let backup = with_backup.then(|| (tree.text_content_len(), tree.deep_copy(ctx)));
    for expression in nodelist {
        for subtree in tree.xpath_nodes(expression)? {
            prune_node(ctx, subtree)?;
        }
    }
    if let Some((old_len, backup)) = backup {
        let new_len = tree.text_content_len();
        return Ok(if new_len as f64 > old_len as f64 / 7.0 { tree } else { backup });
    }
    Ok(tree)
}

/// `prune_unwanted_nodes`' removal: the tail goes to the previous sibling's tail, else to the parent's tail.
pub fn prune_node(ctx: &Ctx, subtree: Node) -> Result<()> {
    if let Some(tail) = subtree.tail() {
        if let Some(prev) = subtree.prev().or_else(|| subtree.parent()) {
            prev.set_tail(Some(&format!("{} {}", prev.tail().unwrap_or_default(), tail)))?;
        }
    }
    subtree.parent().ok_or_else(|| attribute_error("'NoneType' object has no attribute 'remove'"))?.remove(ctx, subtree)
}

/// `collect_link_info`: (lengths, count, short elements). Callers only use the text list's size, so each link's
/// trimmed text is measured, not built.
pub fn collect_link_info(links: &[Node], favor_precision: bool) -> (usize, usize, usize) {
    let threshold = if favor_precision { 50 } else { 10 };
    let lens: Vec<usize> = links.iter().map(|l| l.text_content_trim_len()).filter(|&n| n > 0).collect();
    let shortelems = lens.iter().filter(|&&n| n < threshold).count();
    (lens.iter().sum(), lens.len(), shortelems)
}

/// `link_density_test(element, trim(element.text_content()))` -> (result, whether `mylist` is non-empty). Only the
/// text's length is used, and only when the element has links, so it is computed on demand into `elemlen`.
pub fn link_density_test(element: Node, elemlen: &mut Option<usize>, favor_precision: bool) -> (bool, bool) {
    let links = element.findall(".//ref");
    if !links.is_empty() {
        let limitlen = if element.tag() == "p" {
            match (favor_precision, element.next().is_none()) {
                (true, _) => 200,
                (false, true) => 60,
                (false, false) => 30,
            }
        } else if element.next().is_none() {
            300
        } else {
            100
        };
        let elemlen = *elemlen.get_or_insert_with(|| element.text_content_trim_len());
        if elemlen < limitlen {
            let (linklen, elemnum, shortelems) = collect_link_info(&links, favor_precision);
            if elemnum == 0 {
                return (true, false);
            }
            let dense =
                linklen as f64 > 0.8 * elemlen as f64 || (elemnum > 1 && shortelems as f64 / elemnum as f64 > 0.8);
            return (dense, true);
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
    if elemlen > 250 {
        let (linklen, elemnum, _) = collect_link_info(&links, false);
        if elemnum == 0 {
            return true;
        }
        let factor = if elemlen < 1000 { 0.8 } else { 0.5 };
        return linklen as f64 > factor * elemlen as f64;
    }
    false
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
    let threshold = if favor_precision { 200 } else { 100 };
    for elem in subtree.iter(&[tagname]) {
        let mut elemlen = None;
        let (result, has_links) = link_density_test(elem, &mut elemlen, favor_precision);
        if result {
            deletions.push(elem);
        } else if backtracking && has_links {
            let len = elemlen.expect("links are only counted after the length was computed");
            if 0 < len && len < threshold && elem.len() >= 3 {
                deletions.extend(elem.children());
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for elem in deletions {
        if seen.insert(elem) {
            delete_element(ctx, elem)?;
        }
    }
    Ok(subtree)
}

/// `handle_textnode`.
pub fn handle_textnode(env: &mut Env, elem: Node, comments_fix: bool, preserve_spaces: bool) -> Result<Option<Node>> {
    if elem.tag() == "done" || (elem.is_empty() && !truthy(&elem.text()) && !truthy(&elem.tail())) {
        return Ok(None);
    }
    if !comments_fix && elem.tag() == "lb" {
        if !preserve_spaces {
            elem.set_tail(trim_opt(elem.tail()).as_deref())?;
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
        elem.set_text(trim_opt(elem.text()).as_deref())?;
        if let Some(tail) = elem.tail().filter(|t| !t.is_empty()) {
            elem.set_tail(Some(&trim(&tail)))?;
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
    let (text, tail) = (trim_opt(elem.text()), trim_opt(elem.tail()));
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
