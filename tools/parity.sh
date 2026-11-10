#!/usr/bin/env bash
# Sync, build, run the Rust harness on the corpus (CPU 5 under the shared lock) and compare with the reference.
#   [LIMIT=N] tools/parity.sh [stages|final]      default: stages
#   tools/parity.sh show <i> <stage>    diff one doc's stage output (reference vs Rust)
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
MODE=${1:-stages}
"$ROOT/tools/dev.sh" cargo build --release -q -p harness 2>&1 | grep -E "^(error|warning)" -A6 || true
D='~/work/trafex-data'
PY='~/work/amdahl/.venv/bin/python'
if [ "$MODE" = show ]; then
  ssh testbox "cd ~/work/trafex && $PY harness/ref_stages.py show $D/docs.jsonl $2 $3 > /tmp/ref.out; \
    target/release/harness show $D/docs.jsonl $2 $3 > /tmp/rust.out; diff <(fold -w 160 /tmp/ref.out) <(fold -w 160 /tmp/rust.out) | head -${LINES_MAX:-40}"
  exit 0
fi
# LIMIT=N runs only the first N docs (compare.py compares the common subset).
SRC="$D/docs.jsonl"
[ -n "${LIMIT:-}" ] && SRC="<(head -n $LIMIT $D/docs.jsonl)"
ssh testbox "cd ~/work/trafex && flock ~/bench-cpu5.lock taskset -c 5 target/release/harness $MODE $SRC $D/rust_$MODE.jsonl \
  && $PY harness/compare.py $MODE $D/ref_$MODE.jsonl $D/rust_$MODE.jsonl"
