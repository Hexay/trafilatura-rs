"""Reference outputs from Python trafilatura 2.2.0 (.venv-t22) for the trafilatura2 parity harness.
  ref2_stages.py stages docs.jsonl out.jsonl [cfg]        per-doc sha1 of each trafilatura_sequence stage (dedup off)
  ref2_stages.py final docs.jsonl out.jsonl [cfg]         extract() as in ref_stages.py (doc order, dedup per cfg)
  ref2_stages.py show docs.jsonl <i> <stage> [cfg]        print one stage's full serialization
  ref2_stages.py stoplist out.txt                         define_stoplist(jt_stoplist_init()), one word per line
Stages replay core.trafilatura_sequence for include_comments=False (cfg default: balanced)."""
import json
import sys
from copy import copy, deepcopy

from ref_stages import CONFIGS, docs, ser, sha1

# txt_content, baseline_raw and html2txt are auxiliary (always computed) so baseline.py, xml.py and html2txt are
# verified even on pages where the pipeline never reaches them.
STAGES = ["load", "forum", "prune_raw", "clean", "convert", "prune_comments", "content", "txt_content",
          "baseline_raw", "html2txt", "compare", "baseline", "escalate", "txt"]


def options_for(cfg):
    from trafilatura.settings import Extractor

    kw = CONFIGS[cfg]
    return Extractor(output_format="txt", fast=kw.get("no_fallback", False), precision=kw.get("favor_precision", False),
                     recall=kw.get("favor_recall", False), comments=False, tables=kw.get("include_tables", True),
                     dedup=False)


def triple(t):
    return f"{ser(t[0])}\x00{t[1]}\x00{t[2]}"


def run_stages(html, cfg):
    """Yields (stage, serialized) until the pipeline stops; an exception becomes 'EXC:<type>'."""
    from lxml.etree import Element
    from trafilatura import core
    from trafilatura.baseline import baseline, html2txt
    from trafilatura.external import compare_extraction, justext_rescue
    from trafilatura.htmlprocessing import prune_unwanted_nodes
    from trafilatura.main_extractor import extract_content
    from trafilatura.utils import load_html, normalize_unicode
    from trafilatura.xml import xmltotxt
    from trafilatura.xpaths import REMOVE_COMMENTS_XPATH

    options = options_for(cfg)
    stage = "load"
    try:
        tree = load_html(html)
        yield stage, ser(tree)
        if tree is None:
            return
        stage = "forum"
        is_forum = core._forum_thread_page(tree)
        yield stage, repr(is_forum)
        stage = "prune_raw"
        if options.focus == "precision" or not is_forum:
            tree = prune_unwanted_nodes(copy(tree), REMOVE_COMMENTS_XPATH)
        yield stage, ser(tree)
        stage = "clean"
        cleaned_tree, cleaned_tree_backup = core._prepare_tree(tree, options, None)
        yield stage, ser(cleaned_tree_backup)
        stage = "convert"
        yield stage, ser(cleaned_tree)
        stage = "prune_comments"
        if options.focus == "precision" and not is_forum:
            cleaned_tree = prune_unwanted_nodes(cleaned_tree, REMOVE_COMMENTS_XPATH)
        yield stage, ser(cleaned_tree)
        stage = "content"
        result = extract_content(cleaned_tree, options)
        yield stage, triple(result)
        stage = "txt_content"
        yield stage, xmltotxt(deepcopy(result[0]), options.formatting)
        stage = "baseline_raw"
        yield stage, triple(baseline(tree))
        stage = "html2txt"
        page_text = html2txt(tree)
        yield stage, page_text
        if not options.fast:
            stage = "compare"
            result = compare_extraction(cleaned_tree_backup, copy(tree), *result, options)
            yield stage, triple(result)
        if result[2] < options.min_extracted_size and options.focus != "precision":
            stage = "baseline"
            result = baseline(tree)
            yield stage, triple(result)
        len_text = result[2]
        if options.focus == "balanced" and 0 < len_text < core.ESCALATION_MAX_LENGTH \
                and len_text < core.ESCALATION_PAGE_SHARE * len(page_text):
            stage = "escalate"
            r_options = copy(options)
            r_options.focus = "recall"
            esc_tree = tree if is_forum else prune_unwanted_nodes(copy(tree), REMOVE_COMMENTS_XPATH)
            r_len = j_len = 0
            try:
                r_body, r_text, r_len = core._recall_retry(esc_tree, r_options, None)
            except Exception:  # noqa: BLE001 - as in trafilatura_sequence
                pass
            if not options.fast:
                try:
                    j_body, j_text, j_len = justext_rescue(copy(esc_tree), options)
                except Exception:  # noqa: BLE001
                    pass
            if j_len > r_len and j_len > core.ESCALATION_JUSTEXT_RATIO * len_text:
                result = (j_body, j_text, j_len)
            elif r_len >= options.min_extracted_size and r_len > core.ESCALATION_ACCEPT_RATIO * len_text:
                result = (r_body, r_text, r_len)
            yield stage, triple(result)
        stage = "txt"
        text = f"{xmltotxt(result[0], options.formatting)}\n{xmltotxt(Element('body'), options.formatting)}".strip()
        yield stage, normalize_unicode(text)
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        yield stage, f"EXC:{type(e).__name__}"


def main():
    mode = sys.argv[1]
    if mode == "stoplist":
        from justext.core import define_stoplist
        from trafilatura.external import jt_stoplist_init

        with open(sys.argv[2], "w", encoding="utf-8") as out:
            out.write("\n".join(sorted(define_stoplist(jt_stoplist_init()))) + "\n")
        return
    path = sys.argv[2]
    if mode == "show":
        i, want = int(sys.argv[3]), sys.argv[4]
        cfg = sys.argv[5] if len(sys.argv) > 5 else "balanced"
        doc = next(d for d in docs(path) if d["i"] == i)
        for stage, s in run_stages(doc["html"], cfg):
            if stage == want:
                sys.stdout.write(s)
        return
    with open(sys.argv[3], "w", encoding="utf-8") as out:
        if mode == "stages":
            cfg = sys.argv[4] if len(sys.argv) > 4 else "balanced"
            for d in docs(path):
                out.write(json.dumps({"i": d["i"], "h": {st: sha1(s) for st, s in run_stages(d["html"], cfg)}}) + "\n")
        elif mode == "final":
            from trafilatura import extract

            kwargs = CONFIGS[sys.argv[4] if len(sys.argv) > 4 else "fineweb"]
            for d in docs(path):
                try:
                    text = extract(d["html"], **kwargs)
                except Exception as e:  # noqa: BLE001
                    text = f"EXC:{type(e).__name__}"
                out.write(json.dumps({"i": d["i"], "out": text}) + "\n")


if __name__ == "__main__":
    main()
