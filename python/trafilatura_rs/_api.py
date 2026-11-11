import inspect

from . import _versions
from ._core import extract as _rust_extract

# (favor_precision, favor_recall, include_tables, no_fallback, include_comments) combinations with full-corpus parity:
# each option set below, with and without comments.
VERIFIED = {
    (*options, include_comments)
    for options in (
        (True, False, True, False),   # favor_precision (datatrove/FineWeb, also with deduplicate=True)
        (False, False, True, False),  # defaults
        (False, True, True, False),   # favor_recall
        (False, False, True, True),   # no_fallback
        (False, False, False, False),  # include_tables=False
    )
    for include_comments in (True, False)
}

_RUST_KWARGS = {"favor_precision", "favor_recall", "include_tables", "deduplicate", "no_fallback", "include_comments"}
# Arguments that may be passed as long as they hold these values.
_NEUTRAL = {"output_format": "txt", "url": None, "record_id": None,
            "target_language": None, "include_images": False, "include_formatting": False,
            "include_links": False, "with_metadata": False, "only_with_metadata": False, "max_tree_size": None,
            "url_blacklist": None, "author_blacklist": None, "settingsfile": None, "prune_xpath": None,
            "date_extraction_params": None, "tei_validation": False, "config": None, "options": None}


def _rust_kwargs(kwargs):
    """The Rust keyword arguments for these extract() options, or None when the call must run in Python."""
    rust = {}
    for key, value in kwargs.items():
        if key in _RUST_KWARGS:
            rust[key] = bool(value)
        elif key not in _NEUTRAL or not (value is _NEUTRAL[key] or value == _NEUTRAL[key]):
            return None
    combo = (rust.get("favor_precision", False), rust.get("favor_recall", False),
             rust.get("include_tables", True), rust.get("no_fallback", False), rust.get("include_comments", True))
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
        raise NotImplementedError(f"trafilatura_rs covers only {sorted(VERIFIED)} with txt output; install "
                                  "trafilatura to fall back to it for other calls")
    return python(filecontent, **kwargs)
