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
tools/dev.sh build     # cargo build --release (harness bins)
tools/dev.sh py        # maturin develop --release into .venv
tools/dev.sh test      # cargo test
python -m pytest tests # parity vs trafilatura on tests/fixtures
tools/build_wheel.sh   # abi3 wheel in target/wheels (PGO=1 for profile-guided, via tools/pgo.sh)
```

## Parity

```bash
tools/make_reference.sh   # exports docs.jsonl (first 10,666 URLFilter-passing HTML records of a CC WARC) and all
                          # Python references; REF_JOBS parallel, skips existing files (~50 CPU-min)
tools/parity.sh           # every job; or e.g. tools/parity.sh stages final:fineweb final2:recall_comments
                          # (configs: tools/env.sh; `<cfg>_comments` runs include_comments=True)
LIMIT=500 tools/parity.sh final:fineweb
tools/parity.sh show 123 txt                                  # diff one doc's stage (show2 for 2.3.1)
TRAFILATURA_RS_VERIFY_XPATH=1 tools/parity.sh final:fineweb   # cross-check fast-path XPaths against libxml2
tools/parity.sh meta:balanced_json   # markdown/metadata/json configs (harness/ref_meta.py), via the extension
tools/parity.sh meta2:balanced_json  # same against 2.3.1 (.venv-t23; build with V=23 tools/dev.sh py)
harness/ref_meta.py show docs.jsonl 123 balanced_json          # both outputs of one doc
harness/gen_meta23.sh                # regenerate 2.3.1 metadata regexes, XPaths and Unicode tables
harness/ref_htmldate.py ref|rust [docs.jsonl]                 # htmldate find_date alone, with and without url
harness/diff_dates.py collect docs.jsonl strings.jsonl balanced_json   # log the corpus' date strings, then
harness/diff_dates.py check strings.jsonl                     # native date shortcuts vs dateutil/dateparser
harness/diff_dates.py memo strings.jsonl                      # _external_date memo: call order, time of day
```

After a dateparser, python-dateutil or htmldate bump, regenerate `crates/trafilatura-rs/src/dates/dateparser/tables.txt`
(`harness/gen_dateparser_tables.py`), rerun `diff_dates.py` and update `REPRODUCES_DATES`. After a `regex` or Python
(Unicode) bump, regenerate `chars.txt` (`harness/gen_dateparser_chars.py`) and `tables.txt`; until then non-ASCII
strings skip the shortcut.

`meta:`/`meta2:` jobs go through the built extension (`tools/dev.sh py`), so htmldate's dateutil/dateparser and courlan
callbacks run as they do for users. Both sides pin the wall clock (see docs/metadata-formats-scope.md); `fallbacks`
counts documents the core declined (`Unsupported`), which the Python API reruns in Python.

Any output difference from trafilatura is a bug. New options join `VERIFIED_OUTPUTS` only with full-corpus parity.

## Benchmark

`tools/bench.sh [N]` runs trafilatura 1.11 with datatrove's arguments, the same call after `trafilatura_rs.install()`,
and `rs_trafilatura.extract(...)`. Each engine runs in a fresh process, on the same docs in the same order, single
thread, alternated over `ROUNDS` (default 2). It reports per-doc CPU time (min over rounds) and `perf stat`
user-space instructions net of a load-only baseline, never wall time. It also checks trafilatura-rs output against
trafilatura's on every doc. `CALL=default tools/bench.sh` benches the default call `extract(html)` instead, and
`BENCH_CFG=<cfg>` a `harness/ref_meta.py` config (markdown, metadata, json) against trafilatura.

The Rust side alone: `target/release/harness bench[2] docs.jsonl [n] [cfg]` (cfg as in parity, default fineweb;
`balanced_comments` is the default `extract(html)`), under `perf stat -e instructions:u`. To profile, build with
frame pointers and full debug info, then `perf record -g` that binary:

```bash
CFLAGS=-fno-omit-frame-pointer CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold \
  -C force-frame-pointers=yes" cargo build --profile profiling -p harness   # target/profiling/harness
```

## Releasing

Bump `version` in `pyproject.toml`, merge to `main`, then `git tag vX.Y.Z && git push origin vX.Y.Z`.
`.github/workflows/release.yml` builds abi3 wheels (Linux x86_64/aarch64, macOS x86_64/arm64, Windows x64) and an
sdist, smoke-tests each against trafilatura, checks the tag matches the version, and publishes to PyPI by trusted
publishing (environment `pypi`).
