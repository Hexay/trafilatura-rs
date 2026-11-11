import inspect
import re
import sys
from datetime import datetime

from . import _versions
from ._core import Unsupported
from ._core import extract as _rust_extract

# (favor_precision, favor_recall, include_tables, no_fallback, include_comments) combinations with full-corpus parity
# for plain-text output without metadata: each option set below, with and without comments.
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

# The combinations verified per (output_format, with_metadata).
VERIFIED_OUTPUTS = {
    ("txt", False): VERIFIED,
    ("markdown", False): {(True, False, True, False, False), (False, False, True, False, False)},
    ("txt", True): {(False, False, True, False, False)},
    ("json", True): {(True, False, True, False, False), (False, False, True, False, False)},
    ("json", False): {(False, False, True, False, False)},
}

_BOOL_KWARGS = {"favor_precision", "favor_recall", "include_tables", "deduplicate", "no_fallback", "include_comments",
                "with_metadata"}
_FORMATS = {"txt", "markdown", "json"}
# Arguments that may be passed as long as they hold these values.
_NEUTRAL = {"target_language": None, "include_images": False, "include_formatting": False, "include_links": False,
            "only_with_metadata": False, "max_tree_size": None, "url_blacklist": None, "author_blacklist": None,
            "settingsfile": None, "prune_xpath": None, "tei_validation": False, "config": None, "options": None}
_ISO_DAY = re.compile(r"\d{4}-\d{2}-\d{2}", re.ASCII)


def _max_date(params):
    """htmldate's max_date for these date_extraction_params, or None when the port does not cover them."""
    if not params:
        # set_date_params(): today's local date, as trafilatura computes it for every call
        return datetime.now().strftime("%Y-%m-%d")
    if not isinstance(params, dict) or set(params) - {"original_date", "extensive_search", "max_date", "url"}:
        return None
    if params.get("original_date") is not True or params.get("extensive_search") is not True:
        return None
    max_date = params.get("max_date")
    return max_date if isinstance(max_date, str) and _ISO_DAY.fullmatch(max_date) else None


def _rust_kwargs(kwargs):
    """The Rust keyword arguments for these extract() options, or None when the call must run in Python."""
    rust = {}
    for key, value in kwargs.items():
        if key in _BOOL_KWARGS:
            rust[key] = bool(value)
        elif key == "output_format":
            if value not in _FORMATS:
                return None
            rust[key] = value
        elif key in ("url", "record_id"):
            if value is not None and not isinstance(value, str):
                return None
            rust[key] = value
        elif key == "date_extraction_params":
            continue
        elif key not in _NEUTRAL or not (value is _NEUTRAL[key] or value == _NEUTRAL[key]):
            return None
    output = (rust.get("output_format", "txt"), rust.get("with_metadata", False))
    combo = (rust.get("favor_precision", False), rust.get("favor_recall", False),
             rust.get("include_tables", True), rust.get("no_fallback", False), rust.get("include_comments", True))
    if combo not in VERIFIED_OUTPUTS.get(output, ()):
        return None
    if output[1]:
        # fromisoformat (htmldate) has the full ISO grammar from 3.11; a document that falls back to Python would
        # update Python's dedup cache instead of ours
        if sys.version_info < (3, 11) or rust.get("deduplicate"):
            return None
        rust["max_date"] = _max_date(kwargs.get("date_extraction_params"))
        if rust["max_date"] is None:
            return None
    elif kwargs.get("date_extraction_params") is not None:
        return None
    return rust


def supports(**kwargs):
    """True when `extract(html, **kwargs)` on a str runs in Rust (given matching installed versions, if any).
    With metadata, a rare document can still need Python (see `extract`)."""
    return _rust_kwargs(kwargs) is not None


def _python_extract():
    from ._install import original_extract

    return original_extract()


def extract(filecontent, *args, **kwargs):
    """Drop-in for `trafilatura.extract`: Rust for covered calls, Python trafilatura for the rest.

    Without trafilatura installed, only covered calls work (others raise NotImplementedError). With trafilatura
    installed at versions other than `REPRODUCES` (and `REPRODUCES_METADATA` for metadata calls), those calls run
    Python trafilatura, so results never change. A covered call whose document needs a path the port leaves to
    Python (see docs/metadata-formats-scope.md) also runs Python trafilatura. Metadata extraction calls back into
    Python for courlan, dateutil and dateparser, so it needs trafilatura installed.
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
    if rust is not None and rust.get("with_metadata") and python is None:
        rust = None
    if rust is not None and (python is None or not _versions.mismatched_versions(rust.get("with_metadata", False))):
        try:
            return _rust_extract(filecontent, **rust)
        except Unsupported:
            if python is None:
                raise
    if python is None:
        raise NotImplementedError("trafilatura_rs covers only the VERIFIED_OUTPUTS configurations; install "
                                  "trafilatura to fall back to it for other calls")
    return python(filecontent, **kwargs)
