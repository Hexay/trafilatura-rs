"""extract() keyword sets for harness/ref_meta.py, mirrored in tests/parity.py. Placeholders resolved per document by
`kwargs_for`: "url"/"record_id" True pass the record's URL / str(i); "options" builds a trafilatura `Extractor` (the
CLI path) from those keywords, then sets "options_set" attributes the way 1.11's `args_to_extractor` does; "config"
overrides settings.cfg keys in a copy of DEFAULT_CONFIG; "url_blacklist" True blacklists the record's URL on every
third record. "bytes" configs read the raw HTTP payload (`export_raw.py`) instead of the decoded HTML."""
from importlib.metadata import version

TRAFILATURA = version("trafilatura")
# 1.11's fast-mode flag; 2.x renamed it (no_fallback still works there, with a DeprecationWarning).
FAST = "no_fallback" if TRAFILATURA.startswith("1.") else "fast"
# max_date as set_date_params() would produce it on this day.
PINNED_DAY = "2026-06-30"
DATE_PARAMS = {"original_date": True, "extensive_search": True, "max_date": PINNED_DAY}

_FOCUS = {"precision": {"favor_precision": True}, "balanced": {}, "recall": {"favor_recall": True}, "fast": {FAST: True}}
_OUTPUTS = {
    "md": {"output_format": "markdown"},
    "meta": {"with_metadata": True, "url": True},
    "json": {"output_format": "json", "with_metadata": True, "url": True, "record_id": True},
    "jsontext": {"output_format": "json", "record_id": True},
    "xml": {"output_format": "xml", "record_id": True},
    "xmlmeta": {"output_format": "xml", "with_metadata": True, "url": True, "record_id": True},
}
# The first configs, kept under their names (and reference files).
_LEGACY = {
    "fineweb_md": {"favor_precision": True, "deduplicate": True, "output_format": "markdown"},
    "balanced_md": {"output_format": "markdown"},
    "balanced_meta": {"with_metadata": True, "url": True},
    "balanced_json": {"output_format": "json", "with_metadata": True},
    "precision_json": {"favor_precision": True, "output_format": "json", "with_metadata": True, "url": True},
    "json_text": {"output_format": "json"},
    "url_text": {"url": True},
}
AUTHOR_BLACKLIST = ["admin", "NAME", "Super User", "discuz team", "Unknown", "Administrator", "Інф «фактів",
                    "автор", "Shop", "Author", "Co KG"]
# Options outside the grid, each run once; they combine with any verified grid config.
_EXTRAS = {
    "bytes_default": {"bytes": True, "include_comments": True},
    "bytes_precision_json": {"bytes": True, "favor_precision": True, "output_format": "json", "with_metadata": True,
                             "url": True},
    "cli_bytes": {"bytes": True, "options": {}},
    "cli_md_bytes": {"bytes": True, "options": {"output_format": "markdown"}, "options_set": {"formatting": False}},
    "cli_json_meta": {"options": {"output_format": "json", "with_metadata": True, "comments": False}, "url": True},
    "config_small": {"config": {"MIN_EXTRACTED_SIZE": "400", "MIN_OUTPUT_SIZE": "60", "MIN_OUTPUT_COMM_SIZE": "30",
                                "MIN_DUPLCHECK_SIZE": "40", "MAX_REPETITIONS": "1", "MAX_TREE_SIZE": "40"},
                     "deduplicate": True, "include_comments": True},
    "fmt_txt": {"include_formatting": True, "include_comments": True},
    "blacklist_json": {"output_format": "json", "with_metadata": True, "url": True, "url_blacklist": True,
                       "author_blacklist": AUTHOR_BLACKLIST},
    "author_txt": {"author_blacklist": AUTHOR_BLACKLIST, "url": True},
    "only_meta": {"only_with_metadata": True, "url": True, "include_comments": True},
    "prune": {"prune_xpath": ["//footer", "//div[contains(@class, 'comment')]", "//p[1]"], "include_comments": True},
    "prune_json": {"prune_xpath": "//nav|//aside|//h1", "output_format": "json", "with_metadata": True, "url": True},
    "dates_noorig": {"with_metadata": True, "url": True,
                     "date_extraction_params": {"extensive_search": True, "max_date": PINNED_DAY}},
    "dates_noext": {"output_format": "json", "with_metadata": True, "url": True,
                    "date_extraction_params": {**DATE_PARAMS, "extensive_search": False}},
}
if TRAFILATURA.startswith("1."):  # 2.x raises on the argument (MAX_TREE_SIZE moved to settings.cfg)
    _EXTRAS["max_tree"] = {"max_tree_size": 40, "include_comments": True}


def _grid():
    for focus, fkw in _FOCUS.items():
        for out, okw in _OUTPUTS.items():
            kw = {**fkw, **okw}
            if focus in ("precision", "recall") and not kw.get("with_metadata"):
                kw["deduplicate"] = True  # datatrove-style; with metadata, deduplicate stays on Python
            yield f"{focus}_{out}", {**kw, "include_comments": False}
            yield f"{focus}_{out}_comments", {**kw, "include_comments": True}
    mdmeta = {"output_format": "markdown", "with_metadata": True, "url": True}
    yield "balanced_mdmeta", {**mdmeta, "include_comments": False}
    yield "balanced_mdmeta_comments", {**mdmeta, "include_comments": True}
    for fmt in ("html", "csv"):
        for name, meta in ((fmt, {}), (f"{fmt}meta", {"with_metadata": True, "url": True})):
            kw = {"output_format": fmt, "record_id": True, **meta}
            yield f"balanced_{name}", {**kw, "include_comments": False}
            yield f"balanced_{name}_comments", {**kw, "include_comments": True}


_FEATURES = {"links": {"include_links": True}, "images": {"include_images": True},
             "linksimages": {"include_links": True, "include_images": True}}
# every output with the page URL (links resolve relative targets against it); "nourl": plain text without it
_FEATURE_OUTPUTS = {
    "txt": {}, "nourl": {"url": False}, "md": _OUTPUTS["md"], "meta": _OUTPUTS["meta"],
    "mdmeta": {"output_format": "markdown", "with_metadata": True}, "json": _OUTPUTS["json"],
    "jsontext": _OUTPUTS["jsontext"], "xml": _OUTPUTS["xml"], "xmlmeta": _OUTPUTS["xmlmeta"],
}


def _feature_grid():
    """`<feature>_<out>[_comments]`: balanced focus, every output; `<feature>_<focus>_<out>`: the other focuses."""
    combos = [("balanced", out) for out in _FEATURE_OUTPUTS] + \
             [(f, out) for f in ("precision", "recall", "fast") for out in ("txt", "json")]
    for feature, fekw in _FEATURES.items():
        for focus, out in combos:
            kw = {"url": True, **fekw, **_FOCUS[focus], **_FEATURE_OUTPUTS[out]}
            name = f"{feature}_{out}" if focus == "balanced" else f"{feature}_{focus}_{out}"
            yield name, {**kw, "include_comments": False}
            yield f"{name}_comments", {**kw, "include_comments": True}


CONFIGS = {**dict(_grid()), **dict(_feature_grid()), **{k: {**v, "include_comments": False} for k, v in _LEGACY.items()},
           **{k: {"include_comments": False, **v} for k, v in _EXTRAS.items()}}


def _config(overrides):
    from configparser import ConfigParser

    from trafilatura.settings import DEFAULT_CONFIG

    config = ConfigParser()
    config.read_dict({s: dict(DEFAULT_CONFIG[s]) for s in DEFAULT_CONFIG})
    config["DEFAULT"].update(overrides)
    return config


def _extractor(spec, post, url):
    from trafilatura.settings import Extractor

    kw = dict(spec, url=url)
    if kw.get("with_metadata"):
        kw["date_params"] = dict(DATE_PARAMS)
    ext = Extractor(**kw)
    for key, value in post.items():
        setattr(ext, key, value)
    return ext


def kwargs_for(cfg, doc):
    """(input, extract() keywords) for config `cfg` on record `doc`."""
    kw = dict(CONFIGS[cfg])
    html = doc["raw"] if kw.pop("bytes", False) else doc["html"]
    url = doc["url"] if kw.pop("url", False) else None
    if "options" in kw:
        return html, {"options": _extractor(kw.pop("options"), kw.pop("options_set", {}), url), **kw}
    kw["url"] = url
    if kw.get("record_id"):
        kw["record_id"] = str(doc["i"])
    if "config" in kw:
        kw["config"] = _config(kw["config"])
    if kw.get("url_blacklist"):
        kw["url_blacklist"] = {"http://example.com/", *([doc["url"]] if doc["i"] % 3 == 0 else [])}
    # also pins the calls that only imply metadata (only_with_metadata, blacklists); ignored without metadata
    kw.setdefault("date_extraction_params", dict(DATE_PARAMS))
    return html, kw
