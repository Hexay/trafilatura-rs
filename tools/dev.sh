#!/usr/bin/env bash
# Build/test host is testbox (~/work/trafex); sources live here. Usage:
#   tools/dev.sh sync                 copy the project to the box (skips target/)
#   tools/dev.sh cargo <args...>      sync, then run cargo on the box (e.g. build --release, test -p trafex)
#   tools/dev.sh sh <cmd...>          run a shell command in ~/work/trafex on the box
#   tools/dev.sh bindgen              regenerate crates/libxml2-sys/src/bindings.rs from the built headers
# Timed benchmarks must go through tools/bench.sh (shared-box CPU pinning), not here.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
remote() { ssh testbox "cd ~/work/trafex && . ~/.cargo/env && $*"; }
case ${1:?command} in
  sync) bash "$ROOT/../tools/testbox-sync.sh" trafex | tail -1 ;;
  cargo) shift; "$0" sync >/dev/null && remote cargo "$@" ;;
  sh) shift; remote "$@" ;;
  bindgen)
    inc=$(remote 'ls -d target/*/build/libxml2-sys-*/out/include/libxml2 | head -1')
    remote "bindgen crates/libxml2-sys/wrapper.h --allowlist-function '(xml|html).*' --allowlist-type '(xml|html|_xml|_html).*' \
      --allowlist-var '(XML|HTML|xml|html).*' --no-layout-tests --default-enum-style consts -- -I$inc" > "$ROOT/crates/libxml2-sys/src/bindings.rs"
    wc -l "$ROOT/crates/libxml2-sys/src/bindings.rs" ;;
  *) echo "unknown: $1" >&2; exit 2 ;;
esac
