//! trafilatura 2.2.0 htmlprocessing.py `convert_tags`. Unchanged converters (lists, headings, details) are 1.11's.
use super::settings::Options;
use super::xml::delete_element;
use crate::dom::{Ctx, Error, Node, Result};
use crate::trafilatura::htmlprocessing::truthy;
use crate::trafilatura::htmlprocessing_convert::{
    convert_details, convert_headings, convert_lists, CONVERSION_TAGS, REND_TAGS, REND_VALUES,
};

const CODE_INDICATORS: &[&str] = &["{", "(\"", "('", "\n    "];

/// `convert_quotes`: `pre` becomes `code` for a lone span child, hljs spans or code-like text.
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
        if elem.text().is_some_and(|t| CODE_INDICATORS.iter().any(|i| t.contains(i))) {
            code_flag = true;
        }
    }
    elem.set_tag(if code_flag { "code" } else { "quote" })
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
        let mut expr = String::from(".//*[self::div or self::li or self::p]//a");
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
    for elem in tree.xpath_nodes(r#".//strong[contains(@class, "schema-faq-question")]"#)? {
        elem.clear_attrs();
        elem.set("rend", Some("h3"))?;
        elem.set_tag("head")?;
    }
    for elem in tree.iter(&["sub", "sup"]) {
        if !truthy(&elem.text()) && elem.is_empty() {
            delete_element(ctx, elem, true)?;
        }
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
        // TODO: include_images (img → graphic, moving graphics out of links) is outside the ported configuration.
        unimplemented!("convert_tags with images=True");
    }
    Ok(tree)
}
