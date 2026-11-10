#!/usr/bin/env bash
# On testbox (in tmux): trafilatura 2.2.0 reference outputs (harness/ref2_stages.py) on CPU 11, skipping existing files.
#   tools/ref2.sh [final:<cfg>|stages:<cfg> ...]    default: final for every config, then balanced/fineweb stages
cd ~/work/trafex/harness || exit 1
D=~/work/trafex-data
PY=~/work/t22venv/bin/python
for job in ${@:-final:fineweb final:balanced final:recall final:fast final:notables stages:balanced stages:fineweb}; do
  mode=${job%%:*} cfg=${job#*:}
  out=$D/ref2_${mode}_$cfg.jsonl
  [ -s "$out" ] && continue
  echo "== $mode $cfg $(date +%T)"
  taskset -c 11 $PY ref2_stages.py "$mode" $D/docs.jsonl "$out.tmp" "$cfg" 2>/dev/null && mv "$out.tmp" "$out"
done
echo REF2DONE
