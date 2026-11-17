# Benchmarks

How `tools/bench.sh` measures (fresh process per engine, alternating rounds, per-doc CPU time and `perf stat`
instructions, output checked against trafilatura on every doc): [CONTRIBUTING.md](../CONTRIBUTING.md#benchmark).

## Full corpus, trafilatura-rs 0.1.9

10,666 Common Crawl pages, single thread, release build (`PROFILE=release tools/bench.sh`), Python 3.13, lxml 6.1.3,
measured 2026-11-13. Each value is the median over 3 runs of `tools/bench.sh` (2 rounds each, per-doc minimum over the
rounds). The box is shared: the 1-minute load average was 5 to 31 on 16 cores during the runs, and the benchmark's
cgroup showed no memory pressure after any run. CPU times move with that load; instruction counts (user space, net of a load-only baseline)
don't.

datatrove's call, `extract(html, favor_precision=True, include_comments=False, deduplicate=True)`:

| Engine | mean | p50 | p99 | Minstr/page | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|---:|---:|
| trafilatura 1.11.0 | 15.77 ms | 10.14 ms | 99.07 ms | 261.7 | | |
| trafilatura-rs, 1.11.0 | 1.44 ms | 0.97 ms | 8.76 ms | 19.5 | 11.0x | 13.4x |
| rs-trafilatura 0.1.1 | 11.07 ms | 6.93 ms | 74.08 ms | 182.0 | 1.4x | 1.4x |
| trafilatura 2.3.1 | 13.72 ms | 9.07 ms | 80.71 ms | 199.1 | | |
| trafilatura-rs, 2.3.1 | 1.65 ms | 1.10 ms | 9.77 ms | 24.6 | 8.3x | 8.1x |

The default call, `extract(html)` (`CALL=default`; comments included, no deduplication):

| Engine | mean | p50 | p99 | Minstr/page | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|---:|---:|
| trafilatura 1.11.0 | 12.68 ms | 8.05 ms | 79.43 ms | 192.4 | | |
| trafilatura-rs, 1.11.0 | 1.44 ms | 0.95 ms | 8.92 ms | 19.1 | 8.8x | 10.1x |
| rs-trafilatura 0.1.1 | 10.88 ms | 6.89 ms | 68.70 ms | 162.4 | 1.2x | 1.2x |
| trafilatura 2.3.1 | 16.38 ms | 9.17 ms | 113.45 ms | 242.0 | | |
| trafilatura-rs, 2.3.1 | 1.93 ms | 1.16 ms | 13.36 ms | 26.7 | 8.5x | 9.1x |

trafilatura-rs output equals trafilatura's on 10,666 of 10,666 pages in every run. rs-trafilatura's equals
trafilatura 1.11.0's on 171 pages with datatrove's call and 98 with the default call. rs-trafilatura is compared with
1.11.0 only; it doesn't depend on the trafilatura version.

## lxml tree input, trafilatura 2.3.1

`V=23 INPUT=tree PROFILE=release tools/bench.sh`: each engine gets `lxml.html.fromstring` trees built outside the
timer (10,562 pages lxml parses), one run of 2 rounds, measured 2026-11-17. trafilatura-rs copies the tree into Rust
instead of parsing HTML.

| Call | trafilatura mean | trafilatura-rs mean | Minstr/page | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|---:|
| datatrove's | 20.02 ms | 2.24 ms | 184.8 / 17.4 | 8.9x | 10.6x |
| `extract(tree)` | 25.83 ms | 2.67 ms | 227.1 / 19.6 | 9.7x | 11.6x |

Output equals trafilatura's on 10,562 of 10,562 pages in both. 1.11.0 trees run in Python (see
[coverage.md](coverage.md#extract)).

## Metadata calls, PR #16

The first 3,000 corpus pages, `BENCH_CFG=<config> PROFILE=release tools/bench.sh 3000`, one run with `ROUNDS=1`,
measured 2026-11-16 on the shared box under load (CPU times are noisy; instructions are the stable figure). Configs
are `harness/meta_configs.py`'s, all `include_comments=False` with `url=`: `balanced_meta` is `with_metadata=True`,
`balanced_json` and `precision_json` add `output_format="json"`. Before is main where this branch started.

| Config | Python ms / Minstr | Before: ms / Minstr | Before speedup (CPU / instr) | After: ms / Minstr | After speedup (CPU / instr) |
|---|---:|---:|---:|---:|---:|
| balanced_json, 1.11.0 | 27.35 / 355.7 | 10.41 / 105.3 | 2.6x / 3.4x | 5.08 / 40.1 | 5.4x / 8.9x |
| balanced_json, 2.3.1 | 25.79 / 354.7 | 7.46 / 99.5 | 3.5x / 3.6x | 5.85 / 46.6 | 4.4x / 7.6x |
| balanced_meta, 1.11.0 | 22.89 / 313.1 | 11.05 / 104.1 | 2.1x / 3.0x | 4.83 / 38.8 | 4.7x / 8.1x |
| balanced_meta, 2.3.1 | 28.27 / 353.9 | 7.88 / 99.1 | 3.6x / 3.6x | 5.82 / 46.2 | 4.9x / 7.7x |
| precision_json, 1.11.0 | 30.07 / 413.5 | 10.79 / 105.3 | 2.8x / 3.9x | 5.04 / 40.0 | 6.0x / 10.3x |
| precision_json, 2.3.1 | 36.50 / 327.4 | 10.53 / 95.8 | 3.5x / 3.4x | 5.30 / 42.7 | 6.9x / 7.7x |

Output equals trafilatura's on 3,000 of 3,000 pages in every run. Rust instructions per page dropped 2.1 to 2.7x: the
metadata XPaths now take the DOM fast path, the lookaround-only `fancy_regex` patterns are prefiltered by a `regex`
scan, and htmldate's date-cue check runs natively.

## The README chart

`docs/assets/extract-speed.svg` is generated from `docs/assets/extract-speed.json`, which holds the medians above for
datatrove's call. After new runs, save each `tools/bench.sh` report and regenerate:

```bash
tools/bench.sh > run1.txt                     # x3; V=23 for 2.3.1
python3 tools/make_chart.py collect docs/assets/extract-speed.json 1.11.0 run*.txt
python3 tools/make_chart.py svg docs/assets/extract-speed.json > docs/assets/extract-speed.svg
```
