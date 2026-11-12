#!/usr/bin/env bash
# Rust outputs vs the tools/make_reference.sh references (build first: tools/dev.sh build). Usage:
#   [LIMIT=N] [PAR=2] [FAILING=1] tools/parity.sh [job ...]
#       job = stages | final:<cfg> | stages2:<cfg> | final2:<cfg> | meta:<cfg> | meta2:<cfg> | justext | readability
#       (meta/meta2: through the extension, tools/dev.sh py / V=23 tools/dev.sh py); default: every job but meta.
#       LIMIT compares only the first N docs. PAR=2 runs jobs two at a time, each holding a free light.lock slot
#       (don't wrap it in flock). FAILING=1 reruns only the docs the job's last full run got wrong.
#   tools/parity.sh show[2] <i> <stage> [cfg]   diff one doc's stage output (reference vs Rust)
# TRAFILATURA_RS_VERIFY_XPATH=1 cross-checks every fast-path XPath against libxml2 while running.
# Rust outputs go to $OUT (tools/env.sh, per worktree); each job's mismatching doc ids to $OUT/<out>.bad.
set -euo pipefail
. "$(dirname "$0")/env.sh"
BIN=$(dirname "$H")
DOCS=$D/${CORPUS:-docs.jsonl}  # CORPUS=docs_raw.jsonl for the bytes meta configs
if [[ ${1:-} == show* ]]; then
  ref=$ROOT/harness/ref_stages.py py=$PY
  [ "$1" = show2 ] && ref=$ROOT/harness/ref2_stages.py py=$PY23
  diff <("$py" "$ref" show "$DOCS" "${@:2}" 2>/dev/null | fold -w 160) <("$H" "$1" "$DOCS" "${@:2}" | fold -w 160) \
    | head -n "${LINES_MAX:-40}" || true
  exit 0
fi
if [ -n "${LIMIT:-}" ]; then
  sub=$D/${CORPUS:-docs.jsonl}
  sub=${sub%.jsonl}_$LIMIT.jsonl
  [ -s "$sub" ] || { head -n "$LIMIT" "$DOCS" > "$sub.$$" && mv "$sub.$$" "$sub"; }
  DOCS=$sub
fi

# run <job>: Rust side on $DOCS (or the job's last failing docs), then compare with the reference.
run() {
  local mode=${1%%:*} cfg=${1#*:} ref rust cmp py=$PY docs=$DOCS t0=$SECONDS
  [[ $1 == *:* ]] || cfg=
  case $mode in
    stages | final | stages2 | final2)
      local tag=${mode%2}_$cfg r=ref_
      [[ $mode == *2 ]] && r=ref23_
      [ "$mode" = stages ] && tag=stages
      ref=$D/$r$tag.jsonl rust=rust${r#ref}$tag.jsonl cmp=$mode ;;
    meta | meta2)
      local tag=
      [ "$mode" = meta2 ] && py=$PY23 tag=23
      ref=$D/ref${tag}_meta_$cfg.jsonl rust=rust${tag}_meta_$cfg.jsonl cmp=final ;;
    justext | readability) ref=$D/ref_$mode.jsonl rust=rust_$mode.jsonl ;;
    *) echo "unknown job $1" >&2; return 2 ;;
  esac
  local bad=$OUT/${rust%.jsonl}${LIMIT:+_$LIMIT}.bad
  rust=$OUT/${rust%.jsonl}${LIMIT:+_$LIMIT}${FAILING:+_failing}.jsonl
  echo "== $1"
  if [ -n "${FAILING:-}" ]; then
    [ -f "$bad" ] || { echo "no $bad: run the job once without FAILING"; return 2; }
    [ -s "$bad" ] || { echo "last run had no mismatches"; return 0; }
    docs=${rust%.jsonl}.docs.jsonl
    "$PY" "$ROOT/harness/subset_docs.py" "$bad" "$DOCS" "$docs"
    [[ $cfg == fineweb* ]] && echo "note: $cfg deduplicates across docs; confirm with a full run"
  fi
  case $mode in
    stages | final | stages2 | final2) "$H" "$mode" "$docs" "$rust" $cfg ;;
    meta | meta2)
      "$py" "$ROOT/harness/ref_meta.py" rust "$docs" "$rust" "$cfg"
      echo "fallbacks: $(grep -c '"EXC:Unsupported"' "$rust" || true)" ;;
    *) "$BIN/${mode}_parity" run "$docs" "$rust" ;;
  esac
  [ ! -f "$ref.stamp" ] || [ "$(cat "$ref.stamp")" = "$("$py" "$ROOT/harness/ref_stamp.py")" ] \
    || echo "WARNING: $(basename "$ref") was made with other package versions (tools/make_reference.sh $1)"
  local keep=$bad
  [ -n "${FAILING:-}" ] && keep=$rust.bad  # a subset run must not shrink the full run's failing list
  if [ -n "${cmp:-}" ]; then
    "$py" "$ROOT/harness/compare.py" "$cmp" "$ref" "$rust" "$keep"
  else
    "$PY" "$ROOT/harness/compare_$mode.py" "$ref" "$rust"
  fi
  echo "($1: $((SECONDS - t0))s)"
}

jobs=("$@")
[ ${#jobs[@]} -gt 0 ] || mapfile -t jobs < <(all_jobs)
if [ "${PAR:-1}" -gt 1 ]; then
  export -f run in_light_slot
  export ROOT D PY PY23 H BIN OUT DOCS LIMIT FAILING CORPUS
  # each job's report prints whole once it finishes
  printf '%s\n' "${jobs[@]}" | xargs -P "$PAR" -n 1 bash -c 'set -euo pipefail; log=$(mktemp)
    trap "cat \$log; rm -f \$log" EXIT; in_light_slot run "$1" > "$log" 2>&1' _
else
  for j in "${jobs[@]}"; do run "$j"; done
fi
echo PARITYDONE
