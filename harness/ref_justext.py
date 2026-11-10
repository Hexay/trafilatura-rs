"""Reference jusText paragraphs (trafilatura.external.custom_justext with JT_STOPLIST) for the parity harness.
  ref_justext.py run docs.jsonl out.jsonl          per-doc sha1 of the paragraph dump, None or EXC:<type>
  ref_justext.py show docs.jsonl <i>               print one doc's paragraph dump
  ref_justext.py stoplist out.txt                  export define_stoplist(JT_STOPLIST), sorted, one word per line"""
import hashlib
import json
import sys

FIELDS = {"dom_path", "xpath", "text_nodes", "chars_count_in_links", "tags_count", "class_type", "cf_class", "heading"}


def esc(s):
    return s.replace("\\", "\\\\").replace("\n", "\\n")


def dump(paragraphs, stoplist):
    """Canonical dump; crates/justext/src/bin/justext_parity.rs writes the same bytes."""
    out = []
    for p in paragraphs:
        assert set(vars(p)) == FIELDS, vars(p).keys()
        out.append("P")
        for k in ("dom_path", "xpath", "text", "class_type", "cf_class"):
            out.append(f"{k}={esc(getattr(p, k))}")
        out.append(f"heading={p.heading} tags={p.tags_count} links={p.chars_count_in_links} len={len(p)} "
                   f"words={p.words_count} stop={p.stopwords_count(stoplist)} boiler={p.is_boilerplate}")
        out.extend("node=" + esc(t) for t in p.text_nodes)
    return "".join(line + "\n" for line in out)


def run(html):
    from justext.core import define_stoplist
    from trafilatura.external import custom_justext, jt_stoplist_init
    from trafilatura.utils import load_html

    stoplist = jt_stoplist_init() if run.stoplist is None else run.stoplist
    run.stoplist = stoplist
    try:
        tree = load_html(html)
        if tree is None:
            return None
        return dump(custom_justext(tree, stoplist), define_stoplist(stoplist))
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        return f"EXC:{type(e).__name__}"


run.stoplist = None


def docs(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            yield json.loads(line)


def digest(s):
    if s is None or s.startswith("EXC:"):
        return s
    return hashlib.sha1(s.encode("utf-8", "surrogatepass")).hexdigest()


def main():
    mode = sys.argv[1]
    if mode == "stoplist":
        from justext.core import define_stoplist
        from trafilatura.external import jt_stoplist_init

        with open(sys.argv[2], "w", encoding="utf-8", newline="\n") as f:
            f.writelines(w + "\n" for w in sorted(define_stoplist(jt_stoplist_init())))
    elif mode == "show":
        doc = next(d for d in docs(sys.argv[2]) if d["i"] == int(sys.argv[3]))
        sys.stdout.write(str(run(doc["html"])))
    elif mode == "run":
        with open(sys.argv[3], "w", encoding="utf-8") as out:
            for d in docs(sys.argv[2]):
                out.write(json.dumps({"i": d["i"], "h": digest(run(d["html"]))}) + "\n")


if __name__ == "__main__":
    main()
