#!/usr/bin/env bash
# Python reference outputs in $D (untimed), REF_JOBS at a time. Usage:
#   [SLOTS=1] [FORCE=1] tools/make_reference.sh [job ...]
#       job = stages | final:<cfg> | stages2:<cfg> | final2:<cfg> | meta:<cfg> | meta2:<cfg> | justext | readability
#       (meta configs: harness/ref_meta.py CONFIGS; meta2: trafilatura 2.3.1). Default: every job but meta.
# Each reference gets a <ref>.stamp of the Python package versions that produced it (harness/ref_stamp.py). An
# existing reference is kept when its stamp matches the venv (or it has none: it is adopted); on a mismatch the job
# stops unless FORCE=1. SLOTS=1 runs each job holding a free light.lock slot (two at a time; don't wrap it in flock).
# docs.jsonl is exported first if missing (WARC=path overrides the source WARC).
set -euo pipefail
. "$(dirname "$0")/env.sh"
WARC=${WARC:-/srv/agents/cache/fineweb-profile/warc/CC-MAIN-20240220211055-20240221001055-00000.warc.gz}
if [ ! -s "$D/docs.jsonl" ]; then
  "$PY" "$ROOT/harness/export_docs.py" --limit "${DOCS:-10666}" "$D/docs.jsonl.tmp" "$WARC" 2>&1 | quiet
  mv "$D/docs.jsonl.tmp" "$D/docs.jsonl"
fi

job() {
  local mode=${1%%:*} cfg=${1#*:} out py=$PY cmd
  case $mode in
    stages) out=ref_stages.jsonl cmd=("$ROOT/harness/ref_stages.py" stages) cfg= ;;
    final) out=ref_final_$cfg.jsonl cmd=("$ROOT/harness/ref_stages.py" final) ;;
    stages2 | final2) out=ref23_${mode%2}_$cfg.jsonl py=$PY23 cmd=("$ROOT/harness/ref2_stages.py" "${mode%2}") ;;
    meta) out=ref_meta_$cfg.jsonl cmd=("$ROOT/harness/ref_meta.py" ref) ;;
    meta2) out=ref23_meta_$cfg.jsonl py=$PY23 cmd=("$ROOT/harness/ref_meta.py" ref) ;;
    justext) out=ref_justext.jsonl cmd=("$ROOT/harness/ref_justext.py" run) cfg= ;;
    readability) out=ref_readability.jsonl cmd=("$ROOT/harness/ref_readability.py" run) cfg= ;;
    *) echo "unknown job $1" >&2; return 2 ;;
  esac
  local stamp
  stamp=$("$py" "$ROOT/harness/ref_stamp.py")
  if [ -s "$D/$out" ]; then
    [ -f "$D/$out.stamp" ] || echo "$stamp" > "$D/$out.stamp"
    [ "$(cat "$D/$out.stamp")" = "$stamp" ] && return 0
    [ -n "${FORCE:-}" ] || { echo "STALE $out: made with $(cat "$D/$out.stamp"), venv has $stamp (FORCE=1 redoes it)"; return 0; }
  fi
  "$py" "${cmd[@]}" "$D/${CORPUS:-docs.jsonl}" "$D/$out.tmp" ${cfg:+"$cfg"} 2> "$D/$out.err" \
    && echo "$stamp" > "$D/$out.stamp" && mv "$D/$out.tmp" "$D/$out" && echo "done $out" || echo "FAILED $out (see $D/$out.err)"
  [ -s "$D/$out.err" ] || rm -f "$D/$out.err"
}
export -f job quiet in_light_slot
export ROOT D PY PY23 CORPUS FORCE

jobs=("$@")
[ ${#jobs[@]} -gt 0 ] || mapfile -t jobs < <(all_jobs)
if [ -n "${SLOTS:-}" ]; then
  printf '%s\n' "${jobs[@]}" | xargs -P "${REF_JOBS:-2}" -n 1 bash -c 'in_light_slot job "$1"' _
else
  printf '%s\n' "${jobs[@]}" | xargs -P "${REF_JOBS:-4}" -n 1 bash -c 'job "$1"' _
fi
echo REFDONE
