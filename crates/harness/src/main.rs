//! Rust counterpart of harness/ref_stages.py (and ref2_stages.py for `*2` modes, see `main`).
//! usage: harness {stages|final|bench}[2] docs.jsonl out.jsonl [cfg]    harness show[2] docs.jsonl <i> <stage> [cfg]
mod v2;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, BufWriter, Write};
use trafilatura_rs::dom::{Ctx, Error, Method, Node};
use trafilatura_rs::trafilatura::baseline::baseline;
use trafilatura_rs::trafilatura::core::Extractor;
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::external::compare_extraction;
use trafilatura_rs::trafilatura::htmlprocessing::{prune_unwanted_nodes, tree_cleaning, Env};
use trafilatura_rs::trafilatura::htmlprocessing_convert::convert_tags;
use trafilatura_rs::trafilatura::main_extractor_extract::extract_content;
use trafilatura_rs::trafilatura::settings::{Focus, Options};
use trafilatura_rs::trafilatura::text::normalize_unicode;
use trafilatura_rs::trafilatura::xml::xmltotxt;
use trafilatura_rs::trafilatura::xpaths::REMOVE_COMMENTS_XPATH;

fn ser(n: Option<Node>) -> String {
    n.map_or_else(|| "None".to_owned(), |n| n.tostring(Method::Xml, true))
}

fn ser_triple((body, text, n): &(Node, String, usize)) -> String {
    format!("{}\x00{text}\x00{n}", ser(Some(*body)))
}

/// Python exception class name for "EXC:<name>".
fn exc_name(e: &Error) -> String {
    match e {
        Error::Value(_) => "ValueError".into(),
        Error::Type(_) => "TypeError".into(),
        Error::Parser(_) => "ParserError".into(),
        Error::Syntax(_) => "XMLSyntaxError".into(),
        Error::Runtime(s) => s.split(':').next().unwrap_or("RuntimeError").to_owned(),
    }
}

/// Mirrors `run_stages` in ref_stages.py; stages not yet ported are simply absent.
fn run_stages(html: &str) -> Vec<(&'static str, String)> {
    let ctx = Ctx::new();
    let mut out = Vec::new();
    let tree = trafilatura_rs::trafilatura::utils::load_html(&ctx, html);
    out.push(("load", ser(tree)));
    let Some(tree) = tree else { return out };
    let opts = Options { focus: Focus::Precision, comments: false, dedup: false, ..Options::default() };
    let mut lru = Lru::default();
    let mut stage = "clean";
    let result = (|| -> trafilatura_rs::dom::Result<()> {
        let tree_backup = tree.deep_copy(&ctx);
        let cleaned = tree_cleaning(&ctx, tree, &opts)?;
        out.push((stage, ser(Some(cleaned))));
        let cleaned_backup = cleaned.deep_copy(&ctx);
        stage = "convert";
        let cleaned = convert_tags(&ctx, cleaned, &opts)?;
        out.push((stage, ser(Some(cleaned))));
        stage = "prune_comments";
        let cleaned = prune_unwanted_nodes(&ctx, cleaned, &REMOVE_COMMENTS_XPATH, false)?;
        out.push((stage, ser(Some(cleaned))));
        stage = "content";
        let mut env = Env { ctx: &ctx, opts: &opts, lru: &mut lru };
        let content = extract_content(&mut env, cleaned)?;
        out.push((stage, ser_triple(&content)));
        stage = "txt_content";
        let txt = xmltotxt(content.0.deep_copy(&ctx), opts.formatting)?;
        out.push((stage, normalize_unicode(&txt)));
        stage = "baseline_raw";
        let b = baseline(&ctx, tree_backup.deep_copy(&ctx))?;
        out.push((stage, ser_triple(&b)));
        stage = "compare";
        let mut result = compare_extraction(&mut env, cleaned_backup, tree_backup.deep_copy(&ctx), content)?;
        out.push((stage, ser_triple(&result)));
        if result.2 < opts.min_extracted_size {
            stage = "baseline";
            result = baseline(&ctx, tree_backup.deep_copy(&ctx))?;
            out.push((stage, ser_triple(&result)));
        }
        stage = "txt";
        out.push((stage, normalize_unicode(&xmltotxt(result.0, opts.formatting)?)));
        Ok(())
    })();
    if let Err(e) = result {
        out.push((stage, format!("EXC:{}", exc_name(&e))));
    }
    out
}

/// Mirrors CONFIGS in harness/ref_stages.py (all with include_comments=False).
fn config(name: &str) -> Options {
    let base = Options { comments: false, ..Options::default() };
    match name {
        "fineweb" => Options::fineweb(),
        "balanced" => base,
        "recall" => Options { focus: Focus::Recall, ..base },
        "fast" => Options { fast: true, ..base },
        "notables" => Options { tables: false, ..base },
        _ => panic!("unknown config {name}"),
    }
}

fn sha1(s: &str) -> String {
    sha1_smol::Sha1::from(s.as_bytes()).digest().to_string()
}

fn docs(path: &str) -> impl Iterator<Item = (u64, String)> {
    let f = BufReader::new(std::fs::File::open(path).expect("docs file"));
    f.lines().map(|l| {
        let v: Value = serde_json::from_str(&l.expect("read")).expect("json");
        (v["i"].as_u64().expect("i"), v["html"].as_str().expect("html").to_owned())
    })
}

type Extract = Box<dyn FnMut(&str) -> trafilatura_rs::dom::Result<Option<String>>>;

/// `extract()` of trafilatura 1.11 (`v2` false) or 2.2.0 with the given options.
fn extractor(v2: bool, opts: Options) -> Extract {
    if v2 {
        let mut e = trafilatura_rs::trafilatura2::core::Extractor::new(opts);
        Box::new(move |h| e.extract(h))
    } else {
        let mut e = Extractor::new(opts);
        Box::new(move |h| e.extract(h))
    }
}

/// Modes with a `2` suffix (stages2, final2, show2, bench2) run the trafilatura 2.2.0 port; stages2/show2 take
/// the config as last argument (default balanced), like harness/ref2_stages.py.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, path) = (args[1].as_str(), args[2].as_str());
    let v2 = mode.ends_with('2');
    let mode = mode.trim_end_matches('2');
    let stages = |html: &str, cfg: Option<&String>| {
        if v2 {
            v2::run_stages(html, cfg.map_or("balanced", String::as_str))
        } else {
            run_stages(html)
        }
    };
    if mode == "show" {
        let (want_i, want_stage): (u64, &str) = (args[3].parse().expect("i"), &args[4]);
        let (_, html) = docs(path).find(|(i, _)| *i == want_i).expect("doc");
        for (stage, s) in stages(&html, args.get(5)) {
            if stage == want_stage {
                print!("{s}");
            }
        }
        return;
    }
    if mode == "bench" {
        let n: usize = args.get(3).map_or(usize::MAX, |s| s.parse().expect("n"));
        let pages: Vec<String> = docs(path).take(n).map(|(_, h)| h).collect();
        let mut extract = extractor(v2, Options::fineweb());
        let t = std::time::Instant::now();
        let kept = pages.iter().filter(|h| matches!(extract(h), Ok(Some(_)))).count();
        let secs = t.elapsed().as_secs_f64();
        println!(
            "docs={} kept={kept} extract={secs:.2}s = {:.2} ms/doc",
            pages.len(),
            1000.0 * secs / pages.len() as f64
        );
        return;
    }
    let mut out = BufWriter::new(std::fs::File::create(&args[3]).expect("out file"));
    let default_cfg = if v2 && mode == "stages" { "balanced" } else { "fineweb" };
    let cfg = args.get(4).map_or(default_cfg, String::as_str);
    let mut extract = extractor(v2, config(cfg));
    for (i, html) in docs(path) {
        let rec = match mode {
            "stages" => {
                let h: serde_json::Map<String, Value> = stages(&html, args.get(4))
                    .into_iter()
                    .map(|(st, s)| (st.to_owned(), Value::from(sha1(&s))))
                    .collect();
                json!({"i": i, "h": h})
            }
            "final" => {
                let text = match extract(&html) {
                    Ok(t) => t.map(Value::from).unwrap_or(Value::Null),
                    Err(e) => Value::from(format!("EXC:{}", exc_name(&e))),
                };
                json!({"i": i, "out": text})
            }
            _ => panic!("unknown mode {mode}"),
        };
        writeln!(out, "{rec}").expect("write");
    }
}
