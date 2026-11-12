#!/usr/bin/env bash
# Extraction benchmark on the first N corpus docs (default all): trafilatura via datatrove's call, the same
# call after trafilatura_rs.install(), and rs-trafilatura. Measures per-doc CPU time and user-space instructions (perf
# stat), never wall time; engines alternate within each of ROUNDS rounds so load drift on the shared box hits all.
# Each run is a fresh process (trafilatura's dedup LRU is process-global).
# Usage: [ROUNDS=2] [CALL=default] [V=23] tools/bench.sh [N]
# CALL=default benches the default call extract(html) instead (see harness/bench_extract.py).
# BENCH_CFG=<cfg> benches a harness/meta_configs.py config instead, without rs-trafilatura (BENCH_DOCS=<file>: another
# corpus file, e.g. docs_raw.jsonl for the bytes configs).
# V=23 benches trafilatura 2.3.1 (wheel in .venv-t23, no rs-trafilatura).
# Needs the trafilatura-rs wheel in .venv (tools/dev.sh py). Output: $D/bench[23][_default|_<cfg>]/, report on stdout.
set -euo pipefail
. "$(dirname "$0")/env.sh"
N=${1:-}
py=$PY B=$D/bench engines=(none trafilatura trafilatura_rs rs)
[[ ${V:-} == 23 ]] && py=$PY23 B=$D/bench23
[[ ${V:-} == 23 || -n ${BENCH_CFG:-} ]] && engines=(none trafilatura trafilatura_rs)
B=$B${CALL:+_$CALL}${BENCH_CFG:+_$BENCH_CFG}
export BENCH_CFG PYTHONPATH=$ROOT/harness${PYTHONPATH:+:$PYTHONPATH}
rm -rf "$B" && mkdir -p "$B"
for r in $(seq "${ROUNDS:-2}"); do
  for e in "${engines[@]}"; do
    perf stat -e instructions:u -x, -o "$B/bench_${e}_r$r.perf" \
      "$py" "$ROOT/harness/bench_extract.py" "$e" "$D/${BENCH_DOCS:-docs.jsonl}" "$B/bench_${e}_r$r.jsonl" $N 2> "$B/bench_${e}_r$r.err"
  done
done
"$py" "$ROOT/harness/bench_report.py" "$B"
