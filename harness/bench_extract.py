"""Per-doc CPU time and output of one extractor over the corpus, in corpus order, single thread.
usage: bench_extract.py {none|trafilatura|trafilatura_rs|rs} docs.jsonl out.jsonl [n]
  trafilatura  datatrove's Trafilatura(favour_precision=True).extract (trafilatura 1.11, dedup on)
  trafilatura_rs  the same call after trafilatura_rs.install()
  rs           rs_trafilatura.extract(html, favor_precision=True, include_comments=False).main_content
  none         no extraction (load/loop baseline for instruction counts)
out.jsonl: {"i", "ns", "out"} per doc; summary on stdout (bench_report.py aggregates runs)."""
import importlib.metadata  # noqa: F401  datatrove 0.10 uses it without importing it
import json
import statistics
import sys
import time


def extractor(engine):
    if engine == "none":
        return lambda html: None
    if engine == "rs":
        import rs_trafilatura

        return lambda html: rs_trafilatura.extract(html, favor_precision=True, include_comments=False).main_content
    from datatrove.pipeline.extractors import Trafilatura

    if engine == "trafilatura_rs":
        import trafilatura_rs

        trafilatura_rs.install()
    elif engine != "trafilatura":
        raise SystemExit(f"unknown engine {engine}")
    return Trafilatura(favour_precision=True).extract


def main():
    engine, path, out_path = sys.argv[1:4]
    n = int(sys.argv[4]) if len(sys.argv) > 4 else None
    with open(path, encoding="utf-8") as f:
        docs = [(r["i"], r["html"]) for r in map(json.loads, (line for _, line in zip(range(n or 10**9), f)))]
    extract = extractor(engine)
    clock = time.process_time_ns
    rows = []
    for i, html in docs:
        t = clock()
        try:
            out = extract(html)
        except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
            out = f"EXC:{type(e).__name__}"
        rows.append((i, clock() - t, out))
    with open(out_path, "w", encoding="utf-8") as f:
        for i, ns, out in rows:
            f.write(json.dumps({"i": i, "ns": ns, "out": out}) + "\n")
    ms = sorted(ns / 1e6 for _, ns, _ in rows)
    print(f"{engine}: docs={len(ms)} kept={sum(o is not None for *_, o in rows)} cpu={sum(ms) / 1e3:.1f}s "
          f"mean={statistics.fmean(ms):.2f} p50={ms[len(ms) // 2]:.2f} p99={ms[int(len(ms) * 0.99)]:.2f} ms/doc")


if __name__ == "__main__":
    main()
