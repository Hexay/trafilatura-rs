# trafilatura-rs

[![PyPI](https://img.shields.io/pypi/v/trafilatura-rs.svg)](https://pypi.org/project/trafilatura-rs/)
[![Python versions](https://img.shields.io/pypi/pyversions/trafilatura-rs.svg)](https://pypi.org/project/trafilatura-rs/)
[![CI](https://github.com/Hexay/trafilatura-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Hexay/trafilatura-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/Hexay/trafilatura-rs/blob/main/LICENSE)

A Rust port of [trafilatura](https://github.com/adbar/trafilatura)'s extraction that returns the same output, byte
for byte, as trafilatura 1.11.0 or 2.3.1, using 8 to 11 times less CPU for text extraction.

<p align="center">
  <img src="https://raw.githubusercontent.com/Hexay/trafilatura-rs/main/docs/assets/extract-speed.svg"
       alt="CPU per page with datatrove's call: trafilatura 1.11.0 15.77 ms, trafilatura-rs 1.44 ms; trafilatura 2.3.1 13.72 ms, trafilatura-rs 1.65 ms; rs-trafilatura 0.1.1 11.07 ms" width="720">
</p>

## Install

```bash
pip install "trafilatura-rs[trafilatura]"    # trafilatura 1.11.0
pip install "trafilatura-rs[trafilatura2]"   # trafilatura 2.3.1
```

trafilatura-rs reproduces whichever of the two is installed. The extras pin the versions it is checked against:

| Package | `[trafilatura]` | `[trafilatura2]` |
|---|---|---|
| trafilatura | 1.11.0 | 2.3.1 |
| lxml | 6.1.3 | 6.1.3 |
| htmldate (metadata) | 1.11.0 | 1.11.0 or newer, via trafilatura 2.3.1 |

htmldate 1.11.0 and trafilatura 2.3.1 require lxml 6.1.3 or newer, so the extras install 6.1.3. Metadata runs in
Rust with htmldate 1.11.0 only (see Exactness). Wheels are built for
Linux (glibc and musl, x86_64 and aarch64), macOS (x86_64 and arm64) and Windows (x64 and arm64), Python 3.10+, with
libxml2 statically linked.

## Quickstart

Run existing code unchanged:

```bash
TRAFILATURA_RS=1 python my_script.py    # or: python -m trafilatura_rs my_script.py
```

Or in code:

```python
import trafilatura_rs
trafilatura_rs.install()            # patches trafilatura, including copies imported earlier

from trafilatura_rs import extract  # or import the drop-in directly; same signature
```

Calls trafilatura-rs covers run in Rust; every other call runs Python trafilatura as before. That includes calls
made inside other libraries, such as datatrove's `Trafilatura` step (or use
[datatrove-rs](https://github.com/Hexay/datatrove-rs)), and trafilatura's command line.

`trafilatura_rs.supports(**kwargs)` says whether an `extract` call runs in Rust; `supports_bare_extraction`,
`supports_baseline`, `supports_html2txt`, `supports_extract_metadata` and `supports_find_date` do the same for the
other functions. `trafilatura_rs.engine()` returns the trafilatura version being reproduced.

## What runs in Rust

- `extract()` on str or bytes, with `output_format` txt, markdown, json, xml, html or csv, with or without metadata,
  comments, `include_links` and `include_images`, and the usual options (`favor_precision`, `favor_recall`,
  `no_fallback`/`fast`, `include_tables`, `deduplicate`, `url`, `options=Extractor(...)`, config files, blacklists,
  `prune_xpath`, `date_extraction_params`)
- metadata: title, author, date (a port of htmldate's `find_date`, with native ports of the dateutil and dateparser
  calls it makes; rare strings go to Python), sitename, categories, tags, license, JSON-LD
- `html2txt`, `baseline`, `extract_metadata`, `bare_extraction` and htmldate's `find_date`
- trafilatura's command line, which calls `extract` with bytes

Python trafilatura still runs:

- `target_language`
- `with_metadata=True` together with `deduplicate=True`
- `bare_extraction` with `include_links`, `include_images` or `deduplicate`
- lxml trees and other non-str/bytes input
- 2.3.1's `deduplicate` given a scoped `LRUCache`
- `prune_xpath` expressions that select anything but elements
- every call when the installed versions don't match (see below)

Which configurations each output is verified for, and the details per function: [docs/coverage.md](https://github.com/Hexay/trafilatura-rs/blob/main/docs/coverage.md).

## Exactness

Every covered configuration is checked page by page against trafilatura 1.11.0 and 2.3.1 on 10,666 Common Crawl
pages, comparing the final output and per-stage tree hashes; any difference is treated as a bug. The output matches
because the parser does: trafilatura-rs statically links libxml2 2.14.6, the version lxml 6.1.3 bundles, with the
same parsing options ([PATCHES.md](https://github.com/Hexay/trafilatura-rs/blob/main/crates/libxml2-sys/PATCHES.md)).
Documents the port declines inside a covered call rerun in Python.

Three version guards keep results exact when other packages are installed:

| Guard | Pins | When the installed version differs |
|---|---|---|
| `REPRODUCES` | (trafilatura, lxml) pairs: 1.11.0 or 2.3.1 with lxml 6.0.0 to 6.1.3 | every call runs Python; `install()` warns and patches nothing |
| `REPRODUCES_METADATA` | htmldate 1.11.0 | metadata calls and `find_date` run Python |
| `REPRODUCES_DATES` (`trafilatura_rs._versions`) | dateparser 1.5.0, python-dateutil 4.0.0 | the native date parsers are off; htmldate's dateparser and dateutil calls run in Python |

trafilatura's output under each lxml 6.0.x and 6.1.x release equals its output under 6.1.3 on the full corpus,
which is why `REPRODUCES` accepts them. Metadata also needs Python 3.11+. The differences between the two trafilatura
versions: [docs/trafilatura-2x-diff.md](https://github.com/Hexay/trafilatura-rs/blob/main/docs/trafilatura-2x-diff.md),
[docs/trafilatura-23-diff.md](https://github.com/Hexay/trafilatura-rs/blob/main/docs/trafilatura-23-diff.md).

[rs-trafilatura](https://pypi.org/project/rs-trafilatura/) is a different project: an independent extractor inspired
by trafilatura rather than a port. With datatrove's call its output equals trafilatura 1.11.0's on 171 of the 10,666
pages.

## Speed

CPU time per page, Python trafilatura divided by trafilatura-rs, single thread. The first two rows are fresh runs on
the full corpus, median of 3 ([docs/benchmarks.md](https://github.com/Hexay/trafilatura-rs/blob/main/docs/benchmarks.md) has the times, instruction counts and machine
load). The other rows are the results recorded in the pull request that added them.

| Call | 1.11.0 | 2.3.1 | Pages | Source |
|---|---:|---:|---:|---|
| datatrove's call: `favor_precision=True, include_comments=False, deduplicate=True` | 11.0x | 8.3x | 10,666 | [benchmarks](https://github.com/Hexay/trafilatura-rs/blob/main/docs/benchmarks.md) |
| `extract(html)` | 8.8x | 8.5x | 10,666 | [benchmarks](https://github.com/Hexay/trafilatura-rs/blob/main/docs/benchmarks.md) |
| `output_format="markdown"` | 8.5x | 12.9x | 3,000 | [#7](https://github.com/Hexay/trafilatura-rs/pull/7) |
| command line (bytes, `options=Extractor()`) | 8.2x | 9.5x | 3,000 | [#7](https://github.com/Hexay/trafilatura-rs/pull/7) |
| `with_metadata=True, url=...` | 2.6x | 3.5x | 3,000 | [#7](https://github.com/Hexay/trafilatura-rs/pull/7) |
| `output_format="json", with_metadata=True, include_comments=False` | 3.5x | 4.2x | 3,000 | [#14](https://github.com/Hexay/trafilatura-rs/pull/14) |
| `output_format` xml, html, csv (`include_comments=False`) | 9.9–10.4x | 8.4–9.2x | 3,000 | [#13](https://github.com/Hexay/trafilatura-rs/pull/13) |
| `include_links` / `include_images`, without metadata | 7.2–9.8x | 7.4–8.4x | 3,000 | [#13](https://github.com/Hexay/trafilatura-rs/pull/13) |
| `bare_extraction(html)`, each focus, with and without comments | 6.8–10.1x | 6.2–8.0x | 10,666 | [#10](https://github.com/Hexay/trafilatura-rs/pull/10) |
| `bare_extraction(html, with_metadata=True, url=...)` | 2.4–3.2x | 3.0–3.6x | 10,666 | [#10](https://github.com/Hexay/trafilatura-rs/pull/10) |
| `html2txt(html)` / `baseline(html)` | 2.2x | 4.2–4.3x | 10,666 | [#10](https://github.com/Hexay/trafilatura-rs/pull/10) |
| `extract_metadata(html)`, with and without `default_url` | 1.6–1.7x | 1.9–2.0x | 10,666 | [#10](https://github.com/Hexay/trafilatura-rs/pull/10) |
| `htmldate.find_date(html)`, with and without `url` | 2.0–2.6x | 2.0–2.6x | 10,666 | [#10](https://github.com/Hexay/trafilatura-rs/pull/10) |

The `bare_extraction` speedups leave out the 29 to 74 pages per configuration that rerun in Python.

## Contributing

Development setup, the parity harness, benchmarks and releasing:
[CONTRIBUTING.md](https://github.com/Hexay/trafilatura-rs/blob/main/CONTRIBUTING.md).

## License

Apache-2.0, like trafilatura. Vendored libxml2 is MIT-licensed; win-iconv (Windows builds) is public domain.
trafilatura-rs is an independent project and is not affiliated with the trafilatura authors.
