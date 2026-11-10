#!/usr/bin/env bash
# Extraction benchmark on the first N corpus docs (default all): trafilatura 1.11 via datatrove's call, the same
# call after trafex.install(), and rs-trafilatura. Measures per-doc CPU time and user-space instructions (perf
# stat), never wall time; engines alternate within each of ROUNDS rounds so load drift on the shared box hits all.
# Each run is a fresh process (trafilatura's dedup LRU is process-global). Usage: [ROUNDS=2] tools/bench.sh [N]
# Needs the trafex wheel in .venv (tools/dev.sh py). Output: $D/bench/, report on stdout.
set -euo pipefail
. "$(dirname "$0")/env.sh"
N=${1:-}
B=$D/bench
rm -rf "$B" && mkdir -p "$B"
for r in $(seq "${ROUNDS:-2}"); do
  for e in none trafilatura trafex rs; do
    perf stat -e instructions:u -x, -o "$B/bench_${e}_r$r.perf" \
      "$PY" "$ROOT/harness/bench_extract.py" "$e" "$D/docs.jsonl" "$B/bench_${e}_r$r.jsonl" $N 2> "$B/bench_${e}_r$r.err"
  done
done
"$PY" "$ROOT/harness/bench_report.py" "$B"
