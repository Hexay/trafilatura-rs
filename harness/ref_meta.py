"""Parity for the markdown / metadata / json configs: Python trafilatura 1.11 vs the Rust core (through the
extension module, so htmldate's dateutil/dateparser leaves run as callbacks into Python).
  ref_meta.py ref  docs.jsonl out.jsonl <cfg>    trafilatura.extract(html, include_comments=False, **CONFIGS[cfg])
  ref_meta.py rust docs.jsonl out.jsonl <cfg>    trafilatura_rs._core.extract with the same options, no fallback
  ref_meta.py show docs.jsonl <i> <cfg>          print both outputs of one doc
Wall-clock inputs are pinned in both modes: htmldate's max_date via date_extraction_params and dateutil's
default for missing fields (see docs/metadata-formats-scope.md)."""
import json
import sys
from datetime import datetime
from functools import partial

# max_date as set_date_params() would produce it on this day.
PINNED_DAY = "2026-06-30"
DATE_PARAMS = {"original_date": True, "extensive_search": True, "max_date": PINNED_DAY}

# extract() keyword sets (include_comments=False always); "url" True passes the record's URL. Mirrored in
# tests/parity.py.
CONFIGS = {
    "fineweb_md": {"favor_precision": True, "deduplicate": True, "output_format": "markdown"},
    "balanced_md": {"output_format": "markdown"},
    "balanced_meta": {"with_metadata": True, "url": True},
    "balanced_json": {"output_format": "json", "with_metadata": True},
    "precision_json": {"favor_precision": True, "output_format": "json", "with_metadata": True, "url": True},
    "json_text": {"output_format": "json"},
}


def pin_wall_clock():
    import htmldate.extractors as ex

    ex.dateutil_parse = partial(ex.dateutil_parse, default=datetime.fromisoformat(PINNED_DAY))


def kwargs_for(cfg, doc):
    kw = dict(CONFIGS[cfg])
    kw["url"] = doc["url"] if kw.pop("url", False) else None
    if kw.get("with_metadata"):
        kw["date_extraction_params"] = dict(DATE_PARAMS)
    return kw


def python_extract():
    from trafilatura import extract

    return lambda html, kw: extract(html, include_comments=False, **kw)


def rust_extract():
    import trafilatura_rs

    def run(html, kw):
        rust = trafilatura_rs._api._rust_kwargs(dict(kw, include_comments=False), "1.11.0")
        assert rust is not None, f"not covered by Rust: {kw}"
        return trafilatura_rs._core.extract(html, **rust)

    return run


def docs(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            yield json.loads(line)


def output(extract, doc, cfg):
    try:
        return extract(doc["html"], kwargs_for(cfg, doc))
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        return f"EXC:{type(e).__name__}"


def main():
    mode, path = sys.argv[1:3]
    pin_wall_clock()
    if mode == "show":
        i, cfg = int(sys.argv[3]), sys.argv[4]
        doc = next(d for d in docs(path) if d["i"] == i)
        for name, ex in (("python", python_extract()), ("rust", rust_extract())):
            print(f"== {name}\n{output(ex, doc, cfg)}")
        return
    out_path, cfg = sys.argv[3:5]
    extract = python_extract() if mode == "ref" else rust_extract()
    with open(out_path, "w", encoding="utf-8") as out:
        for d in docs(path):
            out.write(json.dumps({"i": d["i"], "out": output(extract, d, cfg)}) + "\n")


if __name__ == "__main__":
    main()
