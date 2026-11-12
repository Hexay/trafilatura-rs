# Sourced by tools/*.sh: paths shared by the local dev, reference, parity and bench scripts.
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
D=${TRAFILATURA_RS_DATA:-/srv/agents/cache/trafilatura-rs-data}
PY=$ROOT/.venv/bin/python
PY23=$ROOT/.venv-t23/bin/python
# cargo profile of tools/dev.sh build|py and the parity harness: iter (fast rebuilds), release for benches and wheels
PROFILE=${PROFILE:-iter}
H=${HARNESS:-$ROOT/target/$PROFILE/harness}
# Rust-side outputs, per worktree: references stay shared in $D, outputs of parallel worktrees don't clobber
OUT=${TRAFILATURA_RS_OUT:-$D/out/$(basename "$ROOT")}
CONFIGS=(fineweb balanced recall fast notables)
# include_comments=True variants (harness/ref_stages.py CONFIGS), final and final2 only
COMMENT_CONFIGS=(fineweb_comments precision_comments balanced_comments recall_comments fast_comments notables_comments)
export PYTHONUNBUFFERED=1 PYTHONIOENCODING=utf-8 CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export TLDEXTRACT_CACHE=${TLDEXTRACT_CACHE:-$D/tldextract} HF_HOME=${HF_HOME:-$D/hf}
mkdir -p "$D" "$OUT"
quiet() { grep -v -E "INFO|WARNING" || true; }

# Every parity/reference job (default list of tools/parity.sh and tools/make_reference.sh).
all_jobs() {
  local c
  printf '%s\n' stages justext readability stages2:balanced stages2:fineweb
  for c in "${CONFIGS[@]}" "${COMMENT_CONFIGS[@]}"; do printf '%s\n' "final:$c" "final2:$c"; done
}

# in_light_slot cmd...: run one job holding whichever light.lock.{0,1} frees first; nested calls run directly.
# Take it per job, never around a loop of jobs: a held slot blocks every other worktree.
in_light_slot() {
  local s fd rc
  [ -n "${LIGHT_SLOT_HELD:-}" ] && { "$@"; return; }
  while :; do
    for s in 0 1; do
      exec {fd}>"$D/light.lock.$s"
      if flock -w 2 "$fd"; then
        LIGHT_SLOT_HELD=1 "$@"
        rc=$?
        exec {fd}>&-
        return $rc
      fi
      exec {fd}>&-
    done
  done
}
