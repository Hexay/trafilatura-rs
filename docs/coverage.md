# What runs in Rust

Every call below gives the same result as the installed trafilatura (1.11.0 or 2.3.1) on all 10,666 pages of the
parity corpus, with and without comments, unless stated otherwise. Anything else runs the installed Python function.
`trafilatura_rs.supports(**kwargs)` and the `supports_*()` predicates say which path a call takes. Single documents
the port declines inside a covered call rerun in Python; see
[metadata-formats-scope.md](metadata-formats-scope.md#not-covered-stays-on-python).

## extract()

Input is a str, or bytes decoded by the installed trafilatura's own `decode_file` (as its `load_html` does). That
includes the trafilatura command line, which passes file contents as bytes with `options=Extractor(...)`.

Plain text (`trafilatura_rs.VERIFIED`):

| Configuration | Arguments |
|---|---|
| Default | |
| Precision | `favor_precision=True` |
| datatrove / FineWeb | `favor_precision=True, deduplicate=True` |
| Recall | `favor_recall=True` |
| Fast | `no_fallback=True` (2.3.1: also `fast=True`) |
| No tables | `include_tables=False` |

`favor_precision=True, favor_recall=True` is the recall call (trafilatura gives recall precedence).

Other outputs (`trafilatura_rs.verified_outputs()`):

| Output | Arguments | Focus |
|---|---|---|
| Markdown | `output_format="markdown"` (or `include_formatting=True` with txt) | default, precision, recall, fast |
| Text with metadata | `with_metadata=True` | default, precision, recall, fast |
| JSON, with or without metadata | `output_format="json"` | default, precision, recall, fast |
| Markdown with metadata | `output_format="markdown", with_metadata=True` | default |
| XML, HTML, CSV, with or without metadata | `output_format="xml"` / `"html"` / `"csv"` | default |
| Links | `include_links=True` with txt, markdown, JSON or XML, with or without metadata | default |
| Images | `include_images=True`, the same outputs | default |
| Links and images | both, with txt, and with JSON with metadata | default |

Relative link targets and image sources are resolved as courlan and `urllib.parse` do: natively for the common URL
shapes, through Python for the rest.

These options combine with any configuration above:

- `url`, `record_id`
- `deduplicate=True` (checked with precision and recall). Like trafilatura, it keeps a process-global cache, so
  output depends on document order exactly as in Python; `trafilatura_rs.reset_caches()` clears it.
- `options=Extractor(...)`, `config=` / `settingsfile=` (settings.cfg sizes, `MAX_TREE_SIZE` in 2.3.1,
  `EXTENSIVE_DATE_SEARCH`), `max_tree_size=` (1.11.0)
- `url_blacklist`, `author_blacklist`, `only_with_metadata`
- `prune_xpath` with a str or a list of str, as long as each expression selects elements
- `date_extraction_params` with `max_date` and optionally `original_date`, `extensive_search`
  (and `outputformat="%Y-%m-%d"`)

## Metadata

Title, author, date, sitename, categories, tags, license and JSON-LD run in Rust, including a port of htmldate
1.11.0's `find_date`. courlan's URL helpers are called in Python. htmldate's dateutil and dateparser calls are
answered by native ports at the `REPRODUCES_DATES` versions, except for rare strings, which run in Python; at other
versions they all run in Python. Details: [metadata-formats-scope.md](metadata-formats-scope.md).

Metadata needs trafilatura installed, Python 3.11+ and htmldate 1.11.0 (`REPRODUCES_METADATA`); otherwise the
call runs in Python. As in trafilatura, the date search is bounded by today's date.

## Other functions

```python
from trafilatura_rs import bare_extraction, baseline, extract_metadata, find_date, html2txt
```

`install()` and `TRAFILATURA_RS=1` swap these too, including the references trafilatura uses internally. Those
internal calls pass lxml trees, so they run in Python.

| Function | Runs in Rust | Predicate |
|---|---|---|
| `html2txt(content, clean=True)` | every call on str/bytes | `supports_html2txt` |
| `baseline(filecontent)` | every call on str/bytes | `supports_baseline` |
| `extract_metadata(filecontent, default_url, date_config, extensive, author_blacklist)` | `date_config` as `extract`'s `date_extraction_params` | `supports_extract_metadata` |
| `bare_extraction(filecontent, ...)` | `output_format="python"` (the default), the configurations in `VERIFIED_BARE`, with or without metadata, plus the options above | `supports_bare_extraction` |
| `htmldate.find_date(htmlobject, ...)` | `extensive_search`, `original_date`, `url`, `max_date` as str | `supports_find_date` |

`bare_extraction` returns trafilatura's own `Document` (1.11.0: its dict, as `as_dict=True` does by default) with
every slot filled from Rust. `body` and `commentsbody` are lxml trees rebuilt element by element from the Rust trees,
then checked: `etree.tostring` of each must equal the Rust tree's, or the call reruns in Python. The elements are
plain `etree._Element`s, where Python's trees hold some `lxml.html.HtmlElement` proxies. Pages with tag names lxml's
API refuses (`<ahref="x">` parses as a tag), and 1.11.0's errors on unparseable pages, rerun in Python: 29 to 74 of
the 10,666 pages per configuration.

`extract_metadata`, `bare_extraction` with metadata and `find_date` need the same versions as metadata in `extract`.

## Runs in Python

- `target_language`
- `with_metadata=True` with `deduplicate=True`: a document that falls back to Python would update the wrong dedup
  cache
- `bare_extraction` with `include_links`, `include_images` or `deduplicate`
- lxml trees, responses and other non-str/bytes input
- 2.3.1's `deduplicate` given a scoped `LRUCache`
- `prune_xpath` expressions selecting anything but elements
- `find_date` with URLs to download, `min_date`, another `outputformat` or `deferred_url_extractor`
- every call, when the installed versions are not ones trafilatura-rs reproduces (see the README, Exactness)

The full list, including single documents the port declines:
[metadata-formats-scope.md](metadata-formats-scope.md#not-covered-stays-on-python).
