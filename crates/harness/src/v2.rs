//! Stage replay of trafilatura 2.2.0's `trafilatura_sequence`, mirroring `run_stages` in harness/ref2_stages.py.
use super::{config, exc_name, ser};
use trafilatura_rs::dom::{Ctx, Node};
use trafilatura_rs::py;
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::htmlprocessing::Env;
use trafilatura_rs::trafilatura::text::normalize_unicode;
use trafilatura_rs::trafilatura2::baseline::{baseline, html2txt};
use trafilatura_rs::trafilatura2::core::{escalate, escalation_due, forum_thread_page, prepare_tree};
use trafilatura_rs::trafilatura2::external::compare_extraction;
use trafilatura_rs::trafilatura2::htmlprocessing::prune_unwanted_nodes;
use trafilatura_rs::trafilatura2::main_extractor_extract::extract_content;
use trafilatura_rs::trafilatura2::settings::{Focus, Options};
use trafilatura_rs::trafilatura2::utils::load_html;
use trafilatura_rs::trafilatura2::xml::xmltotxt;
use trafilatura_rs::trafilatura2::xpaths::REMOVE_COMMENTS_XPATH;

fn triple((body, text, n): &(Node, String, usize)) -> String {
    format!("{}\x00{text}\x00{n}", ser(Some(*body)))
}

pub fn run_stages(html: &str, cfg: &str) -> Vec<(&'static str, String)> {
    let ctx = Ctx::new();
    let mut out = Vec::new();
    let tree = load_html(&ctx, html);
    out.push(("load", ser(tree)));
    let Some(mut tree) = tree else { return out };
    let opts = Options { comments: false, dedup: false, ..config(cfg) };
    let mut lru = Lru::default();
    let mut stage = "forum";
    let result = (|| -> trafilatura_rs::dom::Result<()> {
        let is_forum = forum_thread_page(tree);
        out.push((stage, if is_forum { "True" } else { "False" }.to_owned()));
        stage = "prune_raw";
        if opts.focus == Focus::Precision || !is_forum {
            tree = prune_unwanted_nodes(&ctx, tree.deep_copy(&ctx), &REMOVE_COMMENTS_XPATH, false)?;
        }
        out.push((stage, ser(Some(tree))));
        stage = "clean";
        let mut env = Env { ctx: &ctx, opts: &opts, lru: &mut lru };
        let (mut cleaned, backup) = prepare_tree(&env, tree)?;
        out.push((stage, ser(Some(backup))));
        stage = "convert";
        out.push((stage, ser(Some(cleaned))));
        stage = "prune_comments";
        if opts.focus == Focus::Precision && !is_forum {
            cleaned = prune_unwanted_nodes(&ctx, cleaned, &REMOVE_COMMENTS_XPATH, false)?;
        }
        out.push((stage, ser(Some(cleaned))));
        stage = "content";
        let mut result = extract_content(&mut env, cleaned)?;
        out.push((stage, triple(&result)));
        stage = "txt_content";
        out.push((stage, xmltotxt(&ctx, result.0.deep_copy(&ctx), opts.formatting)?));
        stage = "baseline_raw";
        out.push((stage, triple(&baseline(&ctx, tree)?)));
        stage = "html2txt";
        let page_text = html2txt(&ctx, tree)?;
        out.push((stage, page_text.clone()));
        if !opts.fast {
            stage = "compare";
            result = compare_extraction(&mut env, backup, tree.deep_copy(&ctx), result)?;
            out.push((stage, triple(&result)));
        }
        if result.2 < opts.min_extracted_size && opts.focus != Focus::Precision {
            stage = "baseline";
            result = baseline(&ctx, tree)?;
            out.push((stage, triple(&result)));
        }
        if escalation_due(&opts, result.2, py::len(&page_text)) {
            stage = "escalate";
            if let Some(better) = escalate(&mut env, tree, is_forum, result.2)? {
                result = better;
            }
            out.push((stage, triple(&result)));
        }
        stage = "txt";
        let text = xmltotxt(&ctx, result.0, opts.formatting)?;
        let comments = xmltotxt(&ctx, ctx.element("body")?, opts.formatting)?;
        out.push((stage, normalize_unicode(py::strip(&format!("{text}\n{comments}")))));
        Ok(())
    })();
    if let Err(e) = result {
        out.push((stage, format!("EXC:{}", exc_name(&e))));
    }
    out
}
