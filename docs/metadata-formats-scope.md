# Metadata, JSON and Markdown output: scope (trafilatura 1.11.0 and 2.3.1)

What `with_metadata=True`, `output_format="json"` and `output_format="markdown"` touch, and how the port keeps them
byte-exact. Written against 1.11.0; trafilatura 2.3.1 follows the same structure, and its changes are listed in
[trafilatura-23-diff.md](trafilatura-23-diff.md) (Markdown, Metadata, Output). Both versions resolve htmldate 1.11.0,
so the htmldate port, the courlan/dateutil/dateparser callbacks (with their native shortcuts) and every caveat below apply to both. 2.3.1 adds one
callback, `trafilatura.utils.safe_base_url`, and its `trafilatura2/` modules reuse the 1.11 code where behaviour is
unchanged. Its regexes, XPaths and `str.isalpha`/punctuation tables come from `harness/gen_meta23.sh`.

## Call graph

- `extract()` → `Extractor(...)`: `output_format="markdown"` sets `formatting=True` (the `include_formatting`
  rebinding in `extract()` itself is dead code). `with_metadata` is also implied by `only_with_metadata`,
  `url_blacklist` and `xmltei`.
- `bare_extraction()`: with metadata, `extract_metadata(tree, url, date_params, ...)` runs on the parsed tree
  *before* cleaning. It does not mutate the tree (author search and htmldate work on deep copies), so the text
  pipeline is unchanged; `document.url` only reaches `convert_tags` for `include_links`.
- `determine_returnstring()`:
  - txt/markdown: optional `---` header with `attr: str(value)` for title, author, url, hostname, description,
    sitename, date, categories, tags, fingerprint, id, license (truthy only; lists print as Python `repr`), then
    `xmltotxt(body, formatting)`; NFC over the whole string.
  - json: `document.id = record_id`, `fingerprint = simhash(str(title) + " " + str(raw_text))` (hex, blake2b-8 per
    token), then `json.dumps(..., ensure_ascii=False)` of either `{"text"}` or every `Document` slot renamed
    (`source`, `source-hostname`, `excerpt`, `;`-joined `categories`/`tags`, plus `raw_text`, `filedate`, ...).
    `raw_text` is extract_content's internal text, not `xmltotxt`.

## Metadata extraction (trafilatura/metadata.py, json_metadata.py)

- `examine_meta` (OpenGraph + `<meta>` names/properties/itemprops), JSON-LD (`json.loads` after a whitespace
  minify and `normalize_json` that rewrites `\uXXXX`, strips tags and collapses spaces; parse errors take a regex
  path), title (`h1`, XPaths, `<title>` split), author (XPaths on a pruned copy + `normalize_authors`: 10 regexes,
  `str.title()`, `isupper()`), canonical URL (courlan `validate_url`/`normalize_url`), hostname
  (`extract_domain(fast=True)`), image, date (htmldate), sitename (title split / URL), categories/tags (XPaths +
  `article:section`), license, `clean_and_trim` (`html.unescape` + `line_processing`, 10,000-char cap).
- Python-semantics needed: `json.loads` (NaN/Infinity, duplicate keys, strict control chars, error vs success
  boundary), dynamic typing in `process_parent` (`in` on str is substring, `TypeError`/`AttributeError` caught by a
  broad `except` that keeps partial mutations), `repr(list)`, `str.title/isupper/lower`, Python `re` (Unicode
  `\w`/`\d`/`\s`/`\b`, `re.I` equivalences such as `ı`/`İ`, lookbehinds, `$` before a final newline).
- Regexes are translated from the live `re` objects by `harness/gen_meta_regexes.py` (sre parse tree → fancy-regex,
  every class and case-folded literal expanded by testing all code points with Python's own `re`).

## htmldate 1.11.0 (largest piece)

`find_date(tree, original_date=True, extensive_search=True, max_date=..., url=...)`: URL date, `<meta>` header,
JSON-LD regexes, `abbr`/`time` elements, date-candidate elements (on a cleaned deep copy), timestamp/image/
idiosyncrasy regexes over the serialized tree, free-text segments, then `search_page` (copyright, 3/2/1-component
patterns with Counter-based candidate selection). Needs `datetime` semantics: constructor validation,
`fromisoformat` (3.11+ grammar), `strptime("%Y-%m-%d")`, `strftime`, naive `timestamp()` in local time.

Two leaves are not ported: `dateutil.parser.parse(s, fuzzy=False)` (strings starting with 4 digits that are not
ISO) and dateparser's `DateDataParser.get_date_data` (multilingual, ~200 locales). On the corpus they are reached on
~6% and ~8% of documents. The port calls back into Python for exactly these two functions
(`htmldate.extractors.dateutil_parse` and `htmldate.extractors._external_date`), so results stay byte-exact
without fallback; the pure-Rust crate (no Python) reports `Unsupported` instead. `has_date_cue` (the gate in front
of dateparser) needs dateparser's month names for all locales: generated table.

Native shortcuts (`crates/trafilatura-rs/src/dates`) answer the common cases in front of both callbacks and defer
everything else to Python:

- `dateutil.parser.parse`: exact port of `_timelex` + `_parse` + `_build_naive` for printable-ASCII strings. Defers on
  two-digit years, a lone weekday, time zone names dateutil warns about or resolves to `tzlocal`, offsets of a day
  or more, and a `default` other than dateutil's own or a `functools.partial` setting a naive one.
- `_external_date` (dateparser): `has_date_cue` natively, and a proof of `None` when no locale accepts the string:
  after `sanitize_date`, it contains a word in no locale's vocabulary. Words are runs of `[^\W\d_]` letters of the
  string as each locale reads it (numerals translated, NFKD without Mn, lowercased, folded case-insensitively;
  `harness/gen_dateparser_chars.py`), vocabularies are per locale, over all scripts (`harness/gen_dateparser_tables.py`).
  ja/zh/ko/he rewrites that may splice words or copy arbitrary letters apply only to strings with their letters;
  for those the locale is ruled out only by a letter it never uses. Any character the table cannot classify
  defers. Non-ASCII strings are tried only when the running `unicodedata` and `regex` versions are the ones the
  table was generated with. Strings some locale accepts always run dateparser.
- A per-process memo of `_external_date` results (`crates/trafilatura-rs-py/src/memo.rs`). With htmldate's parser
  a result depends on the string and, through two-digit years only, on the wall clock; entries live for one local
  day, results within 3 days of a century flip are not kept, and replacing `_external_date` or the parser object
  starts over. dateutil is not memoized: its Python fallbacks are exactly the clock-dependent cases (two-digit
  years, weekdays, `tzlocal`), and they cost ~0.01 ms/doc.

They are on only for the dateparser and python-dateutil versions in `REPRODUCES_DATES` (`_versions.py`); at any
other version the callbacks run in full, so the output still follows what is installed. `TRAFILATURA_RS_NATIVE_DATES=0`
turns them off. `harness/diff_dates.py check` compares every date string the corpus produces against Python;
`diff_dates.py memo` checks the memo's assumptions (call order, time of day) on them.

## courlan (called, not ported)

trafilatura's metadata uses four courlan helpers: `is_valid_url`, `get_base_url`, `validate_url` + `normalize_url`
and `extract_domain(fast=True)`. Each runs at most a few times per document on short strings, and an exact port
would drag in `urllib.parse`, Python's IDNA 2003 codec and the `tld` public-suffix list. They are called back into
Python through the same hook interface as the date leaves (`trafilatura_rs::hooks::Hooks`), looked up in
`trafilatura.metadata` at call time. That keeps them exact for any installed courlan version, so courlan is not
pinned.

## Wall-clock and environment dependence

- `max_date` defaults to `datetime.now().strftime("%Y-%m-%d")` (local date) and bounds every candidate date; the
  port computes the same local date. Parity references pin it with `date_extraction_params` (same dict shape as
  `set_date_params`), which the Python API also accepts.
- `dateutil.parser.parse` fills missing fields from today; the parity scripts pin its `default` in both the
  reference and the Rust run (the callback calls the same patched function).
- Naive `datetime.timestamp()` uses the local time zone; it only matters for candidates within a day of the
  bounds. Ported with the same `localtime` algorithm.
- `datetime.fromisoformat` accepts far less on Python 3.10, so the Rust metadata path requires Python ≥ 3.11.
- Of the packages metadata output depends on, only the ported one is pinned: htmldate, in `REPRODUCES_METADATA`
  and checked only for metadata calls. courlan, dateparser and python-dateutil are called, so whatever version is
  installed is what both engines use (dateparser and python-dateutil versions only gate the native shortcuts).

## Markdown

`formatting=True` changes extraction (`hi`/`del` kept by `convert_tags`, `handle_formatting`) and `xmltotxt`
(headings, emphasis, code fences, `␤` paragraph markers before `sanitize`). Already ported for 2.x-style code paths
but never verified for 1.11; verified by the new `*_md` parity configs.

## Not covered (stays on Python)

Whole calls, decided by `trafilatura_rs.supports()`:

- `include_links`, `include_images`, `xml`/`xmltei`/`csv`/`html` formats, `target_language`, json with
  `include_formatting=True`, input other than str/bytes (lxml trees, responses).
- `date_extraction_params` with keys beyond `original_date`, `extensive_search`, `max_date` (`"YYYY-MM-DD"`, required)
  and `url`, or a custom `outputformat` / `min_date`.
- Blacklists other than a set/frozenset/list/tuple of str; `prune_xpath` other than str or a list/tuple of str.
- 2.3.1 with a truthy `max_tree_size` argument (Python raises; `MAX_TREE_SIZE` in a config is covered).
- `with_metadata=True` together with `deduplicate=True`: a document that falls back to Python would update
  Python's dedup cache instead of Rust's, changing later results.
- Metadata on Python < 3.11 (`fromisoformat` grammar), and metadata without trafilatura installed (the callbacks
  need it).
- Configuration × output combinations not in `VERIFIED_OUTPUTS`.

Single documents inside a covered call (`Unsupported` raised by the core, then rerun in Python). None of these
occur on the 10,666-page corpus:

- `str.title()` on a string containing `Σ` (Final_Sigma context rule not ported); with an `author_blacklist`,
  `str.lower()` on an author with cased letters outside long-stable Unicode blocks, or with a `Σ` whose nearest
  neighbor (past `'.:^` and backtick) is not ASCII, whitespace or a Latin, Greek or Cyrillic letter.
- A `prune_xpath` expression that errors or selects anything but non-root elements (strings, attributes, numbers),
  or uses `re:` (unbound for a bare `XPath(expr)` in lxml).
- A str with lone surrogates (lxml's parser takes them; Rust strings cannot).
- 2.3.1: a truthy non-str sitename (Python raises AttributeError from `lstrip`).
- `\N{...}` escapes or lone surrogates produced by `normalize_authors`' `unicode_escape` decode.
- JSON-LD nested deeper than 900 levels (Python's RecursionError limit) or with lone `\uD800`-style surrogates.
- Python exceptions that `extract()` lets escape (odd JSON-LD sitename types, `';'.join` over non-str
  categories, an x-default `<link>` without `href`): the fallback raises the identical exception.
