# trafex

Byte-exact Rust port of trafilatura 1.11's text extraction (`extract()`, txt output, `include_comments=False`),
pinned to trafilatura 1.11.0 + lxml 6.1.3 / libxml2 2.14.6 (vendored and statically linked, see
`crates/libxml2-sys/PATCHES.md`). A port of trafilatura 2.2.0's pipeline (`crates/trafex/src/trafilatura2`,
diff in `research/trafilatura-2x-diff.md`) is verified by the harness but not exposed in Python yet.

## Verified configs

`trafex.VERIFIED`, each byte-identical to Python on the 10,666-doc corpus below
(`(favor_precision, favor_recall, include_tables, no_fallback)`):

| harness cfg | args | 1.11 final | 2.2.0 final |
|---|---|---|---|
| fineweb | `favor_precision=True, deduplicate=True` (datatrove) | 10666/10666 | 10666/10666 |
| balanced | defaults | 10666/10666 | 10666/10666 |
| recall | `favor_recall=True` | 10666/10666 | 10666/10666 |
| fast | `no_fallback=True` | 10666/10666 | 10666/10666 |
| notables | `include_tables=False` | 10666/10666 | 10666/10666 |

Stage hashes: 1.11 (fineweb) 10666/10666; 2.2.0 balanced and fineweb 10666/10666. jusText and readability
components: 10666/10666. Any other argument falls back to Python trafilatura.

## Usage

```python
import trafex
trafex.install()          # patches trafilatura.extract; covered calls run in Rust, others stay on Python
# e.g. datatrove's Trafilatura(favour_precision=True) now runs trafex
trafex.extract(html, favor_precision=True, deduplicate=True)   # direct; NotImplementedError if not covered
```

`deduplicate=True` keeps a process-global LRU like trafilatura's, so output depends on doc order;
`trafex.reset_caches()` clears it.

## Dev setup (Linux)

Needs Rust, cmake, uv, perf. Corpus and references live outside the repo in `$TRAFEX_DATA`
(default `/srv/agents/cache/trafex-data`).

```bash
tools/dev.sh setup     # .venv (py3.13: trafilatura 1.11, datatrove via .pth from $DATATROVE_SRC, rs-trafilatura)
                       # + .venv-t22 (trafilatura 2.2.0)
tools/dev.sh build     # cargo build --release (harness bins)
tools/dev.sh py        # maturin develop --release into .venv
tools/dev.sh test      # cargo test
tools/build_wheel.sh   # abi3 wheel in target/wheels (PGO=1 for profile-guided, via tools/pgo.sh)
```

## Parity

```bash
tools/make_reference.sh   # exports docs.jsonl (first 10,666 URLFilter-passing HTML records of the WARC) and all
                          # Python references; REF_JOBS parallel, skips existing files (~50 CPU-min)
tools/parity.sh           # every job; or e.g. tools/parity.sh stages final:fineweb final2:recall
LIMIT=500 tools/parity.sh final:fineweb
tools/parity.sh show 123 txt          # diff one doc's stage (show2 for 2.2.0)
TRAFEX_VERIFY_XPATH=1 tools/parity.sh final:fineweb   # cross-check fast-path XPaths against libxml2
```

## Benchmark

`tools/bench.sh [N]` — trafilatura 1.11 via datatrove's call, the same call after `trafex.install()`, and
`rs_trafilatura.extract(html, favor_precision=True, include_comments=False)`. Fresh process per run, same docs in
the same order, single thread, engines alternated over `ROUNDS` (default 2). Reports per-doc CPU time (min over
rounds) and `perf stat` user-space instructions net of a load-only baseline; never wall time (shared box). It also
checks trafex output == trafilatura output on every doc (the end-to-end check) and rs-trafilatura agreement.

Full corpus (10,666 docs), 2026-11-10, loaded 16-core box:

| engine | mean ms/doc | p50 | p99 | Minstr/doc | speedup (CPU / instr) | output == trafilatura |
|---|---|---|---|---|---|---|
| trafilatura 1.11 | 17.93 | 11.19 | 110.83 | 261.7 | 1x | — |
| trafex | 1.92 | 1.25 | 11.85 | 23.2 | 9.3x / 11.3x | 10666/10666 |
| rs-trafilatura 0.1.1 | 12.24 | 7.66 | 81.88 | 181.8 | 1.5x / 1.4x | 171/10666 exact, 974 whitespace-normalized |

rs-trafilatura is a different extractor, not a port: it never returns None, separates paragraphs with `\n\n`, and
selects different content (248/10666 equal with None == "", 172 of them both empty; 637 equal after collapsing
newline runs; median difflib similarity ~0.67 on the first 400 docs).
