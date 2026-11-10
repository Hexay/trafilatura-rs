#!/usr/bin/env bash
# Release abi3 wheel into target/wheels/. PGO=1 first runs tools/pgo.sh and builds with its profile.
set -euo pipefail
. "$(dirname "$0")/env.sh"
if [ "${PGO:-0}" = 1 ]; then
  bash "$ROOT/tools/pgo.sh"
  export RUSTFLAGS="-Cprofile-use=$D/pgo/pgo.profdata"
fi
(cd "$ROOT" && "$ROOT/.venv/bin/maturin" build --release -q)
ls -la "$ROOT/target/wheels/"
