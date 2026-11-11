#!/usr/bin/env bash
# Rust outputs vs the tools/make_reference.sh references (build first: tools/dev.sh build). Usage:
#   [LIMIT=N] tools/parity.sh [job ...]   job = stages | final:<cfg> | stages2:<cfg> | final2:<cfg> | meta:<cfg>
#                                         | justext | readability   (meta: through the extension, tools/dev.sh py)
#                                         default: every job; LIMIT compares only the first N docs
#   tools/parity.sh show[2] <i> <stage> [cfg]   diff one doc's stage output (reference vs Rust)
# TRAFILATURA_RS_VERIFY_XPATH=1 cross-checks every fast-path XPath against libxml2 while running.
set -euo pipefail
. "$(dirname "$0")/env.sh"
BIN=$(dirname "$H")
DOCS=$D/docs.jsonl
if [[ ${1:-} == show* ]]; then
  ref=$ROOT/harness/ref_stages.py py=$PY
  [ "$1" = show2 ] && ref=$ROOT/harness/ref2_stages.py py=$PY22
  diff <("$py" "$ref" show "$DOCS" "${@:2}" 2>/dev/null | fold -w 160) <("$H" "$1" "$DOCS" "${@:2}" | fold -w 160) \
    | head -n "${LINES_MAX:-40}" || true
  exit 0
fi
[ -n "${LIMIT:-}" ] && { head -n "$LIMIT" "$D/docs.jsonl" > "$D/docs_$LIMIT.jsonl"; DOCS=$D/docs_$LIMIT.jsonl; }

run() {
  local mode=${1%%:*} cfg=${1#*:}
  [[ $1 == *:* ]] || cfg=
  echo "== $1"
  case $mode in
    stages | final | stages2 | final2)
      local tag=${mode%2}_$cfg ref=ref_
      [[ $mode == *2 ]] && ref=ref2_
      [ "$mode" = stages ] && tag=stages
      "$H" "$mode" "$DOCS" "$D/rust${ref#ref}$tag.jsonl" $cfg
      "$PY" "$ROOT/harness/compare.py" "$mode" "$D/$ref$tag.jsonl" "$D/rust${ref#ref}$tag.jsonl" ;;
    meta)
      "$PY" "$ROOT/harness/ref_meta.py" rust "$DOCS" "$D/rust_meta_$cfg.jsonl" "$cfg"
      echo "fallbacks: $(grep -c '"EXC:Unsupported"' "$D/rust_meta_$cfg.jsonl" || true)"
      "$PY" "$ROOT/harness/compare.py" final "$D/ref_meta_$cfg.jsonl" "$D/rust_meta_$cfg.jsonl" ;;
    justext | readability)
      "$BIN/${mode}_parity" run "$DOCS" "$D/rust_$mode.jsonl"
      "$PY" "$ROOT/harness/compare_$mode.py" "$D/ref_$mode.jsonl" "$D/rust_$mode.jsonl" ;;
    *) echo "unknown job $1" >&2; return 2 ;;
  esac
}

jobs=("$@")
if [ ${#jobs[@]} -eq 0 ]; then
  jobs=(stages justext readability stages2:balanced stages2:fineweb)
  for c in "${CONFIGS[@]}"; do jobs+=("final:$c" "final2:$c"); done
  for c in "${COMMENT_CONFIGS[@]}"; do jobs+=("final:$c" "final2:$c"); done
fi
for j in "${jobs[@]}"; do run "$j"; done
echo PARITYDONE
