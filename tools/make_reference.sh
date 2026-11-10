#!/usr/bin/env bash
# Python reference outputs in $D (untimed), REF_JOBS at a time, skipping files that already exist. Usage:
#   tools/make_reference.sh [job ...]    job = stages | final:<cfg> | stages2:<cfg> | final2:<cfg> | justext | readability
# Default: every job. docs.jsonl is exported first if missing (WARC=path overrides the source WARC).
set -euo pipefail
. "$(dirname "$0")/env.sh"
WARC=${WARC:-/srv/agents/cache/fineweb-profile/warc/CC-MAIN-20240220211055-20240221001055-00000.warc.gz}
if [ ! -s "$D/docs.jsonl" ]; then
  "$PY" "$ROOT/harness/export_docs.py" --limit "${DOCS:-10666}" "$D/docs.jsonl.tmp" "$WARC" 2>&1 | quiet
  mv "$D/docs.jsonl.tmp" "$D/docs.jsonl"
fi

job() {
  local mode=${1%%:*} cfg=${1#*:} out cmd
  case $mode in
    stages) out=ref_stages.jsonl cmd=("$PY" "$ROOT/harness/ref_stages.py" stages) cfg= ;;
    final) out=ref_final_$cfg.jsonl cmd=("$PY" "$ROOT/harness/ref_stages.py" final) ;;
    stages2 | final2) out=ref2_${mode%2}_$cfg.jsonl cmd=("$PY22" "$ROOT/harness/ref2_stages.py" "${mode%2}") ;;
    justext) out=ref_justext.jsonl cmd=("$PY" "$ROOT/harness/ref_justext.py" run) cfg= ;;
    readability) out=ref_readability.jsonl cmd=("$PY" "$ROOT/harness/ref_readability.py" run) cfg= ;;
    *) echo "unknown job $1" >&2; return 2 ;;
  esac
  [ -s "$D/$out" ] && return 0
  "${cmd[@]}" "$D/docs.jsonl" "$D/$out.tmp" ${cfg:+"$cfg"} 2> "$D/$out.err" && mv "$D/$out.tmp" "$D/$out" \
    && echo "done $out" || echo "FAILED $out (see $D/$out.err)"
  [ -s "$D/$out.err" ] || rm -f "$D/$out.err"
}
export -f job quiet
export ROOT D PY PY22

jobs=("$@")
if [ ${#jobs[@]} -eq 0 ]; then
  jobs=(stages justext readability stages2:balanced stages2:fineweb)
  for c in "${CONFIGS[@]}"; do jobs+=("final:$c" "final2:$c"); done
fi
printf '%s\n' "${jobs[@]}" | xargs -P "${REF_JOBS:-4}" -I{} bash -c 'job {}'
echo REFDONE
