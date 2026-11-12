# trafilatura-rs

See CONTRIBUTING.md for layout, parity and benches.

- Edit loop: `tools/dev.sh build` / `tools/dev.sh py` (profile `iter`, seconds) under heavy.lock, then
  `tools/parity.sh <job>`; `release` only for benches and wheels (`tools/bench.sh` installs it).
- `parity.sh`, `make_reference.sh` and `bench.sh` take a light.lock slot per job. Never wrap a loop of jobs, or these
  scripts, in `flock`: a slot held across a loop blocks every other worktree.
- `FAILING=1 tools/parity.sh <job>` reruns only the docs the last full run got wrong.
