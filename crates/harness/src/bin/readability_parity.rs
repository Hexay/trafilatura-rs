//! Rust counterpart of harness/ref_readability.py.
//!   readability_parity run docs.jsonl out.jsonl
//!   readability_parity show docs.jsonl <i> [tr]
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, BufWriter, Write};
use trafex::dom::{Ctx, Error, Method};
use trafex::readability::{try_readability, Document};
use trafex::trafilatura::utils::load_html;

fn exc_name(e: &Error) -> String {
    let name = match e {
        Error::Value(_) => "ValueError",
        Error::Type(_) => "TypeError",
        Error::Parser(_) => "ParserError",
        Error::Syntax(_) => "XMLSyntaxError",
        Error::Runtime(m) => m.split(':').next().filter(|n| n.ends_with("Error")).unwrap_or("RuntimeError"),
    };
    format!("EXC:{name}")
}

fn summary(html: &str) -> Option<String> {
    let ctx = Ctx::new();
    let tree = load_html(&ctx, html)?;
    Some(Document::new(tree, 25, 250).summary(&ctx).unwrap_or_else(|e| exc_name(&e)))
}

fn readability_tree(html: &str) -> Option<String> {
    let ctx = Ctx::new();
    let tree = load_html(&ctx, html)?;
    try_readability(&ctx, tree).map(|n| n.tostring(Method::Xml, true))
}

fn sha1(s: Option<String>) -> Value {
    match s {
        None => Value::Null,
        Some(s) if s.starts_with("EXC:") => Value::from(s),
        Some(s) => Value::from(sha1_smol::Sha1::from(s.as_bytes()).digest().to_string()),
    }
}

fn docs(path: &str) -> impl Iterator<Item = (u64, String)> {
    let f = BufReader::new(std::fs::File::open(path).expect("docs file"));
    f.lines().map(|l| {
        let v: Value = serde_json::from_str(&l.expect("read")).expect("json");
        (v["i"].as_u64().expect("i"), v["html"].as_str().expect("html").to_owned())
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, path) = (args[1].as_str(), args[2].as_str());
    if mode == "show" {
        let want: u64 = args[3].parse().expect("i");
        let (_, html) = docs(path).find(|(i, _)| *i == want).expect("doc");
        let s = if args.get(4).map(String::as_str) == Some("tr") { readability_tree(&html) } else { summary(&html) };
        print!("{}", s.unwrap_or_else(|| "None".into()));
        return;
    }
    assert_eq!(mode, "run", "unknown mode {mode}");
    let mut out = BufWriter::new(std::fs::File::create(&args[3]).expect("out file"));
    for (i, html) in docs(path) {
        let rec = json!({"i": i, "summary": sha1(summary(&html)), "tr": sha1(readability_tree(&html))});
        writeln!(out, "{rec}").expect("write");
    }
}
