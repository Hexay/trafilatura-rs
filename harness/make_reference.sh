#!/usr/bin/env bash
# On testbox: build the parity corpus and Python reference outputs in ~/work/trafex-data (CPU 11, untimed).
set -euo pipefail
PY=~/work/amdahl/.venv/bin/python
H=~/work/trafex/harness
D=~/work/trafex-data
mkdir -p "$D"
run() { taskset -c 11 "$PY" -u "$@" 2>&1 | grep -v -E "INFO|WARNING" || true; }
[ -s "$D/docs.jsonl" ] || run "$H/export_docs.py" "$D/docs.jsonl" ~/work/amdahl/sample.warc.gz ~/work/amdahl/val2.warc.gz
run "$H/ref_stages.py" stages "$D/docs.jsonl" "$D/ref_stages.jsonl"
run "$H/ref_stages.py" final "$D/docs.jsonl" "$D/ref_final.jsonl"
wc -l "$D"/*.jsonl
echo REFDONE
