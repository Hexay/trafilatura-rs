#!/usr/bin/env bash
# Extraction benchmark on the first N corpus docs (default all): trafilatura via datatrove's call, the same
# call after trafilatura_rs.install(), and rs-trafilatura. Measures per-doc CPU time and user-space instructions (perf
# stat), never wall time; engines alternate within each of ROUNDS rounds so load drift on the shared box hits all.
# Each run is a fresh process (trafilatura's dedup LRU is process-global).
# Usage: [ROUNDS=2] [CALL=default] [V=23] [REUSE_BASELINE=1] tools/bench.sh [N]
# CALL=default benches the default call extract(html) instead (see harness/bench_extract.py).
# BENCH_CFG=<cfg> benches a harness/meta_configs.py config instead, without rs-trafilatura (BENCH_DOCS=<file>: another
# corpus file, e.g. docs_raw.jsonl for the bytes configs).
# V=23 benches trafilatura 2.3.1 (wheel in .venv-t23, no rs-trafilatura).
# Installs a release build of the extension first (PROFILE=release tools/dev.sh py; a no-op when current).
# REUSE_BASELINE=1 reruns only trafilatura_rs and reuses the other engines' runs from $D/bench-cache, keyed by the
# arguments, corpus, bench scripts and package versions: instructions stay comparable, CPU times come from another
# moment's load. Output: $OUT/bench[23][_default|_<cfg>]/ (tools/env.sh), report on stdout.
set -euo pipefail
. "$(dirname "$0")/env.sh"
N=${1:-}
py=$PY B=$OUT/bench engines=(none trafilatura trafilatura_rs rs)
[[ ${V:-} == 23 ]] && py=$PY23 B=$OUT/bench23
[[ ${V:-} == 23 || -n ${BENCH_CFG:-} ]] && engines=(none trafilatura trafilatura_rs)
B=$B${CALL:+_$CALL}${BENCH_CFG:+_$BENCH_CFG}
export BENCH_CFG PYTHONPATH=$ROOT/harness${PYTHONPATH:+:$PYTHONPATH}
PROFILE=release bash "$ROOT/tools/dev.sh" py
docs=$D/${BENCH_DOCS:-docs.jsonl}
key=$( { echo "$N ${ROUNDS:-2} ${CALL:-} ${BENCH_CFG:-} ${V:-}"; stat -c '%s %Y' "$docs"
  cat "$ROOT"/harness/{bench_extract,meta_configs,ref_meta}.py; "$py" "$ROOT/harness/ref_stamp.py"
  "$py" -c 'from importlib.metadata import version as v
for p in ("rs-trafilatura", "datatrove"):
    try: print(p, v(p))
    except Exception: pass'; } | sha1sum | cut -c1-16)
C=$D/bench-cache/$key
rm -rf "$B" && mkdir -p "$B"
for r in $(seq "${ROUNDS:-2}"); do
  for e in "${engines[@]}"; do
    f=bench_${e}_r$r
    if [[ -n ${REUSE_BASELINE:-} && $e != trafilatura_rs && -s $C/$f.jsonl ]]; then
      cp "$C/$f".* "$B/"
      continue
    fi
    perf stat -e instructions:u -x, -o "$B/$f.perf" \
      "$py" "$ROOT/harness/bench_extract.py" "$e" "$docs" "$B/$f.jsonl" $N 2> "$B/$f.err"
    if [[ $e != trafilatura_rs ]]; then
      mkdir -p "$C" && cp "$B/$f".* "$C/"
    fi
  done
done
"$py" "$ROOT/harness/bench_report.py" "$B"
