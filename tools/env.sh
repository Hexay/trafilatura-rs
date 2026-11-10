# Sourced by tools/*.sh: paths shared by the local dev, reference, parity and bench scripts.
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
D=${TRAFEX_DATA:-/srv/agents/cache/trafex-data}
PY=$ROOT/.venv/bin/python
PY22=$ROOT/.venv-t22/bin/python
H=${HARNESS:-$ROOT/target/release/harness}
CONFIGS=(fineweb balanced recall fast notables)
export PYTHONUNBUFFERED=1 PYTHONIOENCODING=utf-8 CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export TLDEXTRACT_CACHE=${TLDEXTRACT_CACHE:-$D/tldextract} HF_HOME=${HF_HOME:-$D/hf}
mkdir -p "$D"
quiet() { grep -v -E "INFO|WARNING" || true; }
