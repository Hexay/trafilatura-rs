#!/usr/bin/env bash
# Extraction benchmark on the first N corpus docs (default all): trafilatura 1.11 via datatrove's call, the same
# call after trafilatura_rs.install(), and rs-trafilatura. Measures per-doc CPU time and user-space instructions (perf
# stat), never wall time; engines alternate within each of ROUNDS rounds so load drift on the shared box hits all.
# Each run is a fresh process (trafilatura's dedup LRU is process-global). Usage: [ROUNDS=2] [CALL=default] tools/bench.sh [N]
# CALL=default benches the default call extract(html) instead (see harness/bench_extract.py).
# BENCH_CFG=<cfg> benches a harness/ref_meta.py config (markdown/metadata/json) instead, without rs-trafilatura.
# Needs the trafilatura-rs wheel in .venv (tools/dev.sh py). Output: $D/bench[_default|_<cfg>]/, report on stdout.
set -euo pipefail
. "$(dirname "$0")/env.sh"
N=${1:-}
B=$D/bench${CALL:+_$CALL}${BENCH_CFG:+_$BENCH_CFG}
ENGINES=(none trafilatura trafilatura_rs rs)
[ -n "${BENCH_CFG:-}" ] && ENGINES=(none trafilatura trafilatura_rs)
export BENCH_CFG PYTHONPATH=$ROOT/harness${PYTHONPATH:+:$PYTHONPATH}
rm -rf "$B" && mkdir -p "$B"
for r in $(seq "${ROUNDS:-2}"); do
  for e in "${ENGINES[@]}"; do
    perf stat -e instructions:u -x, -o "$B/bench_${e}_r$r.perf" \
      "$PY" "$ROOT/harness/bench_extract.py" "$e" "$D/docs.jsonl" "$B/bench_${e}_r$r.jsonl" $N 2> "$B/bench_${e}_r$r.err"
  done
done
"$PY" "$ROOT/harness/bench_report.py" "$B"
