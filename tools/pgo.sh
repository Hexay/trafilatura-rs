#!/usr/bin/env bash
# On testbox: profile-guided build of the harness (Rust code only; libxml2 stays -O3). Trains on corpus docs
# 5001-5600 (CC-MAIN-2023 half, disjoint from the 2885-doc benchmark), writes the binary to /tmp/h_pgo.
set -euo pipefail
. ~/.cargo/env
cd ~/work/trafex
T=~/work/trafex-pgo-target
PROF=$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-profdata)
rm -rf /tmp/pgo-data && mkdir -p /tmp/pgo-data
sed -n 5001,5600p ~/work/trafex-data/docs.jsonl > /tmp/train.jsonl
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" cargo build -q --release -p harness --target-dir $T/gen
taskset -c 11 $T/gen/release/harness bench /tmp/train.jsonl
taskset -c 11 $T/gen/release/harness bench2 /tmp/train.jsonl
"$PROF" merge -o /tmp/pgo.profdata /tmp/pgo-data
RUSTFLAGS="-Cprofile-use=/tmp/pgo.profdata" cargo build -q --release -p harness --target-dir $T/use
cp $T/use/release/harness /tmp/h_pgo
echo PGODONE
