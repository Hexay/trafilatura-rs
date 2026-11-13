//! Stage replay of trafilatura 2.3.1's `trafilatura_sequence`, mirroring `run_stages` in harness/ref2_stages.py.
use super::{config, exc_name, ser};
use trafilatura_rs::dom::{Ctx, Node};
use trafilatura_rs::py;
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::htmlprocessing::Env;
use trafilatura_rs::trafilatura::text::normalize_unicode;
use trafilatura_rs::trafilatura2::baseline::{baseline, html2txt};
use trafilatura_rs::trafilatura2::core::{
    baseline_rescue, escalate, escalation_due, forum_thread_page, prepare_tree, prune_raw_tree,
};
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

fn pair((body, text): &(Node, String)) -> String {
    format!("{}\x00{text}", ser(Some(*body)))
}

pub fn run_stages(html: &str, cfg: &str) -> Vec<(&'static str, String)> {
    let ctx = Ctx::new();
    let mut out = Vec::new();
    let tree = load_html(&ctx, html);
    out.push(("load", ser(tree)));
    let Some(tree) = tree else { return out };
    let opts = Options { comments: false, dedup: false, ..config(cfg) };
    let mut lru = Lru::default();
    let mut stage = "forum";
    let result = (|| -> trafilatura_rs::dom::Result<()> {
        let is_forum = forum_thread_page(tree);
        out.push((stage, if is_forum { "True" } else { "False" }.to_owned()));
        stage = "prune_raw";
        let mut env = Env { ctx: &ctx, opts: &opts, lru: &mut lru, urls: Default::default() };
        let tree = prune_raw_tree(&env, tree, is_forum)?;
        out.push((stage, ser(Some(tree))));
        stage = "clean";
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
        out.push((stage, pair(&result)));
        stage = "txt_content";
        out.push((stage, xmltotxt(&ctx, result.0.deep_copy(&ctx), false)?));
        stage = "baseline_raw";
        out.push((stage, triple(&baseline(&ctx, tree)?)));
        stage = "html2txt";
        let page_text = html2txt(&ctx, tree, true, true)?;
        assert_eq!(py::len(&page_text), trafilatura_rs::trafilatura2::baseline::html2txt_len(tree)?, "html2txt_len");
        out.push((stage, page_text.clone()));
        if !opts.fast {
            stage = "compare";
            result = compare_extraction(&mut env, backup, tree.deep_copy(&ctx), result)?;
            out.push((stage, pair(&result)));
        }
        if py::len(&result.1) < opts.min_extracted_size && opts.focus != Focus::Precision {
            stage = "baseline";
            result = baseline_rescue(&ctx, tree, result)?.0;
            out.push((stage, pair(&result)));
        }
        let len_text = py::len(&result.1);
        if escalation_due(&opts, len_text, py::len(&page_text)) {
            stage = "escalate";
            if let Some(better) = escalate(&mut env, tree, len_text)? {
                result = better;
            }
            out.push((stage, pair(&result)));
        }
        stage = "txt";
        let text = xmltotxt(&ctx, result.0, false)?;
        let comments = xmltotxt(&ctx, ctx.element("body")?, false)?;
        out.push((stage, normalize_unicode(py::strip(&format!("{text}\n{comments}")))));
        Ok(())
    })();
    if let Err(e) = result {
        out.push((stage, format!("EXC:{}", exc_name(&e))));
    }
    out
}
