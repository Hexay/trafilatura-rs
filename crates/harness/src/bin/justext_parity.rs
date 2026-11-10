//! Rust counterpart of harness/ref_justext.py. usage: justext_parity run docs.jsonl out.jsonl
//!                                                    justext_parity show docs.jsonl <i>
use trafex::justext::{custom_justext_full as custom_justext, Paragraph, Stoplist};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, BufWriter, Write};
use trafex::dom::{Ctx, Error};

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn py_bool(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

/// Same bytes as `dump` in ref_justext.py.
fn dump(paragraphs: &[Paragraph], stoplist: &Stoplist) -> String {
    let mut out = String::new();
    for p in paragraphs {
        out.push_str("P\n");
        let fields = [
            ("dom_path", p.dom_path.as_str()),
            ("xpath", &p.xpath),
            ("text", &p.text),
            ("class_type", p.class_type.as_str()),
            ("cf_class", p.cf_class.as_str()),
        ];
        for (k, v) in fields {
            out.push_str(&format!("{k}={}\n", esc(v)));
        }
        out.push_str(&format!(
            "heading={} tags={} links={} len={} words={} stop={} boiler={}\n",
            py_bool(p.heading),
            p.tags_count,
            p.chars_count_in_links,
            p.length,
            p.words_count(),
            p.stopwords_count(stoplist),
            py_bool(p.is_boilerplate())
        ));
        for t in &p.text_nodes {
            out.push_str(&format!("node={}\n", esc(t)));
        }
    }
    out
}

fn exc_name(e: &Error) -> &'static str {
    match e {
        Error::Value(_) => "ValueError",
        Error::Type(_) => "TypeError",
        Error::Parser(_) => "ParserError",
        Error::Syntax(_) => "XMLSyntaxError",
        Error::Runtime(_) => "RuntimeError",
    }
}

/// None when load_html rejects the page; `Err` carries "EXC:<type>".
fn run(html: &str) -> Option<Result<String, String>> {
    let ctx = Ctx::new();
    let tree = trafex::trafilatura::utils::load_html(&ctx, html)?;
    let stoplist = Stoplist::jt();
    Some(custom_justext(tree, stoplist).map(|ps| dump(&ps, stoplist)).map_err(|e| format!("EXC:{}", exc_name(&e))))
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
    match mode {
        "show" => {
            let want: u64 = args[3].parse().expect("i");
            let (_, html) = docs(path).find(|(i, _)| *i == want).expect("doc");
            match run(&html) {
                None => print!("None"),
                Some(Ok(s) | Err(s)) => print!("{s}"),
            }
        }
        "run" => {
            let mut out = BufWriter::new(std::fs::File::create(&args[3]).expect("out file"));
            for (i, html) in docs(path) {
                let h = match run(&html) {
                    None => Value::Null,
                    Some(Ok(s)) => Value::from(sha1_smol::Sha1::from(s.as_bytes()).digest().to_string()),
                    Some(Err(e)) => Value::from(e),
                };
                writeln!(out, "{}", json!({"i": i, "h": h})).expect("write");
            }
        }
        _ => panic!("unknown mode {mode}"),
    }
}
