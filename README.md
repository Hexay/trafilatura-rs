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
pip install "trafilatura-rs[trafilatura]"    # trafilatura 1.11.0
pip install "trafilatura-rs[trafilatura2]"   # trafilatura 2.3.1
```

trafilatura-rs reproduces whichever of the two is installed (2.3.1 when neither is). The extras install the versions
it is verified against (`[trafilatura]`: trafilatura 1.11.0, lxml 6.0.0–6.1.3, htmldate 1.11.0;
`[trafilatura2]`: trafilatura 2.3.1, lxml 6.0.0–6.1.3; htmldate 1.11.0 itself requires lxml 6.1.3, and with an
older htmldate metadata calls stay on Python), so you can fall back to and compare with the original. Wheels
are built for Linux (glibc and musl, x86_64 and aarch64), macOS and Windows (x64 and arm64), Python 3.10+, with
libxml2 statically linked.

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

`trafilatura_rs.supports(**kwargs)` tells you which path a call takes, and `trafilatura_rs.engine()` which
trafilatura version is reproduced. If your installed trafilatura or lxml isn't a pair trafilatura-rs reproduces
(`trafilatura_rs.REPRODUCES`), every call stays on Python and `install()` warns, so results never change. Markdown,
JSON and metadata output are covered too (see below).

## What's supported

`extract()` on str or bytes, in the configurations below (`trafilatura_rs.verified_outputs()`). Other arguments and
output formats run Python trafilatura. bytes are decoded by the installed trafilatura's own `decode_file` (as its
`load_html` does), then extracted in Rust; that includes the trafilatura command line, which passes file contents
as bytes with `options=Extractor(...)`.

Plain text, identical pages out of 10,666, with comments (`include_comments=True`, trafilatura's default) and
without:

| Configuration | Arguments | 1.11.0 | 1.11.0, no comments | 2.3.1 | 2.3.1, no comments |
|---|---|---|---|---|---|
| Default | | 10,666 | 10,666 | 10,666 | 10,666 |
| Precision | `favor_precision=True` | 10,666 | 10,666 | 10,666 | 10,666 |
| datatrove / FineWeb | `favor_precision=True, deduplicate=True` | 10,666 | 10,666 | 10,666 | 10,666 |
| Recall | `favor_recall=True` | 10,666 | 10,666 | 10,666 | 10,666 |
| Fast | `no_fallback=True` (2.3.1: also `fast=True`) | 10,666 | 10,666 | 10,666 | 10,666 |
| No tables | `include_tables=False` | 10,666 | 10,666 | 10,666 | 10,666 |

With 2.3.1, `deduplicate` given an `LRUCache` runs Python trafilatura.

Markdown, JSON and metadata: every output below with each of default, `favor_precision=True`, `favor_recall=True`
and fast (`no_fallback`/`fast`), with and without comments, gives identical pages, 10,666 out of 10,666, on both
1.11.0 and 2.3.1:

| Output | Arguments |
|---|---|
| Markdown | `output_format="markdown"` (or `include_formatting=True` with txt) |
| Text with metadata | `with_metadata=True` |
| JSON | `output_format="json"` |
| JSON with metadata | `output_format="json", with_metadata=True` |
| Markdown with metadata | `output_format="markdown", with_metadata=True` (default focus only) |

With the default focus, with and without comments, these also give identical pages on both versions:

| Output | Arguments |
|---|---|
| XML | `output_format="xml"`, with or without `with_metadata=True` |
| HTML | `output_format="html"`, with or without `with_metadata=True` |
| CSV | `output_format="csv"`, with or without `with_metadata=True` |
| Links | `include_links=True` with txt, markdown, JSON and XML, each with and without metadata |
| Images | `include_images=True`, the same outputs |
| Links and images | both, with txt and with JSON with metadata |

Relative link targets and image sources are resolved like courlan and `urllib.parse` do, natively for the common
URL shapes (identical under Python 3.12 to 3.14 on the corpus' million links) and through Python for the rest.

`url` and `record_id` can be passed with any of them. `deduplicate=True` is verified with the precision and recall
configurations. Like trafilatura, it keeps a process-global cache, so output depends on document order exactly as in
Python; `trafilatura_rs.reset_caches()` clears it.

These options combine with any configuration above, each verified on the full corpus for both versions:

- `options=Extractor(...)`, `config=` / `settingsfile=` (settings.cfg sizes, `MAX_TREE_SIZE` in 2.3.1,
  `EXTENSIVE_DATE_SEARCH`), `max_tree_size=` (1.11)
- `url_blacklist`, `author_blacklist`, `only_with_metadata`, `prune_xpath` (expressions selecting elements; anything
  else runs Python)
- `date_extraction_params` with `original_date`, `extensive_search`, `max_date` (and `outputformat="%Y-%m-%d"`)
- bytes input

A call with `favor_precision=True, favor_recall=True` is the recall call (trafilatura gives recall precedence).

### Metadata

```python
trafilatura.extract(html, with_metadata=True, url=url)                  # Rust: text with a --- header
trafilatura.extract(html, output_format="json", with_metadata=True)    # Rust
trafilatura.extract(html, with_metadata=True, deduplicate=True)        # Python
```

Title, author, date, sitename, categories, tags, license and the JSON-LD parsing run in Rust, including a port of
htmldate 1.11's `find_date` (the htmldate both trafilatura versions resolve). A few leaves run in Python: courlan's
URL helpers, and the dateutil and dateparser calls htmldate makes for unusual date strings, which native ports answer
except for rare cases (for the dateparser and python-dateutil versions in `REPRODUCES_DATES`; at other versions they
run in Python, so results stay exact). Metadata calls therefore need trafilatura installed and Python 3.11+. They
also need htmldate 1.11.0 (`REPRODUCES_METADATA`); with another version they stay on Python.

As in trafilatura, the date search is bounded by today's date (`max_date`). `date_extraction_params` is supported
when it sets `max_date` and optionally `original_date` / `extensive_search`. `with_metadata=True` with `deduplicate=True` stays on Python, because a document that
needs Python would update the wrong deduplication cache. Rare documents the port leaves to Python (listed in
[docs/metadata-formats-scope.md](docs/metadata-formats-scope.md)) are rerun there automatically. None occur in the
10,666-page corpus.

### Other functions

The rest of trafilatura's public functions, and htmldate's `find_date`, are drop-ins on the same terms: str or bytes
input runs in Rust; anything else (lxml trees, responses, uncovered arguments) runs the installed Python function,
and so does any document the port declines. `install()` and `TRAFILATURA_RS=1` swap them too, including the
references trafilatura uses internally (those calls pass lxml trees, so they stay on Python).

```python
from trafilatura_rs import bare_extraction, baseline, extract_metadata, find_date, html2txt
```

| Function | Runs in Rust | Predicate |
|---|---|---|
| `html2txt(content, clean=True)` | every call on str/bytes | `supports_html2txt` |
| `baseline(filecontent)` | every call on str/bytes; same lxml `<body>` | `supports_baseline` |
| `extract_metadata(filecontent, default_url, date_config, extensive, author_blacklist)` | `date_config` as `extract`'s `date_extraction_params` | `supports_extract_metadata` |
| `bare_extraction(filecontent, ...)` | `output_format="python"` (the default), the configurations in `VERIFIED_BARE`, the option extras above; not `deduplicate` | `supports_bare_extraction` |
| `htmldate.find_date(htmlobject, ...)` | `extensive_search`, `original_date`, `url`, `max_date` as str; not URLs to download, `min_date`, another `outputformat` or `deferred_url_extractor` | `supports_find_date` |

`bare_extraction` returns trafilatura's own `Document` (1.11: its dict, as `as_dict=True` does by default) with every
slot filled from Rust. `body` and `commentsbody` are lxml trees rebuilt element by element from the Rust trees, then
checked: `etree.tostring` of each must equal the Rust tree's, or the call reruns in Python. Elements are plain
`etree._Element`s; in Python's trees some are `lxml.html.HtmlElement` proxies, depending on which elements were moved
from the parsed page. Tag names lxml's API refuses (`<ahref="x">` parses as a tag) also rerun in Python.
`extract_metadata`, `bare_extraction` with metadata and `find_date` need the versions metadata in `extract` needs.

On the 10,666-page corpus every call below gives the same result as Python on every page; "Python reruns" counts
the pages that fall back (names lxml's API refuses, and 1.11's error on unparseable pages). CPU time per page, Rust
path vs the installed Python function in the same process:

| Call | 1.11.0 speedup | 2.3.1 speedup | Python reruns (1.11.0 / 2.3.1) |
|---|---|---|---|
| `html2txt(html)` | 2.2x | 4.3x | 0 / 0 |
| `html2txt(html, clean=False)` | 2.1x | 2.5x | 0 / 0 |
| `baseline(html)` | 2.2x | 4.2x | 0 / 0 |
| `extract_metadata(html)` | 1.7x | 2.0x | 0 / 0 |
| `extract_metadata(html, default_url=url)` | 1.6x | 1.9x | 0 / 0 |
| `htmldate.find_date(html)` | 2.5x | 2.5x | 0 / 0 |
| `htmldate.find_date(html, url=url)` | 2.0x | 2.0x | 0 / 0 |
| `htmldate.find_date(html, extensive_search=False, original_date=True)` | 2.6x | 2.6x | 0 / 0 |
| `bare_extraction(html)` | 7.6x | 8.0x | 73 / 41 |
| `bare_extraction(html, favor_precision=True)` | 10.0x | 7.9x | 74 / 40 |
| `bare_extraction(html, favor_recall=True)` | 7.5x | 6.6x | 73 / 43 |
| `bare_extraction(html, no_fallback=True)` (2.3.1: `fast=True`) | 7.0x | 7.3x | 65 / 31 |
| `bare_extraction(html, include_comments=False)`, each focus | 6.8–10.1x | 6.2–7.7x | 64–73 / 29–41 |
| `bare_extraction(html, with_metadata=True, url=url)`, each focus | 2.4–3.2x | 3.0–3.6x | as without metadata |

Speedups exclude the reruns, which cost the Rust attempt plus the Python call.

## Exactness

Each configuration is checked page by page against trafilatura 1.11.0 and 2.3.1 on 10,666 Common Crawl pages,
comparing final output and per-stage tree hashes. The jusText and readability fallbacks are checked separately.
Exactness holds because the parser is the same: trafilatura-rs vendors and statically links libxml2 2.14.6, the
version lxml 6.1.3 bundles, with the same parsing options (see [PATCHES.md](crates/libxml2-sys/PATCHES.md)).
trafilatura's output under every other lxml 6.0.x/6.1.x release (libxml2 2.14.4 to 2.14.6) is identical to its
output under 6.1.3 on the full corpus (default, FineWeb, markdown-with-comments and JSON-with-metadata calls, both
versions); any other lxml runs Python.
What changed between the two versions: [docs/trafilatura-2x-diff.md](docs/trafilatura-2x-diff.md) and
[docs/trafilatura-23-diff.md](docs/trafilatura-23-diff.md).

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

trafilatura 2.3.1 (first 3,000 pages, `V=23 tools/bench.sh 3000`, mean CPU time per page):

| Call | trafilatura 2.3.1 | trafilatura-rs | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|
| datatrove / FineWeb (`favor_precision=True, deduplicate=True`) | 10.58 ms | 2.51 ms | 4.2x | 4.1x |
| `extract(html)` | 12.92 ms | 2.63 ms | 4.9x | 5.0x |

Markdown, metadata and JSON (first 3,000 pages, `include_comments=False`, `BENCH_CFG=<cfg> tools/bench.sh 3000`):

| Call | trafilatura 1.11 | trafilatura-rs | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|
| `output_format="markdown"` | 8.36 ms | 1.58 ms | 5.3x | 6.5x |
| `with_metadata=True, url=...` | 18.45 ms | 6.77 ms | 2.7x | 3.0x |
| `output_format="json", with_metadata=True` | 23.41 ms | 6.74 ms | 3.5x | 3.4x |

The same calls against trafilatura 2.3.1 (`V=23 BENCH_CFG=<cfg> tools/bench.sh 3000`):

| Call | trafilatura 2.3.1 | trafilatura-rs | Speedup (CPU) | Speedup (instructions) |
|---|---:|---:|---:|---:|
| `output_format="markdown"` | 18.70 ms | 3.88 ms | 4.8x | 4.4x |
| `with_metadata=True, url=...` | 41.07 ms | 16.14 ms | 2.5x | 2.5x |
| `output_format="json", with_metadata=True` | 27.08 ms | 6.50 ms | 4.2x | 3.6x |

With comments (trafilatura's default), and the command line's call (`extract(bytes, options=Extractor())`), first
3,000 pages:

| Call | trafilatura 1.11 | trafilatura-rs | Speedup (CPU) | trafilatura 2.3.1 | trafilatura-rs | Speedup (CPU) |
|---|---:|---:|---:|---:|---:|---:|
| `output_format="markdown"` | 9.65 ms | 1.14 ms | 8.5x | 21.08 ms | 1.64 ms | 12.9x |
| `with_metadata=True, url=...` | 30.69 ms | 11.75 ms | 2.6x | 37.92 ms | 10.90 ms | 3.5x |
| `output_format="json", with_metadata=True, url=..., record_id=...` | 26.01 ms | 8.72 ms | 3.0x | 39.11 ms | 10.19 ms | 3.8x |
| command line, bytes input | 16.26 ms | 1.98 ms | 8.2x | 20.51 ms | 2.16 ms | 9.5x |

Mean CPU time per page. The dateparser calls htmldate makes for unusual date strings run natively except for rare
strings (see [docs/metadata-formats-scope.md](docs/metadata-formats-scope.md)); with them in Python
(`BENCH_PY_DATEPARSER=1`), the JSON call takes 7.77 ms (1.11) and 7.19 ms (2.3.1).

Other output formats, links and images (first 3,000 pages, `include_comments=False`, mean CPU time per page,
`[V=23] BENCH_CFG=<cfg> tools/bench.sh 3000`):

| Call | trafilatura 1.11 | trafilatura-rs | Speedup (CPU) | trafilatura 2.3.1 | trafilatura-rs | Speedup (CPU) |
|---|---:|---:|---:|---:|---:|---:|
| `output_format="xml"` | 12.64 ms | 1.21 ms | 10.4x | 15.57 ms | 1.80 ms | 8.7x |
| `output_format="xml", with_metadata=True` | 26.01 ms | 8.86 ms | 2.9x | 29.60 ms | 7.36 ms | 4.0x |
| `output_format="html"` | 13.73 ms | 1.38 ms | 9.9x | 14.39 ms | 1.57 ms | 9.2x |
| `output_format="csv"` | 11.52 ms | 1.11 ms | 10.4x | 13.12 ms | 1.57 ms | 8.4x |
| `include_links=True` | 11.66 ms | 1.62 ms | 7.2x | 19.36 ms | 2.63 ms | 7.4x |
| `include_links=True, output_format="xml"` | 12.26 ms | 1.25 ms | 9.8x | 14.63 ms | 1.73 ms | 8.4x |
| `include_links=True, output_format="json", with_metadata=True` | 24.77 ms | 8.69 ms | 2.9x | 36.29 ms | 8.58 ms | 4.2x |
| `include_images=True, output_format="markdown"` | 12.31 ms | 1.36 ms | 9.0x | 15.17 ms | 2.01 ms | 7.6x |
| `include_links=True, include_images=True` | 9.22 ms | 1.15 ms | 8.0x | 15.52 ms | 2.05 ms | 7.6x |

## Contributing

Development setup, the parity harness, benchmarks and releasing: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0, like trafilatura. Vendored libxml2 is MIT-licensed; win-iconv (Windows builds) is public domain.
trafilatura-rs is an independent project and is not affiliated with the trafilatura authors.
