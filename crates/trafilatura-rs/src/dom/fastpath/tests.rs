use super::{plan, plan_first, Plan};
use crate::dom::xpath::{XItem, XValue};
use crate::dom::{Ctx, Node};
use crate::trafilatura::utils::load_html;

const HTML: &str = r#"<html><head><title>t</title><meta property="og:url" content="u"><link rel="canonical" href="c">
<meta name="x" content="y"></head><body><div class="tags"><a href="/t/1">one</a><span><a href="/t/2">two</a></span>
<a>none</a></div><div class="post-meta"><div class="tags"><a href="/t/3">x</a></div></div>
<p class="author">A <rel>me</rel></p><span rel="author">B</span><author>C</author><footer><a href="l">l</a></footer>
<div class="footer-x"><p><a href="f">f</a></p></div><time>1</time><figure>2</figure></body></html>"#;

const EXPRS: &[&str] = &[
    r#"//div[@class="tags"]//a[@href]"#,
    r#"//div[starts-with(@class, 'post-meta') or @class="tags"]//a[@href]"#,
    r#".//head/meta[@property="og:url"][@content]"#,
    ".//head//meta[@content]",
    r#".//head//link[@rel="canonical"][@href]"#,
    r#"//*[(self::a or self::p or self::span)][@rel="author" or rel="me"]|//author"#,
    r#".//footer//a[@href]|.//div[contains(@class, "footer")]//a[@href]"#,
    "//time|//figure",
    "//body/div/div/a",
    "//html",
];

fn libxml2(node: Node, expr: &str) -> Vec<Node> {
    match node.xpath(expr).expect("valid") {
        XValue::Nodes(items) => {
            items.into_iter().filter_map(|i| if let XItem::Node(n) = i { Some(n) } else { None }).collect()
        }
        _ => panic!("not a node-set"),
    }
}

#[test]
fn paths_match_libxml2() {
    let ctx = Ctx::new();
    let tree = load_html(&ctx, HTML).expect("parses");
    let body = tree.xpath_first(".//body").expect("valid").expect("a body");
    for node in [tree, body, tree.deep_copy(&ctx), body.deep_copy(&ctx)] {
        for expr in EXPRS {
            let p = plan(expr).unwrap_or_else(|| panic!("no plan for {expr}"));
            assert!(matches!(p, Plan::Paths { .. }), "{expr}");
            let lib = libxml2(node, expr);
            assert_eq!(p.nodes(node), lib, "{expr} from {}", node.tag());
            let first = plan_first(expr).expect("a first plan");
            assert_eq!(first.first(node), lib.first().copied(), "{expr}");
        }
    }
}

/// Every expression has a native plan.
pub(crate) fn assert_planned(exprs: Vec<&str>) {
    let missing: Vec<&str> = exprs.into_iter().filter(|e| plan(e).is_none()).collect();
    assert!(missing.is_empty(), "left to libxml2: {missing:#?}");
}

#[test]
fn unsupported_paths_stay_on_libxml2() {
    for expr in ["//a/@href", "//p/text()", "//div[1]//a", ".//a|//b", "//a/..", "//*[name()='a']"] {
        assert!(plan(expr).is_none(), "{expr}");
    }
}
