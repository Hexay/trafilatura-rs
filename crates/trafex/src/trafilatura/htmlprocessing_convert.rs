//! htmlprocessing.py `convert_tags` and the `convert_*` family (HTML markup to trafilatura's XML vocabulary).
use super::settings::Options;
use crate::dom::{Ctx, Error, Node, Result};

pub(crate) const REND_TAGS: &[&str] = &["em", "i", "b", "strong", "u", "kbd", "samp", "tt", "var", "sub", "sup"];
pub(crate) const REND_VALUES: &[&str] = &["#i", "#i", "#b", "#b", "#u", "#t", "#t", "#t", "#t", "#sub", "#sup"];
pub(crate) const CONVERSION_TAGS: &[&str] = &[
    "dl", "ol", "ul", "h1", "h2", "h3", "h4", "h5", "h6", "br", "hr", "blockquote", "pre", "q", "del", "s", "strike",
    "details",
];

/// `convert_lists`.
pub(crate) fn convert_lists(elem: Node) -> Result<()> {
    elem.set("rend", Some(elem.tag()))?;
    elem.set_tag("list")?;
    let mut i = 1;
    for subelem in elem.iter(&["dd", "dt", "li"]) {
        let tag = subelem.tag();
        if tag == "dd" || tag == "dt" {
            subelem.set("rend", Some(&format!("{tag}-{i}")))?;
            if tag == "dd" {
                i += 1;
            }
        }
        subelem.set_tag("item")?;
    }
    Ok(())
}

/// `convert_quotes`.
fn convert_quotes(elem: Node) -> Result<()> {
    let mut code_flag = false;
    if elem.tag() == "pre" {
        if elem.len() == 1 && elem.child(0).is_some_and(|c| c.tag() == "span") {
            code_flag = true;
        }
        let code_elems = elem.xpath_nodes(".//span[starts-with(@class,'hljs')]")?;
        if !code_elems.is_empty() {
            code_flag = true;
            for sub in code_elems {
                sub.clear_attrs();
            }
        }
    }
    elem.set_tag(if code_flag { "code" } else { "quote" })
}

/// `convert_headings`.
pub(crate) fn convert_headings(elem: Node) -> Result<()> {
    elem.clear_attrs();
    elem.set("rend", Some(elem.tag()))?;
    elem.set_tag("head")
}

/// `convert_details`.
pub(crate) fn convert_details(elem: Node) -> Result<()> {
    elem.set_tag("div")?;
    for sub in elem.iter(&["summary"]) {
        sub.set_tag("head")?;
    }
    Ok(())
}

fn convert(elem: Node) -> Result<()> {
    match elem.tag() {
        "dl" | "ol" | "ul" => convert_lists(elem),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => convert_headings(elem),
        "br" | "hr" => elem.set_tag("lb"),
        "blockquote" | "pre" | "q" => convert_quotes(elem),
        "del" | "s" | "strike" => {
            elem.set_tag("del")?;
            elem.set("rend", Some("overstrike"))
        }
        "details" => convert_details(elem),
        other => Err(Error::Runtime(format!("KeyError: {other:?}"))),
    }
}

/// `convert_tags` (url=None).
pub fn convert_tags(ctx: &Ctx, tree: Node, opts: &Options) -> Result<Node> {
    if !opts.links {
        let mut expr = String::from(".//div//a|.//ul//a");
        if opts.tables {
            expr += "|.//table//a";
        }
        for elem in tree.xpath_nodes(&expr)? {
            elem.set_tag("ref")?;
        }
        tree.strip_tags(ctx, &["a"]);
    } else {
        // TODO: include_links (href → target, relative URL fixing) is outside the ported configuration.
        unimplemented!("convert_tags with links=True");
    }
    if opts.formatting {
        for elem in tree.iter(REND_TAGS) {
            elem.clear_attrs();
            let i = REND_TAGS.iter().position(|&t| t == elem.tag());
            let rend = i.map(|i| REND_VALUES[i]).ok_or_else(|| Error::Runtime(format!("KeyError: {:?}", elem.tag())))?;
            elem.set("rend", Some(rend))?;
            elem.set_tag("hi")?;
        }
    } else {
        tree.strip_tags(ctx, REND_TAGS);
    }
    for elem in tree.iter(CONVERSION_TAGS) {
        convert(elem)?;
    }
    if opts.images {
        for elem in tree.iter(&["img"]) {
            elem.set_tag("graphic")?;
        }
    }
    Ok(tree)
}
