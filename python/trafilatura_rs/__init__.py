"""Byte-exact Rust port of trafilatura 1.11's text extraction.

Supported: ``extract()`` with txt output and ``include_comments=False``, for the option combinations listed in
``VERIFIED`` (each proven byte-identical to trafilatura 1.11 + lxml 6.1.3 on a 10,666-page Common Crawl corpus).
``install()`` routes matching ``trafilatura.extract`` calls (e.g. datatrove's Trafilatura step) to Rust and leaves
every other call on Python trafilatura.
"""
from ._core import Extractor, reset_caches
from ._core import extract as _extract

__all__ = ["Extractor", "extract", "install", "reset_caches", "VERIFIED"]

# (favor_precision, favor_recall, include_tables, no_fallback) combinations with full-corpus parity.
VERIFIED = {
    (True, False, True, False),   # favor_precision (datatrove/FineWeb, with deduplicate=True)
    (False, False, True, False),  # defaults
    (False, True, True, False),   # favor_recall
    (False, False, True, True),   # no_fallback
    (False, False, False, False),  # include_tables=False
}

_RUST_KWARGS = {"favor_precision", "favor_recall", "include_tables", "deduplicate", "no_fallback"}
_NEUTRAL = {"include_comments": False, "output_format": "txt", "url": None, "record_id": None,
            "target_language": None, "include_images": False, "include_formatting": False,
            "include_links": False, "with_metadata": False, "only_with_metadata": False, "max_tree_size": None,
            "url_blacklist": None, "author_blacklist": None, "settingsfile": None, "prune_xpath": None,
            "date_extraction_params": None, "tei_validation": False, "config": None, "options": None}


def _rust_args(filecontent, kwargs):
    """The Rust keyword arguments for a trafilatura.extract call, or None when it must stay on Python."""
    if not isinstance(filecontent, str):
        return None
    rust = {}
    for key, value in kwargs.items():
        if key in _RUST_KWARGS:
            rust[key] = bool(value)
        elif key not in _NEUTRAL or not (value is _NEUTRAL[key] or value == _NEUTRAL[key]):
            return None
    combo = (rust.get("favor_precision", False), rust.get("favor_recall", False),
             rust.get("include_tables", True), rust.get("no_fallback", False))
    return rust if combo in VERIFIED else None


def extract(filecontent, **kwargs):
    """``trafilatura.extract`` for verified configurations; raises NotImplementedError otherwise."""
    rust = _rust_args(filecontent, kwargs)
    if rust is None:
        raise NotImplementedError(f"trafilatura_rs does not cover this call yet (verified: {sorted(VERIFIED)})")
    return _extract(filecontent, **rust)


def install():
    """Patch trafilatura.extract so covered calls run in Rust; returns the original function."""
    import trafilatura
    import trafilatura.core

    original = trafilatura.core.extract

    def patched(filecontent, *args, **kwargs):
        rust = None if args else _rust_args(filecontent, kwargs)
        return original(filecontent, *args, **kwargs) if rust is None else _extract(filecontent, **rust)

    trafilatura.extract = trafilatura.core.extract = patched
    return original
