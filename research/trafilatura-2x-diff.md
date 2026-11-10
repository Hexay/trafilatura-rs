# trafilatura 1.11 → 2.2.0: semantic diff of the extract() path

Scope: `extract(html, include_comments=False, output_format="txt")` with the 5 parity configs (fineweb,
balanced, recall, fast, notables); no metadata, comments, links, images, formatting, target_language.
"same" = pure refactor (typing, formatting, renames). The Rust port (`crates/trafex/src/trafilatura2`) reuses
1.11 code only where this says "same".

Environment: both versions pin lxml 6.1.3 / libxml2 2.14.6, jusText 3.0.2, Python 3.12.3. The jusText stoplist
union (`jt_stoplist_init`, now built from `get_stoplists()` instead of a pickle) is identical to 1.11's (checked by
`ref2_stages.py stoplist`), so `justext::Stoplist::jt()` is reused.

## settings.cfg / settings.py
- settings.cfg: extraction values same (MIN_EXTRACTED_SIZE 250, MIN_OUTPUT_SIZE 1, MIN_OUTPUT_COMM_SIZE 1,
  MIN_EXTRACTED_COMM_SIZE 1, MIN_DUPLCHECK_SIZE 100, MAX_REPETITIONS 2). New `MAX_TREE_SIZE =` (empty → None,
  same effect as 1.11's default None); `max_tree_size` argument now raises ValueError (not used by the configs).
- `Extractor`: same fields/defaults for the ported options → `Options`/`Focus` reused.
- MANUALLY_CLEANED: **+ fencedframe, noindex**. MANUALLY_STRIPPED: **+ nobr**. CUT_EMPTY_ELEMS, TAG_CATALOG: same.
- BASIC_CLEAN_XPATH: **changed** — adds `div[contains(@class|@id,'footer')]`, fencedframe, svg, template and an
  EXSLT `re:test(..., 'i')` cookie/consent-banner match on @class/@id.
- New constants: MIN_DUPLICATE_LENGTH 50, DEDUPE_SCAN_CAP 200000, INLINE_CONSUMING/FORMATTABLE/CARRIED.
- `Document.commentsbody` now defaults to `Element("body")` (was None) — see core/determine_returnstring.

## xpaths.py
All content XPaths rewritten with EXSLT `re:test` (lxml implements it with Python `re`, taking the *first node of
the unsorted node-set*, so `re:test(@id|@class, …)` sees @id when present). Behaviour changes, not just syntax:
- BODY_XPATH: token sets changed (e.g. `post[-_]?content`, `(?:entry|page|text|article|art)-content`,
  `re:test(@class,'fulltext','i')`); expression 5 same.
- REMOVE_COMMENTS_XPATH: + `self::details`, regexes (`^[Cc]omment`, `(?:article|post)-comments`, …).
- OVERALL_DISCARD_XPATH: vocabulary regrouped; several matches change (e.g. `elated`, `avigation`, `nfoline`,
  `overlay`; `cookie` only on the first of @id|@class; role-nav only via translate(@role); paywall tokens moved in).
  Second expression reworded with `re:test`.
- PAYWALL_DISCARD_XPATH: **removed** (and its uses in extract/compare/justext).
- PRECISION_DISCARD_XPATH: `link` now a whole class token; `bottom` on @id|@class.
- TEASER_DISCARD_XPATH, DISCARD_IMAGE_ELEMENTS: same semantics.
- external.SANITIZED_XPATH: + fencedframe.
Port: generated verbatim by `harness/gen_xpaths2.py`; `re:test` added to the DOM XPath layer (`dom/exslt.rs`).

## utils.py
- DOCTYPE_TAG regex **changed** (`^< ?! ?DOCTYPE[^>]*/[^<>]*>`); `repair_faulty_html` **now first deletes**
  `[\x00-\x08\x0b\x0c\x0e-\x1f￾￿]`. FAULTY_HTML same. `load_html`: same flow (uses the new repair).
- `trim`: returns `""` for None (was None); callers now write `trim(x) or None`.
- `is_image_file`: **re.I** extension match and a >8192-char guard. New `is_image_element`.
- `textfilter`, RE_FILTER, `text_chars_test`, `remove_control_characters` (fast path only),
  `line_processing`, `sanitize`, `normalize_unicode`: same.
- New serializer helpers: `is_in_table_cell`, `is_last_element_in_cell`, `is_element_in_item`,
  `item_if_first_element`, `is_last_element_in_item`.

## deduplication.py
- LRUCache, `duplicate_test`: same (Simhash internals changed, not on this path) → reused.

## htmlprocessing.py
- `tree_cleaning`: **+ `table[@role="presentation" or @role="none"]` → div** (tables on); rest same;
  now called on a copy (core).
- `prune_html(tree, focus)`: `.//` instead of `//` (no effect); **tails dropped in precision mode**
  (`delete_element(keep_tail=focus!="precision")`).
- `prune_unwanted_nodes`: **tail handling changed** — `delete_element` (tail appended without the old `" "`
  separator; no AttributeError on detached nodes).
- `collect_link_info`: short-link threshold always 10 (was 50 in precision).
- `link_density_test`: **rewritten** — graphic guard, single-link shortcut, p limit no longer 200 in precision,
  large link-farm branch (LINK_FARM_RATIO 0.9, avg link len < 100).
- `link_density_test_tables`: threshold `< 200` → False (was `<= 250`); no-text links no longer force True.
- `delete_by_link_density`: backtracking deletes the element itself (was its children), depth threshold 1 in
  precision, `p` inside item/td/th kept.
- `handle_textnode`: + graphic bypass; `trim(...) or None`. `process_node`: `trim(...) or None`.
- `convert_tags`: link xpath `.//*[self::div or self::li or self::p]//a` (was div/ul); + Yoast FAQ strong → head;
  **empty sub/sup deleted**; `convert_quotes` + code-text indicators. Other converters same (reused).

## main_extractor.py
- `handle_titles`, `handle_other_elements`, `handle_code_blocks`, `is_text_element`: same (on new helpers).
- `handle_formatting`: returns None whenever process_node does (1.11 could raise TypeError).
- `update_elem_rendition`: skips empty rend. `define_newelem`: copies KEEP_ATTRS, optional inline children.
- `process_nested_elements`, `handle_lists` (no TypeError on None text), `is_code_block_element`,
  `handle_quotes` (walks all descendants, paragraphs, images, inline carry), `handle_paragraphs` (inline wrap,
  lb tails, graphic, no TypeError), `handle_textelem` (+del, graphic): **changed**.
- `handle_table`: **rewritten** (direct rows only, colspan/rowspan placeholders, captions, nested tables, width
  padding). `handle_image`: reachable via raw `<graphic>` tags.
- `recover_wild_text`: new search set, teasers kept in fast mode, **dedup against the existing body**.
- `prune_unwanted_sections`: no paywall pass, link density twice, table pruning moved here, trailing heads
  removed without tails.
- `_extract`: loop exits on >1 non-graphic element; trailing removal drops tails.
- `extract_content`: **drops adjacent duplicate elements** (>50 chars).

## external.py / readability_lxml.py / jusText
- `try_readability`: failure or unparsable summary → empty `HtmlElement()` (1.11: TypeError downstream).
- readability `summary`: **also drops fencedframe**; everything else same (reused, `with_drop_tags`).
- `compare_extraction`: no paywall prune; `_prefer_readability` adds recall ×1.5 rule; jusText accepted when
  `text2 and len_text <= 3*len2` (was 4×) and only then suppresses sanitize_tree.
- `custom_justext`: length_high 150 (200), max_link_density 0.25 (0.2), max_heading_distance 150 (200).
- `try_justext`: any exception → empty body. `justext_rescue`: basic_cleaning instead of paywall/comment prune.
- `sanitize_tree`: head role only on the first th-row per table; no dead findall loop.

## baseline.py
- `basic_cleaning`: new BASIC_CLEAN_XPATH. `baseline`: **rewritten** — JSON-LD walk (articleBody, reviewBody,
  recipe steps, FAQ answers, @graph/mainEntity, Product/VideoObject teasers, strict=False), Discourse preload,
  HTML-in-JSON rendering, top-level articles with a 1/5 cutoff, deduped paragraphs, teaser tier, body dump.
- `html2txt`: **rewritten** (block boundaries spaced, element input copied, no body → element itself).

## xml.py
- `delete_element`: + keep_tail. `merge_with_parent`, `remove_empty_elements`, `strip_double_tags`: same (not on
  the txt path).
- `xmltotxt` / `process_element` / `replace_element_text`: **rewritten** — list markers, cell/row rendering
  (`| … |`, head separator), block newlines, link text only when non-empty, `sanitize(…, preserve_space=True)`
  (lines no longer trimmed), markdown helpers for formatting.

## core.py
- `trafilatura_sequence` (new): forum detection (DiscussionForumPosting JSON-LD); comment prune on the **raw
  tree** unless a forum; cleaning on a copy; baseline rescue **skipped in precision**; **recall escalation**
  for balanced (recall retry vs jusText, gated by `html2txt` page share).
- `bare_extraction`: same checks. `extract`: **no longer catches RuntimeError** (exceptions propagate);
  output = `(xmltotxt(body) + "\n" + xmltotxt(empty comments body)).strip()` → **leading/trailing whitespace
  stripped**, then NFC.
