"""Pick the parity-test fixtures (tests/fixtures/pages.jsonl) from the corpus: ~40 varied pages under 2 MB.
usage: make_fixtures.py $TRAFILATURA_RS_DATA > tests/fixtures/pages.jsonl
Buckets cover each PEP-393 parser path, XML declarations, tables, empty results, config-sensitive pages, pages whose
output changes with include_comments, JSON-LD metadata, markdown formatting and a same-host run (exercises
deduplicate=True). Needs ref_final_{fineweb,balanced,recall,fineweb_comments,balanced_comments}.jsonl and
ref_meta_balanced_md.jsonl (tools/make_reference.sh).
"""
import json
import re
import sys
from collections import defaultdict
from urllib.parse import urlsplit

MAX_DOC = 60_000
XML_DECL = re.compile(r"^\s*<\?xml[^>]+encoding", re.I)


def kind(html):
    top = max(map(ord, html), default=0)
    if top < 0x80:
        return "ascii"
    if top < 0x100:
        return "latin1_c1" if re.search("[\x80-\x9f]", html) else "latin1"
    return "bmp" if top < 0x10000 else "astral"


def rows(data):
    files = [open(f"{data}/{n}.jsonl", encoding="utf-8") for n in
             ("docs", "ref_final_fineweb", "ref_final_balanced", "ref_final_recall", "ref_final_fineweb_comments",
              "ref_final_balanced_comments", "ref_meta_balanced_md")]
    for lines in zip(*files):
        doc, *outs = (json.loads(x) for x in lines)
        yield doc, *(o["out"] for o in outs)


def main():
    quota = {"astral": 3, "bmp": 4, "latin1": 2, "latin1_c1": 1, "xmldecl": 2, "table": 3, "none": 3,
             "config": 4, "ascii": 4, "comments": 3, "comments_precision": 2, "jsonld": 3, "markdown": 2}
    picked, hosts = {}, defaultdict(list)
    for doc, fw, bal, rec, fw_c, bal_c, md in rows(sys.argv[1]):
        html = doc["html"]
        if len(html.encode("utf-8", "surrogatepass")) > MAX_DOC:
            continue
        hosts[urlsplit(doc["url"]).hostname].append(doc)
        tags = [kind(html)]
        if XML_DECL.match(html):
            tags.append("xmldecl")
        if "<table" in html and fw:
            tags.append("table")
        if fw is None:
            tags.append("none")
        if fw != bal and bal != rec:
            tags.append("config")
        if bal != bal_c:
            tags.append("comments")
        if fw != fw_c:
            tags.append("comments_precision")
        if "application/ld+json" in html:
            tags.append("jsonld")
        if md != bal:
            tags.append("markdown")
        for tag in reversed(tags):
            if quota.get(tag, 0) > 0 and doc["i"] not in picked:
                quota[tag] -= 1
                picked[doc["i"]] = dict(doc, why=tag)
    run = next(docs for docs in hosts.values() if len(docs) >= 4)[:4]
    for doc in run:
        picked.setdefault(doc["i"], dict(doc, why="same_host"))
    astral = next(d for d in picked.values() if d["why"] == "astral")
    # A leading BOM keeps lxml's UCS-4 buffer, the only path through iconv.
    synthetic = dict(astral, i=-1, html="﻿" + astral["html"], why="bom_astral")
    for doc in [*sorted(picked.values(), key=lambda d: d["i"]), synthetic]:
        sys.stdout.write(json.dumps({k: doc[k] for k in ("i", "url", "why", "html")}) + "\n")
    print(f"unfilled quotas: {[k for k, v in quota.items() if v]}", file=sys.stderr)


if __name__ == "__main__":
    main()
