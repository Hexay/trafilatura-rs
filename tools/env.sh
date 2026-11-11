# Sourced by tools/*.sh: paths shared by the local dev, reference, parity and bench scripts.
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
D=${TRAFILATURA_RS_DATA:-/srv/agents/cache/trafilatura-rs-data}
PY=$ROOT/.venv/bin/python
PY22=$ROOT/.venv-t22/bin/python
H=${HARNESS:-$ROOT/target/release/harness}
CONFIGS=(fineweb balanced recall fast notables)
# include_comments=True variants (harness/ref_stages.py CONFIGS), final and final2 only
COMMENT_CONFIGS=(fineweb_comments precision_comments balanced_comments recall_comments fast_comments notables_comments)
export PYTHONUNBUFFERED=1 PYTHONIOENCODING=utf-8 CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export TLDEXTRACT_CACHE=${TLDEXTRACT_CACHE:-$D/tldextract} HF_HOME=${HF_HOME:-$D/hf}
mkdir -p "$D"
quiet() { grep -v -E "INFO|WARNING" || true; }
