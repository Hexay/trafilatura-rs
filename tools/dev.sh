#!/usr/bin/env bash
# Local build/test entry point. Usage:
#   tools/dev.sh setup      create .venv (trafilatura 1.11, datatrove, rs-trafilatura) and .venv-t23 (trafilatura 2.3.1)
#   tools/dev.sh build      cargo build of the harness + parity bins (PROFILE, tools/env.sh: iter; release for benches)
#   tools/dev.sh py         maturin develop into .venv (V=23: .venv-t23), same PROFILE
#   tools/dev.sh test       cargo test, same PROFILE
#   tools/dev.sh bindgen    regenerate crates/libxml2-sys/src/bindings.rs from the built headers
set -euo pipefail
. "$(dirname "$0")/env.sh"
DATATROVE_SRC=${DATATROVE_SRC:-/srv/agents/work/datatrove/src}
# iter's `incremental = true` covers workspace crates only, deps stay sccache-cached; sccache rejects the env var
[ "$PROFILE" = iter ] && unset CARGO_INCREMENTAL
case ${1:?command} in
  setup)
    [ -x "$PY" ] || uv venv -q -p 3.13 "$ROOT/.venv"
    uv pip install -q -p "$ROOT/.venv" -r "$ROOT/tools/requirements.txt"
    echo "$DATATROVE_SRC" > "$("$PY" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')/datatrove_src.pth"
    [ -x "$PY23" ] || uv venv -q -p 3.13 "$ROOT/.venv-t23"
    uv pip install -q -p "$ROOT/.venv-t23" trafilatura==2.3.1 lxml==6.1.3 lxml-html-clean==0.4.6 justext==3.0.2 \
      courlan==1.4.0 htmldate==1.11.0 dateparser==1.5.0 python-dateutil==4.0.0 charset-normalizer==3.5.2 maturin==1.16.0 pytest ;;
  build) cargo build --profile "$PROFILE" --manifest-path "$ROOT/Cargo.toml" -p harness ;;
  py) venv=$ROOT/.venv${V:+-t$V}  # V=23: into .venv-t23
    (cd "$ROOT" && VIRTUAL_ENV="$venv" "$venv/bin/maturin" develop --profile "$PROFILE" -q) ;;
  test) cargo test --profile "$PROFILE" --manifest-path "$ROOT/Cargo.toml" ;;
  bindgen)
    inc=$(ls -d "$ROOT"/target/release/build/libxml2-sys-*/out/include/libxml2 | head -1)
    bindgen "$ROOT/crates/libxml2-sys/wrapper.h" --allowlist-function '(xml|html).*' --allowlist-type '(xml|html|_xml|_html).*' \
      --allowlist-var '(XML|HTML|xml|html).*' --no-layout-tests --default-enum-style consts -- -I"$inc" \
      > "$ROOT/crates/libxml2-sys/src/bindings.rs"
    wc -l "$ROOT/crates/libxml2-sys/src/bindings.rs" ;;
  *) echo "unknown: $1" >&2; exit 2 ;;
esac
