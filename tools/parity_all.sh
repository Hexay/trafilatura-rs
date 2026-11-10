#!/usr/bin/env bash
# On testbox (in tmux): full-corpus parity of final extract() output, then every stage. Writes ~/work/trafex-data/parity.log.
cd ~/work/trafex || exit 1
D=~/work/trafex-data
PY=~/work/amdahl/.venv/bin/python
for mode in final stages; do
  echo "== $mode"
  flock ~/bench-cpu5.lock taskset -c 5 target/release/harness $mode $D/docs.jsonl $D/rust_$mode.jsonl \
    && $PY harness/compare.py $mode $D/ref_$mode.jsonl $D/rust_$mode.jsonl
done
echo PARITYDONE
