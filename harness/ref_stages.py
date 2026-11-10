"""Reference outputs from Python trafilatura 1.11 for the parity harness.
  ref_stages.py stages docs.jsonl out.jsonl     per-doc sha1 of each bare_extraction stage (dedup off)
  ref_stages.py final docs.jsonl out.jsonl      extract() exactly as datatrove calls it (dedup on, doc order)
  ref_stages.py show docs.jsonl <i> <stage>     print one stage's full serialization
Stages follow trafilatura/core.py bare_extraction for favor_precision=True, include_comments=False, txt output."""
import hashlib
import json
import sys
from copy import copy, deepcopy

# txt_content and baseline_raw are auxiliary (not in bare_extraction's flow): xmltotxt right after extract_content
# and an unconditional baseline(), so xml.py and baseline.py can be verified before compare_extraction is ported.
STAGES = ["load", "clean", "convert", "prune_comments", "content", "txt_content", "baseline_raw", "compare",
          "baseline", "txt"]


# extract() keyword sets checked in final mode (include_comments=False always); mirrored in crates/harness.
CONFIGS = {
    "fineweb": {"favor_precision": True, "deduplicate": True},
    "balanced": {},
    "recall": {"favor_recall": True},
    "fast": {"no_fallback": True},
    "notables": {"include_tables": False},
}


def ser(obj):
    from lxml.etree import _Element, tostring

    if isinstance(obj, _Element):
        return tostring(obj, encoding=str)
    return repr(obj)


def run_stages(html):
    """Yields (stage, serialized) until the pipeline stops; an exception becomes 'EXC:<type>'."""
    from trafilatura.baseline import baseline
    from trafilatura.external import compare_extraction
    from trafilatura.htmlprocessing import convert_tags, prune_unwanted_nodes, tree_cleaning
    from trafilatura.main_extractor import extract_content
    from trafilatura.settings import Extractor
    from trafilatura.utils import load_html, normalize_unicode
    from trafilatura.xml import xmltotxt
    from trafilatura.xpaths import REMOVE_COMMENTS_XPATH

    stage = "load"
    try:
        tree = load_html(html)
        yield stage, ser(tree)
        if tree is None:
            return
        options = Extractor(output_format="txt", precision=True, comments=False, dedup=False)
        tree_backup = copy(tree)
        stage = "clean"
        cleaned = tree_cleaning(tree, options)
        yield stage, ser(cleaned)
        cleaned_backup = copy(cleaned)
        stage = "convert"
        cleaned = convert_tags(cleaned, options, None)
        yield stage, ser(cleaned)
        stage = "prune_comments"
        cleaned = prune_unwanted_nodes(cleaned, REMOVE_COMMENTS_XPATH)
        yield stage, ser(cleaned)
        stage = "content"
        body, text, n = extract_content(cleaned, options)
        yield stage, f"{ser(body)}\x00{text}\x00{n}"
        stage = "txt_content"
        yield stage, normalize_unicode(xmltotxt(deepcopy(body), options.formatting))
        stage = "baseline_raw"
        b_body, b_text, b_n = baseline(deepcopy(tree_backup))
        yield stage, f"{ser(b_body)}\x00{b_text}\x00{b_n}"
        stage = "compare"
        body, text, n = compare_extraction(cleaned_backup, deepcopy(tree_backup), body, text, n, options)
        yield stage, f"{ser(body)}\x00{text}\x00{n}"
        if n < options.min_extracted_size:
            stage = "baseline"
            body, text, n = baseline(deepcopy(tree_backup))
            yield stage, f"{ser(body)}\x00{text}\x00{n}"
        stage = "txt"
        yield stage, normalize_unicode(xmltotxt(body, options.formatting))
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        yield stage, f"EXC:{type(e).__name__}"


def docs(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            yield json.loads(line)


def sha1(s):
    return hashlib.sha1(s.encode("utf-8", "surrogatepass")).hexdigest()


def main():
    mode, path = sys.argv[1], sys.argv[2]
    if mode == "show":
        i, want = int(sys.argv[3]), sys.argv[4]
        doc = next(d for d in docs(path) if d["i"] == i)
        for stage, s in run_stages(doc["html"]):
            if stage == want:
                sys.stdout.write(s)
        return
    with open(sys.argv[3], "w", encoding="utf-8") as out:
        if mode == "stages":
            for d in docs(path):
                out.write(json.dumps({"i": d["i"], "h": {st: sha1(s) for st, s in run_stages(d["html"])}}) + "\n")
        elif mode == "final":
            from trafilatura import extract

            kwargs = CONFIGS[sys.argv[4] if len(sys.argv) > 4 else "fineweb"]
            for d in docs(path):
                try:
                    text = extract(d["html"], include_comments=False, **kwargs)
                except Exception as e:  # noqa: BLE001
                    text = f"EXC:{type(e).__name__}"
                out.write(json.dumps({"i": d["i"], "out": text}) + "\n")


if __name__ == "__main__":
    main()
