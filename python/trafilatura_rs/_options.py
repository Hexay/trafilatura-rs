"""extract() arguments → `_core.extract` keywords. Each trafilatura version first builds an `Extractor` (from the
keywords, a config / settings file, or an `options=` instance); this mirrors that step, then maps the result onto the
paths the port has, or returns None (the call runs Python trafilatura)."""
import importlib
import re
from datetime import datetime

_FORMATS = {"txt", "markdown", "json"}
_ISO_DAY = re.compile(r"\d{4}-\d{2}-\d{2}", re.ASCII)
_FOCUS = {"precision", "recall", "balanced"}
# settings.cfg values of the Extractor size attributes, the port's defaults (1.11's MIN_EXTRACTED_COMM_SIZE only logs)
_SIZES = {"min_extracted_size": 250, "min_output_size": 1, "min_output_comm_size": 1, "min_duplcheck_size": 100,
          "max_repetitions": 2}
_DATE_KEYS = {"original_date", "extensive_search", "max_date", "url", "outputformat"}
# extract()'s parameters and defaults in both versions (include_formatting: 1.11's False acts like 2.x's None)
_PARAMS = {
    "url": None, "record_id": None, "no_fallback": False, "favor_precision": False, "favor_recall": False,
    "include_comments": True, "output_format": "txt", "tei_validation": False, "target_language": None,
    "include_tables": True, "include_images": False, "include_formatting": None, "include_links": False,
    "deduplicate": False, "date_extraction_params": None, "with_metadata": False, "only_with_metadata": False,
    "max_tree_size": None, "url_blacklist": None, "author_blacklist": None, "settingsfile": None,
    "prune_xpath": None, "config": None, "options": None,
}
_PARAMS_2 = {**_PARAMS, "fast": False}
_EXTRACTOR_FIELDS = ("format", "fast", "focus", "comments", "formatting", "links", "images", "tables", "dedup", "lang",
                     "max_tree_size", "url", "with_metadata", "only_with_metadata", "date_params",
                     "author_blacklist", "url_blacklist", "max_file_size", *_SIZES)
_UNSUPPORTED = object()


def _settings():
    try:
        return importlib.import_module("trafilatura.settings")
    except ImportError:
        return None


def _spec(kw, v1):
    """The Extractor attributes extract() derives from its keywords under the default config."""
    fmt, formatting = kw["output_format"], kw["include_formatting"]
    implied = kw["only_with_metadata"] or kw["url_blacklist"] or (not v1 and kw["author_blacklist"])
    return {
        "format": fmt,
        "fast": kw["no_fallback"] if v1 else kw["fast"] or kw["no_fallback"],
        "focus": "recall" if kw["favor_recall"] else "precision" if kw["favor_precision"] else "balanced",
        "comments": kw["include_comments"],
        # 1.11 forces formatting for markdown; 2.x honors an explicit False
        "formatting": (formatting or fmt == "markdown") if v1 or formatting is None else formatting,
        "links": kw["include_links"], "images": kw["include_images"], "tables": kw["include_tables"],
        "dedup": kw["deduplicate"], "lang": kw["target_language"],
        "max_tree_size": kw["max_tree_size"] if v1 else None,
        "url": kw["url"], "with_metadata": kw["with_metadata"] or implied,
        "only_with_metadata": kw["only_with_metadata"], "date_params": kw["date_extraction_params"],
        "author_blacklist": kw["author_blacklist"], "url_blacklist": kw["url_blacklist"],
        "max_file_size": None, **dict.fromkeys(_SIZES),
    }


def _build_extractor(settings, kw, v1):
    """`Extractor(...)` exactly as extract() builds it when given a config or settings file."""
    if v1:
        config = settings.use_config(kw["settingsfile"], kw["config"])
        version_args = {"fast": kw["no_fallback"], "max_tree_size": kw["max_tree_size"]}
    else:
        config = settings.use_config(kw["settingsfile"]) if kw["settingsfile"] else kw["config"]
        version_args = {"fast": kw["fast"] or kw["no_fallback"]}
    return settings.Extractor(
        config=config, output_format=kw["output_format"], precision=kw["favor_precision"],
        recall=kw["favor_recall"], comments=kw["include_comments"], formatting=kw["include_formatting"],
        links=kw["include_links"], images=kw["include_images"], tables=kw["include_tables"], dedup=kw["deduplicate"],
        lang=kw["target_language"], url=kw["url"], with_metadata=kw["with_metadata"],
        only_with_metadata=kw["only_with_metadata"], tei_validation=kw["tei_validation"],
        author_blacklist=kw["author_blacklist"], url_blacklist=kw["url_blacklist"],
        date_params=kw["date_extraction_params"], **version_args,
    )


def _extractor(kw, v1):
    """The Extractor extract() would use when it isn't derived from keywords alone, else None (or _UNSUPPORTED)."""
    if kw["options"] is None and kw["config"] is None and kw["settingsfile"] is None:
        return None
    settings = _settings()
    if settings is None:
        return _UNSUPPORTED
    if isinstance(kw["options"], settings.Extractor):
        return kw["options"]
    if kw["settingsfile"] is None and (kw["config"] is None or kw["config"] is settings.DEFAULT_CONFIG):
        return None
    try:
        return _build_extractor(settings, kw, v1)
    except Exception:  # noqa: BLE001 - Python raises it again
        return _UNSUPPORTED


def _strings(value):
    """A blacklist as a list of str, or None when its membership test isn't plain str equality."""
    if not value:
        return []
    if isinstance(value, (set, frozenset, list, tuple)) and all(type(v) is str for v in value):
        return list(value)
    return None


def _dates(params):
    """(max_date, flags differing from the port's defaults) for htmldate's find_date(**params), or None."""
    if not params:  # set_date_params(): today's local date
        return datetime.now().strftime("%Y-%m-%d"), {}
    if not isinstance(params, dict) or set(params) - _DATE_KEYS:
        return None
    if params.get("outputformat", "%Y-%m-%d") != "%Y-%m-%d":
        return None
    # find_date's defaults for missing keys; "url" is overwritten with the document's
    original, extensive = params.get("original_date", False), params.get("extensive_search", True)
    max_date = params.get("max_date")
    if type(original) is not bool or type(extensive) is not bool:
        return None
    if not (isinstance(max_date, str) and _ISO_DAY.fullmatch(max_date)):
        return None
    flags = {k: v for k, v in (("original_date", original), ("extensive_search", extensive)) if not v}
    return max_date, flags


def _metadata(spec, rust, extra):
    """Date parameters and filters, which only apply with metadata. False when not covered."""
    dates = _dates(spec["date_params"])
    if dates is None:
        return False
    rust["max_date"], flags = dates
    extra.update(flags)
    if spec["only_with_metadata"]:
        extra["only_with_metadata"] = True
    urls, authors = _strings(spec["url_blacklist"]), _strings(spec["author_blacklist"])
    if urls is None or authors is None:
        return False
    if urls:
        extra["url_blacklist"] = urls
    if authors:
        extra["author_blacklist"] = [a.lower() for a in authors]
    return True


def _translate(spec):
    """`_core.extract` keywords for an Extractor's attributes, or None."""
    fmt, focus, dedup, url = spec["format"], spec["focus"], spec["dedup"], spec["url"]
    if fmt not in _FORMATS or focus not in _FOCUS or spec["links"] or spec["images"] or spec["lang"]:
        return None
    if fmt == "json" and spec["formatting"]:
        return None
    if not (dedup is None or isinstance(dedup, int)) or not (url is None or isinstance(url, str)):
        return None  # 2.x: deduplicate as a scoped LRUCache
    rust = {
        # txt and markdown differ only by `formatting` in both versions
        "output_format": fmt if fmt == "json" else "markdown" if spec["formatting"] else "txt",
        "favor_precision": focus == "precision", "favor_recall": focus == "recall",
        "include_tables": bool(spec["tables"]), "no_fallback": bool(spec["fast"]),
        "include_comments": bool(spec["comments"]), "deduplicate": bool(dedup),
        "with_metadata": bool(spec["with_metadata"]), "url": url,
    }
    extra = {}
    for key, default in _SIZES.items():
        value = spec[key]
        if value is not None and (type(value) is not int or value < 0):
            return None
        if value is not None and value != default:
            extra[key] = value
    if spec["max_tree_size"]:
        if type(spec["max_tree_size"]) is not int or spec["max_tree_size"] < 0:
            return None
        extra["max_tree_size"] = spec["max_tree_size"]
    if rust["with_metadata"] and not _metadata(spec, rust, extra):
        return None
    if extra:
        rust["extra"] = extra
    return rust


def _prune_xpath(value):
    if value is None or isinstance(value, str):
        return [value] if value else []
    if isinstance(value, (list, tuple)) and all(type(v) is str for v in value):
        return list(value)
    return None


def core_kwargs(kwargs, engine):
    """(`_core.extract` keywords, max_file_size for decoding bytes) for these extract() options under trafilatura
    `engine`, verified or not, or None when the port has no code path for them."""
    v1 = engine == "1.11.0"
    params = _PARAMS if v1 else _PARAMS_2
    if not kwargs.keys() <= params.keys():
        return None
    kw = {**params, **kwargs}
    if not v1 and kw["max_tree_size"]:  # 2.x raises
        return None
    ext = _extractor(kw, v1)
    if ext is _UNSUPPORTED:
        return None
    spec = _spec(kw, v1) if ext is None else {f: getattr(ext, f, None) for f in _EXTRACTOR_FIELDS}
    rust = _translate(spec)
    prune, record_id = _prune_xpath(kw["prune_xpath"]), kw["record_id"]
    if rust is None or prune is None or not (record_id is None or isinstance(record_id, str)):
        return None
    if prune:
        rust.setdefault("extra", {})["prune_xpath"] = prune
    rust.update(engine=engine, record_id=record_id)
    return rust, spec["max_file_size"]
