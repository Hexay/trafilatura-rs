#!/usr/bin/env bash
# On testbox (in tmux): final-output parity for every extract() configuration in CONFIGS (harness/ref_stages.py).
# Python reference on CPU 11, Rust on CPU 5. Output: ~/work/trafex-data/matrix.log
# HARNESS=/path/to/copy pins the binary (target/ may be rebuilt by concurrent work mid-run).
cd ~/work/trafex || exit 1
D=~/work/trafex-data
H=${HARNESS:-target/release/harness}
PY=~/work/amdahl/.venv/bin/python
for cfg in ${@:-balanced recall fast notables}; do
  [ -s $D/ref_final_$cfg.jsonl ] || taskset -c 11 $PY harness/ref_stages.py final $D/docs.jsonl $D/ref_final_$cfg.jsonl $cfg 2>/dev/null &
  flock ~/bench-cpu5.lock taskset -c 5 $H final $D/docs.jsonl $D/rust_final_$cfg.jsonl $cfg
  wait
  echo "== $cfg"
  $PY harness/compare.py final $D/ref_final_$cfg.jsonl $D/rust_final_$cfg.jsonl
done
echo MATRIXDONE
