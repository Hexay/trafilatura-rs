# Contributing

## Layout

```
crates/trafilatura-rs     the port: DOM/XPath layer over libxml2, trafilatura 1.11 and 2.3.1, jusText, readability
crates/trafilatura-rs-py  PyO3 bindings (trafilatura_rs._core)
crates/libxml2-sys        vendored static libxml2 2.14.6 (lxml 6.1.3's version), see PATCHES.md
crates/harness            parity and bench binaries
python/trafilatura_rs     Python API, install() patching
harness/                  Python reference generators and comparers
tests/                    pytest parity smoke on tests/fixtures (runs in CI)
docs/                     trafilatura 1.11 → 2.2.0 → 2.3.1 diffs, README assets
```

## Setup (Linux)

Needs Rust, cmake, uv and perf. The corpus and references live outside the repo in `$TRAFILATURA_RS_DATA`
(default `/srv/agents/cache/trafilatura-rs-data`).

```bash
tools/dev.sh setup     # .venv (py3.13: trafilatura 1.11, datatrove, rs-trafilatura) + .venv-t23 (trafilatura 2.3.1)
tools/dev.sh build     # cargo build of the harness bins, profile iter
tools/dev.sh py        # maturin develop into .venv, profile iter
tools/dev.sh test      # cargo test, profile iter
python -m pytest tests # parity vs trafilatura on tests/fixtures
tools/build_wheel.sh   # abi3 wheel in target/wheels (PGO=1 for profile-guided, via tools/pgo.sh)
```

`tools/dev.sh` builds with `PROFILE` (default `iter`: no LTO, 16 codegen units, incremental for the workspace crates,
so an edit rebuilds in seconds; same output). Benches and wheels need `release` (fat LTO): `tools/bench.sh` installs
it itself, `PROFILE=release tools/dev.sh build` gives `target/release/harness`. Leave `CARGO_INCREMENTAL` alone:
`dev.sh` unsets it for `iter` so dependencies stay sccache-cached.

## Parity

```bash
tools/make_reference.sh   # exports docs.jsonl (first 10,666 URLFilter-passing HTML records of a CC WARC) and all
                          # Python references; REF_JOBS (2) parallel, skips current ones (~50 CPU-min)
tools/parity.sh           # every job; or e.g. tools/parity.sh stages final:fineweb final2:recall_comments
                          # (configs: tools/env.sh; `<cfg>_comments` runs include_comments=True)
PAR=2 tools/parity.sh     # two jobs at a time, each in a free light.lock slot (don't wrap it in flock)
FAILING=1 tools/parity.sh final:balanced   # only the docs the job's last full run got wrong
LIMIT=500 tools/parity.sh final:fineweb
tools/parity.sh show 123 txt                                  # diff one doc's stage (show2 for 2.3.1)
TRAFILATURA_RS_VERIFY_XPATH=1 tools/parity.sh final:fineweb   # cross-check fast-path XPaths against libxml2
tools/parity.sh meta:balanced_json   # markdown/metadata/json and option configs (harness/meta_configs.py), via the
                                     # extension and trafilatura_rs's own argument translation
tools/parity.sh meta2:balanced_json  # same against 2.3.1 (.venv-t23; build with V=23 tools/dev.sh py)
tools/parity.sh funcs:bare_balanced  # the other drop-ins (bare_extraction, baseline, html2txt, extract_metadata,
                                     # htmldate.find_date), with CPU time per side (funcs2: 2.3.1; references:
                                     # make_reference.sh funcs:<cfg>); configs: harness/ref_funcs.py list. Speedups
                                     # need PROFILE=release tools/dev.sh py
harness/ref_meta.py list             # every meta config: the focus x output x comments grid, then option extras
harness/ref_meta.py show docs.jsonl 123 balanced_json          # both outputs of one doc
harness/export_raw.py docs.jsonl <warc> docs_raw.jsonl         # raw HTTP payloads for the bytes configs; reference
                                     # and check those with CORPUS=docs_raw.jsonl (meta:bytes_default, meta:cli_bytes)
harness/gen_meta23.sh                # regenerate 2.3.1 metadata regexes, XPaths and Unicode tables
harness/ref_htmldate.py ref|rust [docs.jsonl]                 # htmldate find_date alone, with and without url
harness/diff_dates.py collect docs.jsonl strings.jsonl balanced_json   # log the corpus' date strings, then
harness/diff_dates.py check strings.jsonl                     # native date shortcuts vs dateutil/dateparser
harness/diff_dates.py memo strings.jsonl                      # _external_date memo: call order, time of day
FUZZ=20000 PAR=2 harness/diff_dates.py dateparser strings*.jsonl  # native dateparser port vs Python (+ fuzz)
harness/diff_urls.py collect docs.jsonl pairs.jsonl           # the corpus' link/image URLs, then per Python:
target/iter/harness urls pairs.jsonl native.jsonl && python3.X harness/diff_urls.py check pairs.jsonl native.jsonl
```

After a dateparser, python-dateutil or htmldate bump, regenerate `crates/trafilatura-rs/src/dates/dateparser/tables.txt`
(`harness/gen_dateparser_tables.py`) and `locales.txt` (`harness/gen_dateparser_locales.py > locales.txt`), rerun
`diff_dates.py` (check and dateparser) and update `REPRODUCES_DATES`. After a `regex` or Python (Unicode) bump,
regenerate `chars.txt` (`harness/gen_dateparser_chars.py`), `unicode.txt` (`harness/gen_dateparser_unicode.py`),
`tables.txt` and `locales.txt`; until then non-ASCII strings skip the proof and every string skips the port.

`meta:`/`meta2:` jobs go through the built extension (`tools/dev.sh py`), so htmldate's dateutil/dateparser and courlan
callbacks run as they do for users. Both sides pin the wall clock (see docs/metadata-formats-scope.md); `fallbacks`
counts documents the core declined (`Unsupported`), which the Python API reruns in Python.

References are shared in `$TRAFILATURA_RS_DATA`; Rust outputs go to `$TRAFILATURA_RS_OUT` (default
`$TRAFILATURA_RS_DATA/out/<worktree dir>`), so worktrees running parity at once don't overwrite each other's and need
no private data dir. Each job also leaves `<output>.bad`, its mismatching doc ids, which `FAILING=1` reruns; for the
`fineweb*` configs (`deduplicate=True`) a subset run lacks the earlier docs' dedup history, so confirm a fix with a
full run. Each reference has a `<ref>.stamp` with the Python and package versions that made it
(`harness/ref_stamp.py`): `make_reference.sh` keeps a reference whose stamp matches the venv, reports `STALE` (redo
with `FORCE=1`) otherwise, and `parity.sh` warns when comparing against one made with other versions.

On the shared box, `parity.sh`, `make_reference.sh` and `bench.sh` take one of the two light.lock slots per job
(`in_light_slot`, tools/env.sh) and release it in between. Never wrap a loop of jobs, or these scripts, in `flock`:
a slot held across a loop blocks every other worktree for its whole length. Builds still go under heavy.lock.

Any output difference from trafilatura is a bug. New options join `VERIFIED_OUTPUTS` (`VERIFIED_BARE` for
bare_extraction) only with full-corpus parity.
`python/trafilatura_rs/_options.py` turns extract() arguments into the `Extractor` each version would build, then
into `_core.extract` keywords; a new option goes there, gets a `meta_configs.py` entry and a `tests/` case.

A new lxml release joins `_versions.REPRODUCES` after its Python output equals 6.1.3's on the full corpus: install it
with `uv pip install --target <dir> lxml==X`, rerun the `final:balanced_comments`, `final:fineweb`,
`meta:balanced_md_comments` and `meta:balanced_json` references (and the 2.3.1 ones) with `PYTHONPATH=<dir>`, and
compare them with the 6.1.3 files. Rust output does not depend on lxml, so no Rust run is needed.

## Benchmark

`tools/bench.sh [N]` runs trafilatura 1.11 with datatrove's arguments, the same call after `trafilatura_rs.install()`,
and `rs_trafilatura.extract(...)`. Each engine runs in a fresh process, on the same docs in the same order, single
thread, alternated over `ROUNDS` (default 2). It reports per-doc CPU time (min over rounds) and `perf stat`
user-space instructions net of a load-only baseline, never wall time. It also checks trafilatura-rs output against
trafilatura's on every doc. `CALL=default tools/bench.sh` benches the default call `extract(html)` instead, and
`BENCH_CFG=<cfg>` a `harness/ref_meta.py` config (markdown, metadata, json) against trafilatura. It installs the
release build first. `REUSE_BASELINE=1` reruns only trafilatura-rs and takes the other engines' runs from
`$TRAFILATURA_RS_DATA/bench-cache` (keyed by arguments, corpus, bench scripts and package versions); instruction counts
stay comparable, CPU times then come from different load.

The Rust side alone: `target/release/harness bench[2] docs.jsonl [n] [cfg]` (`PROFILE=release tools/dev.sh build`;
cfg as in parity, default fineweb;
`balanced_comments` is the default `extract(html)`), under `perf stat -e instructions:u`. To profile, build with
frame pointers and full debug info, then `perf record -g` that binary:

```bash
CFLAGS=-fno-omit-frame-pointer CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold \
  -C force-frame-pointers=yes" cargo build --profile profiling -p harness   # target/profiling/harness
```

## Releasing

Bump `version` in `pyproject.toml`, merge to `main`, then `git tag vX.Y.Z && git push origin vX.Y.Z`.
`.github/workflows/release.yml` builds abi3 wheels (Linux manylinux and musllinux x86_64/aarch64, macOS
x86_64/arm64, Windows x64/arm64) and an sdist, smoke-tests each against trafilatura, checks the tag matches the version, and publishes to PyPI by trusted
publishing (environment `pypi`).
