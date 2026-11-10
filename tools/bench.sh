#!/usr/bin/env bash
# On testbox: extraction time on the first N corpus docs (default 2885 = the CC-MAIN-2024 sample), CPU 5 under
# the shared lock. Rust trafex vs stock trafilatura 1.11 vs trafilatura with the Python patches (research/).
N=${1:-2885}
D=~/work/trafex-data
A=~/work/amdahl
head -n "$N" $D/docs.jsonl > $D/bench_docs.jsonl
run() { flock ~/bench-cpu5.lock taskset -c 5 "$@"; }
echo "== rust"; run ~/work/trafex/target/release/harness bench $D/bench_docs.jsonl
for mode in orig fused+lookup+readability; do
  echo "== python $mode"
  run $A/.venv/bin/python - "$mode" "$D/bench_docs.jsonl" <<'EOF' 2>&1 | grep -v -E "WARNING|INFO"
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
done
