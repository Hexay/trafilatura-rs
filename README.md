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
lxml 6.1.3), so you can fall back to and compare with the original. Wheels are built for Linux, macOS and Windows,
Python 3.10+, with libxml2 statically linked.

## Quickstart

Keep calling trafilatura as you do now; supported calls run in Rust:

```python
import trafilatura
import trafilatura_rs

trafilatura_rs.install()

text = trafilatura.extract(html, favor_precision=True)  # runs in Rust, same result
```

Calls trafilatura-rs doesn't cover (metadata, XML/JSON output, comments, ...) keep running Python trafilatura. That
also makes it work in libraries that call trafilatura internally. For example, datatrove's `Trafilatura` step is
accelerated by [datatrove-rs](https://github.com/Hexay/datatrove-rs).

Or call it directly:

```python
from trafilatura_rs import extract

text = extract(html, favor_precision=True)  # raises NotImplementedError for unsupported options
```

## What's supported

Plain-text `extract()` with `include_comments=False`, in these configurations (`trafilatura_rs.VERIFIED`):

| Configuration | Arguments | Identical to trafilatura |
|---|---|---|
| Default | | 10,666 / 10,666 |
| Precision (datatrove / FineWeb) | `favor_precision=True, deduplicate=True` | 10,666 / 10,666 |
| Recall | `favor_recall=True` | 10,666 / 10,666 |
| Fast | `no_fallback=True` | 10,666 / 10,666 |
| No tables | `include_tables=False` | 10,666 / 10,666 |

`deduplicate=True` is verified with the precision configuration (datatrove's call). Like trafilatura, it keeps a
process-global cache, so output depends on document order exactly as in Python; `trafilatura_rs.reset_caches()`
clears it.

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

## Contributing

Development setup, the parity harness, benchmarks and releasing: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0, like trafilatura. Vendored libxml2 is MIT-licensed; win-iconv (Windows builds) is public domain.
trafilatura-rs is an independent project and is not affiliated with the trafilatura authors.
