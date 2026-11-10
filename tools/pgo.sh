#!/usr/bin/env bash
# Profile-guided build of the harness (Rust code only; libxml2 stays -O3). Trains on corpus docs 5001-5600,
# disjoint from the first 2885; leaves $D/pgo/pgo.profdata (used by tools/build_wheel.sh) and $D/pgo/harness.
# RUSTFLAGS replaces the config's target rustflags, so these builds use the default linker.
set -euo pipefail
. "$(dirname "$0")/env.sh"
P=$D/pgo
T=$ROOT/target/pgo
PROF=$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-profdata)
rm -rf "$P" && mkdir -p "$P/raw"
sed -n 5001,5600p "$D/docs.jsonl" > "$P/train.jsonl"
RUSTFLAGS="-Cprofile-generate=$P/raw" cargo build -q --release -p harness --manifest-path "$ROOT/Cargo.toml" --target-dir "$T/gen"
"$T/gen/release/harness" bench "$P/train.jsonl"
"$T/gen/release/harness" bench2 "$P/train.jsonl"
"$PROF" merge -o "$P/pgo.profdata" "$P/raw"
RUSTFLAGS="-Cprofile-use=$P/pgo.profdata" cargo build -q --release -p harness --manifest-path "$ROOT/Cargo.toml" --target-dir "$T/use"
cp "$T/use/release/harness" "$P/harness"
echo PGODONE
