"""Per-doc CPU time and output of one extractor over the corpus, in corpus order, single thread.
usage: bench_extract.py {none|trafilatura|trafilatura_rs|rs} docs.jsonl out.jsonl [n]
  trafilatura  datatrove's Trafilatura(favour_precision=True).extract (dedup on), or the extract() call it makes when
               datatrove isn't installed (the trafilatura 2.3.1 venv)
  trafilatura_rs  the same call after trafilatura_rs.install()
  rs           rs_trafilatura.extract(html, favor_precision=True, include_comments=False).main_content
  none         no extraction (load/loop baseline for instruction counts)
CALL=default switches every engine to its default call: trafilatura.extract(html) (include_comments=True, no dedup)
and rs_trafilatura.extract(html).main_content.
With BENCH_CFG=<cfg> (a harness/meta_configs.py config), trafilatura and trafilatura_rs call extract() with that
config's arguments instead of datatrove's (rs is not comparable there); bytes configs need BENCH_DOCS=docs_raw.jsonl.
BENCH_PY_DATEPARSER=1 keeps htmldate's dateparser calls in Python there (the native port off, for before/after).
out.jsonl: {"i", "ns", "out"} per doc; summary on stdout (bench_report.py aggregates runs)."""
import importlib.metadata  # noqa: F401  datatrove 0.10 uses it without importing it
import json
import os
import statistics
import sys
import time

DEFAULT_CALL = os.environ.get("CALL", "datatrove") == "default"


def config_extractor(engine, cfg):
    from meta_configs import kwargs_for
    from ref_meta import pin_wall_clock

    pin_wall_clock()
    if os.environ.get("BENCH_PY_DATEPARSER"):
        import htmldate.extractors as ex

        # not the stock function: trafilatura_rs runs dateparser in Python (still memoized), as before the port
        ex._external_date = (lambda f: lambda s: f(s))(ex._external_date)
    if engine == "trafilatura_rs":
        from trafilatura_rs import extract
    else:
        from trafilatura import extract

    def run(doc):
        html, kwargs = kwargs_for(cfg, doc)
        return extract(html, **kwargs)

    return run


def extractor(engine):
    cfg = os.environ.get("BENCH_CFG")
    if engine == "none":
        return lambda doc: None
    if cfg:
        return config_extractor(engine, cfg)
    if engine == "rs":
        import rs_trafilatura

        if DEFAULT_CALL:
            return lambda doc: rs_trafilatura.extract(doc["html"]).main_content
        return lambda doc: rs_trafilatura.extract(doc["html"], favor_precision=True,
                                                  include_comments=False).main_content
    if engine == "trafilatura_rs":
        import trafilatura_rs

        assert trafilatura_rs.install() is not None, "installed trafilatura/lxml not reproduced"
    elif engine != "trafilatura":
        raise SystemExit(f"unknown engine {engine}")
    if DEFAULT_CALL:
        import trafilatura

        return lambda doc: trafilatura.extract(doc["html"])
    try:
        from datatrove.pipeline.extractors import Trafilatura
    except ImportError:
        import trafilatura

        return lambda doc: trafilatura.extract(doc["html"], favor_precision=True, include_comments=False,
                                               deduplicate=True)
    extract = Trafilatura(favour_precision=True).extract
    return lambda doc: extract(doc["html"])


def main():
    engine, path, out_path = sys.argv[1:4]
    n = int(sys.argv[4]) if len(sys.argv) > 4 else None
    from ref_meta import docs as read_docs

    # streamed, not loaded up front: the corpus is ~1.4 GB, and only extract() is inside the timer
    docs = (d for _, d in zip(range(n or 10**9), read_docs(path)))
    extract = extractor(engine)
    clock = time.process_time_ns
    rows = []
    for doc in docs:
        t = clock()
        try:
            out = extract(doc)
        except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
            out = f"EXC:{type(e).__name__}"
        rows.append((doc["i"], clock() - t, out))
    with open(out_path, "w", encoding="utf-8") as f:
        for i, ns, out in rows:
            f.write(json.dumps({"i": i, "ns": ns, "out": out}) + "\n")
    ms = sorted(ns / 1e6 for _, ns, _ in rows)
    print(f"{engine}: docs={len(ms)} kept={sum(o is not None for *_, o in rows)} cpu={sum(ms) / 1e3:.1f}s "
          f"mean={statistics.fmean(ms):.2f} p50={ms[len(ms) // 2]:.2f} p99={ms[int(len(ms) * 0.99)]:.2f} ms/doc")


if __name__ == "__main__":
    main()
