"""Aggregate tools/bench.sh runs: per-engine CPU ms/doc (per-doc min over rounds), user-space instructions net of
the `none` baseline, speedups vs trafilatura, and output agreement with trafilatura.
usage: bench_report.py <dir>    reads <dir>/bench_<engine>_r<k>.{jsonl,perf}"""
import glob
import json
import os
import re
import statistics
import sys
from collections import defaultdict

ENGINES = ["trafilatura", "trafex", "rs"]


def load(path):
    with open(path, encoding="utf-8") as f:
        return [json.loads(line) for line in f]


def instructions(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.strip().split(",")
            if len(parts) > 2 and parts[2].startswith("instructions") and parts[0].isdigit():
                return int(parts[0])
    return None


def ws(s):
    return " ".join((s or "").split())


def main():
    d = sys.argv[1]
    runs, instr = defaultdict(list), defaultdict(list)
    for path in sorted(glob.glob(f"{d}/bench_*_r*.jsonl")):
        engine = re.match(r"bench_(\w+?)_r\d+\.jsonl", os.path.basename(path)).group(1)
        runs[engine].append(load(path))
        n = instructions(path[:-len(".jsonl")] + ".perf")
        if n is not None:
            instr[engine].append(n)
    base = min(instr["none"]) if instr.get("none") else 0
    ndocs = len(runs["trafilatura"][0])
    stats = {}
    print(f"docs={ndocs} rounds={ {e: len(runs[e]) for e in ENGINES if e in runs} }")
    print(f"{'engine':<12} {'mean ms':>8} {'p50 ms':>7} {'p99 ms':>7} {'CPU s':>7} {'Minstr/doc':>11}")
    for e in ENGINES:
        if e not in runs:
            continue
        ms = sorted(min(r[k]["ns"] for r in runs[e]) / 1e6 for k in range(ndocs))
        mi = (min(instr[e]) - base) / ndocs / 1e6 if instr.get(e) else float("nan")
        stats[e] = (statistics.fmean(ms), mi)
        print(f"{e:<12} {stats[e][0]:8.2f} {ms[len(ms) // 2]:7.2f} {ms[int(len(ms) * 0.99)]:7.2f} "
              f"{sum(ms) / 1e3:7.1f} {mi:11.1f}")
    t_ms, t_mi = stats["trafilatura"]
    for e in ("trafex", "rs"):
        if e in stats:
            print(f"speedup {e} vs trafilatura: CPU {t_ms / stats[e][0]:.2f}x, instructions {t_mi / stats[e][1]:.2f}x")
    ref = runs["trafilatura"][0]
    for e in ("trafex", "rs"):
        if e not in runs:
            continue
        out = runs[e][0]
        assert [r["i"] for r in out] == [r["i"] for r in ref], f"{e}: doc order differs"
        exact = [k for k in range(ndocs) if out[k]["out"] == ref[k]["out"]]
        loose = sum((out[k]["out"] or "") == (ref[k]["out"] or "") for k in range(ndocs))
        wsn = sum(ws(out[k]["out"]) == ws(ref[k]["out"]) for k in range(ndocs))
        bad = [ref[k]["i"] for k in range(ndocs) if out[k]["out"] != ref[k]["out"]]
        print(f"{e} vs trafilatura: exact={len(exact)}/{ndocs} (None==''): {loose}/{ndocs} "
              f"whitespace-normalized={wsn}/{ndocs} first mismatches={bad[:10]}")


if __name__ == "__main__":
    main()
