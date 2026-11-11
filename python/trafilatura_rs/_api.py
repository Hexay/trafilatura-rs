import inspect

from . import _versions
from ._core import extract as _rust_extract

# (favor_precision, favor_recall, include_tables, no_fallback) combinations with full-corpus parity.
VERIFIED = {
    (True, False, True, False),   # favor_precision (datatrove/FineWeb, with deduplicate=True)
    (False, False, True, False),  # defaults
    (False, True, True, False),   # favor_recall
    (False, False, True, True),   # no_fallback
    (False, False, False, False),  # include_tables=False
}

_RUST_KWARGS = {"favor_precision", "favor_recall", "include_tables", "deduplicate", "no_fallback"}
# Arguments that may be passed as long as they hold these values. include_comments defaults to True in trafilatura,
# so it must be passed explicitly as False.
_NEUTRAL = {"include_comments": False, "output_format": "txt", "url": None, "record_id": None,
            "target_language": None, "include_images": False, "include_formatting": False,
            "include_links": False, "with_metadata": False, "only_with_metadata": False, "max_tree_size": None,
            "url_blacklist": None, "author_blacklist": None, "settingsfile": None, "prune_xpath": None,
            "date_extraction_params": None, "tei_validation": False, "config": None, "options": None}


def _rust_kwargs(kwargs):
    """The Rust keyword arguments for these extract() options, or None when the call must run in Python."""
    if kwargs.get("include_comments", True) is not False:
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


def supports(**kwargs):
    """True when `extract(html, **kwargs)` on a str runs in Rust (given matching installed versions, if any)."""
    return _rust_kwargs(kwargs) is not None


def _python_extract():
    from ._install import original_extract

    return original_extract()


def extract(filecontent, *args, **kwargs):
    """Drop-in for `trafilatura.extract`: Rust for covered calls, Python trafilatura for the rest.

    Without trafilatura installed, only covered calls work (others raise NotImplementedError). With trafilatura
    installed at versions other than `REPRODUCES`, every call runs Python trafilatura, so results never change.
    """
    python = _python_extract()
    if args:
        if python is None:
            raise TypeError("positional options need trafilatura installed; pass them as keywords")
        bound = inspect.signature(python).bind(filecontent, *args, **kwargs).arguments
        extra = bound.pop("kwargs", {})
        bound.pop("filecontent")
        kwargs = {**bound, **extra}
    rust = _rust_kwargs(kwargs) if isinstance(filecontent, str) else None
    if rust is not None and (python is None or not _versions.mismatched_versions()):
        return _rust_extract(filecontent, **rust)
    if python is None:
        raise NotImplementedError(f"trafilatura_rs covers only {sorted(VERIFIED)} with include_comments=False and "
                                  "txt output; install trafilatura to fall back to it for other calls")
    return python(filecontent, **kwargs)
