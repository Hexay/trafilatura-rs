#!/usr/bin/env bash
# On testbox (in tmux): full-corpus final parity with every fast-path XPath cross-checked against libxml2, then
# the extraction benchmark. Output: ~/work/trafex-data/vb.log
cd ~/work/trafex || exit 1
D=~/work/trafex-data
echo "== final (TRAFEX_VERIFY_XPATH=1)"
TRAFEX_VERIFY_XPATH=1 flock ~/bench-cpu5.lock taskset -c 5 target/release/harness final $D/docs.jsonl $D/rust_final.jsonl \
  && ~/work/amdahl/.venv/bin/python harness/compare.py final $D/ref_final.jsonl $D/rust_final.jsonl
bash tools/bench.sh
echo VBDONE
