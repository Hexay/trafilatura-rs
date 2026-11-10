#!/usr/bin/env bash
# Local build/test entry point. Usage:
#   tools/dev.sh setup      create .venv (trafilatura 1.11, datatrove, rs-trafilatura) and .venv-t22 (trafilatura 2.2.0)
#   tools/dev.sh build      cargo build --release (harness + parity bins)
#   tools/dev.sh py         maturin develop --release into .venv
#   tools/dev.sh test       cargo test
#   tools/dev.sh bindgen    regenerate crates/libxml2-sys/src/bindings.rs from the built headers
set -euo pipefail
. "$(dirname "$0")/env.sh"
DATATROVE_SRC=${DATATROVE_SRC:-/srv/agents/work/datatrove/src}
case ${1:?command} in
  setup)
    [ -x "$PY" ] || uv venv -q -p 3.13 "$ROOT/.venv"
    uv pip install -q -p "$ROOT/.venv" -r "$ROOT/tools/requirements.txt"
    echo "$DATATROVE_SRC" > "$("$PY" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')/datatrove_src.pth"
    [ -x "$PY22" ] || uv venv -q -p 3.13 "$ROOT/.venv-t22"
    uv pip install -q -p "$ROOT/.venv-t22" trafilatura==2.2.0 lxml==6.1.3 lxml-html-clean==0.4.6 justext==3.0.2 ;;
  build) cargo build --release --manifest-path "$ROOT/Cargo.toml" -p harness ;;
  py) (cd "$ROOT" && VIRTUAL_ENV="$ROOT/.venv" "$ROOT/.venv/bin/maturin" develop --release -q) ;;
  test) cargo test --release --manifest-path "$ROOT/Cargo.toml" ;;
  bindgen)
    inc=$(ls -d "$ROOT"/target/release/build/libxml2-sys-*/out/include/libxml2 | head -1)
    bindgen "$ROOT/crates/libxml2-sys/wrapper.h" --allowlist-function '(xml|html).*' --allowlist-type '(xml|html|_xml|_html).*' \
      --allowlist-var '(XML|HTML|xml|html).*' --no-layout-tests --default-enum-style consts -- -I"$inc" \
      > "$ROOT/crates/libxml2-sys/src/bindings.rs"
    wc -l "$ROOT/crates/libxml2-sys/src/bindings.rs" ;;
  *) echo "unknown: $1" >&2; exit 2 ;;
esac
