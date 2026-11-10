#!/usr/bin/env bash
# On testbox (in tmux): trafilatura 2.2.0 parity of the Rust port (harness *2 modes) on CPU 5 under the shared
# lock, against tools/ref2.sh outputs. Output: ~/work/trafex-data/parity2.log
#   tools/parity2.sh [final:<cfg>|stages:<cfg> ...]    default: final for every config
cd ~/work/trafex || exit 1
D=~/work/trafex-data
PY=~/work/amdahl/.venv/bin/python
for job in ${@:-final:fineweb final:balanced final:recall final:fast final:notables}; do
  mode=${job%%:*} cfg=${job#*:}
  echo "== $mode $cfg"
  flock ~/bench-cpu5.lock taskset -c 5 target/release/harness ${mode}2 $D/docs.jsonl $D/rust2_${mode}_$cfg.jsonl $cfg \
    && $PY harness/compare.py ${mode}2 $D/ref2_${mode}_$cfg.jsonl $D/rust2_${mode}_$cfg.jsonl
done
echo PARITY2DONE
