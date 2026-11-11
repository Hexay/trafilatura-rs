"""htmldate parity: htmldate.find_date on trafilatura's parsed tree vs trafilatura_rs._core._find_date, both with
original_date=True, extensive_search=True, max_date=PINNED_DAY, once with url=None and once with the record's URL.
  ref_htmldate.py ref [docs.jsonl]      compute the Python references (cached, see REF_PATHS)
  ref_htmldate.py rust [docs.jsonl]     compare the Rust port against the cached references
  ref_htmldate.py show <i> [docs.jsonl] print both outputs of one doc
REF_JOBS sets the worker count (default 4). Exceptions compare as "EXC:<type>"."""
import json
import os
import sys
from multiprocessing import Pool

from ref_meta import PINNED_DAY, docs, pin_wall_clock

DATA = os.environ.get("TRAFILATURA_RS_DATA", "/srv/agents/cache/trafilatura-rs-data")
VARIANTS = ("nourl", "url")
REF_PATHS = {v: f"{DATA}/ref_meta_htmldate_{v}.jsonl" for v in VARIANTS}


def python_find_date(html, url):
    from htmldate import find_date
    from trafilatura.utils import load_html

    tree = load_html(html)
    if tree is None:  # trafilatura stops before metadata; _find_date returns None too
        return None
    return find_date(tree, original_date=True, extensive_search=True, max_date=PINNED_DAY, url=url)


def rust_find_date(html, url):
    import trafilatura_rs

    return trafilatura_rs._core._find_date(html, PINNED_DAY, url)


ENGINES = {"python": python_find_date, "rust": rust_find_date}


def run(engine, doc):
    out = {}
    for variant in VARIANTS:
        url = doc["url"] if variant == "url" else None
        try:
            out[variant] = ENGINES[engine](doc["html"], url)
        except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
            out[variant] = f"EXC:{type(e).__name__}"
    return doc["i"], out


def run_python(doc):
    return run("python", doc)


def run_rust(doc):
    return run("rust", doc)


def pool():
    return Pool(int(os.environ.get("REF_JOBS", "4")), initializer=pin_wall_clock)


def make_reference(path):
    files = {v: open(REF_PATHS[v], "w", encoding="utf-8") for v in VARIANTS}
    with pool() as p:
        for i, out in p.imap(run_python, docs(path), chunksize=8):
            for v in VARIANTS:
                files[v].write(json.dumps({"i": i, "out": out[v]}) + "\n")
    for f in files.values():
        f.close()


def load_reference():
    refs = {}
    for v in VARIANTS:
        with open(REF_PATHS[v], encoding="utf-8") as f:
            refs[v] = {r["i"]: r["out"] for r in map(json.loads, f)}
    return refs


def compare(path):
    refs = load_reference()
    ok = {v: 0 for v in VARIANTS}
    bad = {v: [] for v in VARIANTS}
    with pool() as p:
        for i, out in p.imap(run_rust, docs(path), chunksize=8):
            for v in VARIANTS:
                if out[v] == refs[v][i]:
                    ok[v] += 1
                else:
                    bad[v].append((i, refs[v][i], out[v]))
    for v in VARIANTS:
        print(f"{v}: {ok[v]}/{ok[v] + len(bad[v])}")
        for i, want, got in bad[v][:20]:
            print(f"  doc {i}: python={want!r} rust={got!r}")


def main():
    mode = sys.argv[1]
    if mode == "show":
        i = int(sys.argv[2])
        path = sys.argv[3] if len(sys.argv) > 3 else f"{DATA}/docs.jsonl"
        pin_wall_clock()
        doc = next(d for d in docs(path) if d["i"] == i)
        for engine in ENGINES:
            print(engine, run(engine, doc)[1])
        return
    path = sys.argv[2] if len(sys.argv) > 2 else f"{DATA}/docs.jsonl"
    if mode == "ref":
        make_reference(path)
    elif mode == "rust":
        compare(path)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
