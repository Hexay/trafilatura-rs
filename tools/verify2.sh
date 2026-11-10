#!/usr/bin/env bash
# On testbox (in tmux), after `tools/dev.sh sync`: rebuild, then 2.2.0 stage parity (balanced, fineweb), 2.2.0
# final parity for every config, and the 1.11 fineweb final check. Output: ~/work/trafex-data/verify2.log
cd ~/work/trafex || exit 1
. ~/.cargo/env
taskset -c 11 cargo build --release -q -p harness 2>&1 | grep -E "^(error|warning)" -A6
bash tools/parity2.sh stages:balanced stages:fineweb final:fineweb final:balanced final:recall final:fast final:notables
bash tools/config_matrix.sh fineweb
echo VERIFY2DONE
