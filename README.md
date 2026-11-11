# trafilatura-rs

[![PyPI](https://img.shields.io/pypi/v/trafilatura-rs.svg)](https://pypi.org/project/trafilatura-rs/)
[![Python versions](https://img.shields.io/pypi/pyversions/trafilatura-rs.svg)](https://pypi.org/project/trafilatura-rs/)
[![CI](https://github.com/Hexay/trafilatura-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Hexay/trafilatura-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A Rust port of [trafilatura](https://github.com/adbar/trafilatura)'s main-text extraction that returns **the same
output, byte for byte**, as `trafilatura.extract()`, using about a tenth of the CPU.

<p align="center">
  <img src="https://raw.githubusercontent.com/Hexay/trafilatura-rs/main/docs/assets/extract-speed.svg"
       alt="CPU per page: trafilatura 17.93 ms, rs-trafilatura 12.24 ms, trafilatura-rs 1.92 ms" width="720">
</p>

## Installation

```bash
pip install "trafilatura-rs[trafilatura]"
```

The `[trafilatura]` extra installs the exact versions trafilatura-rs is verified against (trafilatura 1.11.0,
lxml 6.1.3, htmldate 1.11.0), so you can fall back to and compare with the original. Wheels are built for Linux,
macOS and Windows, Python 3.10+, with libxml2 statically linked.

## Quickstart

Your existing code, unchanged:

```bash
TRAFILATURA_RS=1 python my_script.py
```

Every `trafilatura.extract` call trafilatura-rs supports now runs in Rust; every other call runs Python trafilatura
as before. That includes calls made inside other libraries; datatrove's `Trafilatura` step, for example, is covered
(or use [datatrove-rs](https://github.com/Hexay/datatrove-rs)).

Prefer it in code? Any of these works too:

```python
import trafilatura_rs
trafilatura_rs.install()          # patch trafilatura.extract, including copies imported earlier

from trafilatura_rs import extract  # or swap the import: same signature, falls back to Python when needed
```

You can also run a script unmodified with `python -m trafilatura_rs my_script.py`.

`trafilatura_rs.supports(**kwargs)` tells you which path a call takes. If your installed trafilatura or lxml
isn't the version trafilatura-rs reproduces, every call stays on Python and `install()` warns, so results never
change. Markdown, JSON and metadata output are covered too (see below).

## What's supported

`extract()` with string input, in the configurations below (`trafilatura_rs.VERIFIED_OUTPUTS`). Other arguments,
bytes input and other output formats run Python trafilatura.

Plain text, each with comments (`include_comments=True`, trafilatura's default) and without:

| Configuration | Arguments | Identical, comments | Identical, `include_comments=False` |
|---|---|---|---|
| Default | | 10,666 / 10,666 | 10,666 / 10,666 |
| Precision | `favor_precision=True` | 10,666 / 10,666 | 10,666 / 10,666 |
| datatrove / FineWeb | `favor_precision=True, deduplicate=True` | 10,666 / 10,666 | 10,666 / 10,666 |
| Recall | `favor_recall=True` | 10,666 / 10,666 | 10,666 / 10,666 |
| Fast | `no_fallback=True` | 10,666 / 10,666 | 10,666 / 10,666 |
| No tables | `include_tables=False` | 10,666 / 10,666 | 10,666 / 10,666 |

Markdown, JSON and metadata, with `include_comments=False`:

| Configuration | Arguments | Identical to trafilatura |
|---|---|---|
| Markdown | `output_format="markdown"` | 10,666 / 10,666 |
| Markdown, datatrove / FineWeb | `output_format="markdown", favor_precision=True, deduplicate=True` | 10,666 / 10,666 |
| Text with metadata | `with_metadata=True` | 10,666 / 10,666 |
| JSON | `output_format="json"` | 10,666 / 10,666 |
| JSON with metadata | `output_format="json", with_metadata=True` | 10,666 / 10,666 |
| JSON with metadata, precision | `output_format="json", with_metadata=True, favor_precision=True` | 10,666 / 10,666 |

`url` and `record_id` can be passed with any of them. `deduplicate=True` is verified with the precision
configurations. Like trafilatura, it keeps a process-global cache, so output depends on document order exactly as in
Python; `trafilatura_rs.reset_caches()` clears it.

### Metadata

```python
trafilatura.extract(html, include_comments=False, with_metadata=True, url=url)        # Rust: text with a --- header
trafilatura.extract(html, include_comments=False, output_format="json", with_metadata=True)  # Rust
trafilatura.extract(html, include_comments=False, with_metadata=True, deduplicate=True)      # Python
```

Title, author, date, sitename, categories, tags, license and the JSON-LD parsing run in Rust, including a port of
htmldate 1.11's `find_date`. A few leaves run in Python: courlan's URL helpers, and the dateutil and dateparser
calls htmldate makes for unusual date strings. Those calls cost little and keep results exact for any installed
version. Metadata calls therefore need trafilatura installed and Python 3.11+. They also need htmldate 1.11.0
(`REPRODUCES_METADATA`); with another version they stay on Python.

As in trafilatura, the date search is bounded by today's date (`max_date`). `date_extraction_params` is supported
when it only sets `max_date`. `with_metadata=True` with `deduplicate=True` stays on Python, because a document that
needs Python would update the wrong deduplication cache. Rare documents the port leaves to Python (listed in
[docs/metadata-formats-scope.md](docs/metadata-formats-scope.md)) are rerun there automatically. None occur in the
10,666-page corpus.

## Exactness

Each configuration is checked page by page against trafilatura 1.11.0 on 10,666 Common Crawl pages, comparing final
output and per-stage tree hashes. The jusText and readability fallbacks are checked separately. Exactness holds
because the parser is the same: trafilatura-rs vendors and statically links libxml2 2.14.6, the version lxml 6.1.3
bundles, with the same parsing options (see [PATCHES.md](crates/libxml2-sys/PATCHES.md)).

A port of trafilatura 2.2.0's extraction is verified by the same harness but not yet exposed in Python
(differences: [docs/trafilatura-2x-diff.md](docs/trafilatura-2x-diff.md)).

### How is this different from rs-trafilatura?

[rs-trafilatura](https://pypi.org/project/rs-trafilatura/) is an independent extractor inspired by trafilatura,
not a port. On the same 10,666 pages its output equals trafilatura's on 171. It never returns `None`, separates
paragraphs differently and often selects different content. Use trafilatura-rs when you need trafilatura's results,
for example to reproduce a dataset.

## Benchmark

Single thread, CPU time per page, 10,666 Common Crawl pages, `favor_precision=True` (`tools/bench.sh`):

| | mean | p50 | p99 | Speedup |
|---|---:|---:|---:|---:|
| trafilatura 1.11 | 17.93 ms | 11.19 ms | 110.83 ms | 1x |
| rs-trafilatura 0.1.1 | 12.24 ms | 7.66 ms | 81.88 ms | 1.5x |
| **trafilatura-rs** | **1.92 ms** | **1.25 ms** | **11.85 ms** | **9.3x** |

Measured by instruction count, the speedup is 11.3x.

The default call, `extract(html)` (comments included, `CALL=default tools/bench.sh`):

| | mean | p50 | p99 | Speedup |
|---|---:|---:|---:|---:|
| trafilatura 1.11 | 12.56 ms | 7.91 ms | 78.49 ms | 1x |
| rs-trafilatura 0.1.1 | 9.63 ms | 6.20 ms | 61.98 ms | 1.3x |
| **trafilatura-rs** | **2.10 ms** | **1.53 ms** | **10.91 ms** | **6.0x** |

Measured by instruction count, the speedup is 7.1x.

Markdown, metadata and JSON (first 3,000 pages, `include_comments=False`, `BENCH_CFG=<cfg> tools/bench.sh 3000`):

| Call | trafilatura 1.11 | trafilatura-rs | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|
| `output_format="markdown"` | 8.36 ms | 1.58 ms | 5.3x | 6.5x |
| `with_metadata=True, url=...` | 18.07 ms | 9.53 ms | 1.9x | 2.3x |
| `output_format="json", with_metadata=True` | 21.02 ms | 9.09 ms | 2.3x | 2.6x |

Mean CPU time per page. With metadata, about half of the remaining time is dateparser, which htmldate calls for
unusual date strings and which trafilatura-rs calls back into rather than ports (see
[docs/metadata-formats-scope.md](docs/metadata-formats-scope.md)).

## Contributing

Development setup, the parity harness, benchmarks and releasing: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0, like trafilatura. Vendored libxml2 is MIT-licensed; win-iconv (Windows builds) is public domain.
trafilatura-rs is an independent project and is not affiliated with the trafilatura authors.
