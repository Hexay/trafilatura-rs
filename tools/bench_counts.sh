#!/usr/bin/env bash
# On testbox: user-space cycles/instructions (perf stat, CPU 11) of extract() over the first N corpus docs,
# alternating Rust and Python so drift on the shared box hits both. Counts exclude nothing but are dominated by
# extraction (the loaders read N docs of JSON first). Usage: bench_counts.sh [N] [rust binary]
N=${1:-2885}
H=${2:-/tmp/h_final}
D=~/work/trafex-data
head -n "$N" $D/docs.jsonl > /tmp/bench_docs.jsonl
stat() { taskset -c 11 perf stat -e cycles:u,instructions:u -x, "$@" 2>&1 | grep -E "extract=|cycles|instructions" | cut -d, -f1,3; }
py() {
  stat ~/work/amdahl/.venv/bin/python - "$1" /tmp/bench_docs.jsonl <<'EOF'
import json, sys, time
sys.path.insert(0, "/home/ubuntu/work/amdahl")
import trafpatch
trafpatch.apply(sys.argv[1])
from trafilatura import extract
pages = [json.loads(l)["html"] for l in open(sys.argv[2], encoding="utf-8")]
t = time.perf_counter(); kept = 0
for h in pages:
    try:
        kept += extract(h, favor_precision=True, include_comments=False, deduplicate=True) is not None
    except Exception:
        pass
s = time.perf_counter() - t
print(f"docs={len(pages)} kept={kept} extract={s:.2f}s = {1000*s/len(pages):.2f} ms/doc")
EOF
}
for round in 1 2; do
  echo "== rust (round $round)"; stat "$H" bench /tmp/bench_docs.jsonl
  echo "== python stock (round $round)"; py orig
done
echo "== python patched"; py fused+lookup+readability
echo BENCHDONE
