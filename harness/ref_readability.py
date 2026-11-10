"""Reference outputs of trafilatura 1.11's readability_lxml for the readability parity harness.
  ref_readability.py run docs.jsonl out.jsonl    per-doc sha1 of Document.summary() and of try_readability()
  ref_readability.py show docs.jsonl <i> [tr]    print one doc's full summary (or try_readability serialization)"""
import hashlib
import json
import sys

from lxml.etree import tostring
from trafilatura.external import try_readability
from trafilatura.readability_lxml import Document
from trafilatura.utils import load_html


def summary(html):
    tree = load_html(html)
    if tree is None:
        return None
    try:
        return Document(tree, min_text_length=25, retry_length=250).summary()
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        return f"EXC:{type(e).__name__}"


def readability_tree(html):
    tree = load_html(html)
    if tree is None:
        return None
    result = try_readability(tree)
    return None if result is None else tostring(result, encoding=str)


def sha1(s):
    # summaries always start with "<", so an exception marker stays readable
    if s is None or s.startswith("EXC:"):
        return s
    return hashlib.sha1(s.encode("utf-8", "surrogatepass")).hexdigest()


def docs(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            yield json.loads(line)


def main():
    import logging

    logging.disable(logging.CRITICAL)
    mode, path = sys.argv[1], sys.argv[2]
    if mode == "show":
        i = int(sys.argv[3])
        doc = next(d for d in docs(path) if d["i"] == i)
        s = readability_tree(doc["html"]) if sys.argv[4:] == ["tr"] else summary(doc["html"])
        sys.stdout.write(repr(s) if s is None else s)
        return
    with open(sys.argv[3], "w", encoding="utf-8") as out:
        for d in docs(path):
            rec = {"i": d["i"], "summary": sha1(summary(d["html"])), "tr": sha1(readability_tree(d["html"]))}
            out.write(json.dumps(rec) + "\n")


if __name__ == "__main__":
    main()
