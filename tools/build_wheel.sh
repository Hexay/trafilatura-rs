#!/usr/bin/env bash
# On testbox: profile-guided abi3 wheel (tools/pgo.sh trains on the corpus and leaves /tmp/pgo.profdata; the
# profile covers the trafex crate the extension module links). Output: ~/work/trafex/target/wheels/.
set -euo pipefail
cd ~/work/trafex
bash tools/pgo.sh
. ~/.cargo/env
RUSTFLAGS="-Cprofile-use=/tmp/pgo.profdata" ~/work/amdahl/.venv/bin/maturin build --release -q
ls -la target/wheels/
