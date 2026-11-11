import inspect
import re
import sys
import warnings
from datetime import datetime

from . import _versions
from ._core import Extractor as _RustExtractor
from ._core import Unsupported
from ._core import extract as _rust_extract

# (favor_precision, favor_recall, include_tables, fast, include_comments) combinations with full-corpus parity for
# plain-text output without metadata, for every REPRODUCES pair: each option set below, with and without comments.
# fast is no_fallback in trafilatura 1.11.
VERIFIED = {
    (*options, include_comments)
    for options in (
        (True, False, True, False),   # favor_precision (datatrove/FineWeb, also with deduplicate=True)
        (False, False, True, False),  # defaults
        (False, True, True, False),   # favor_recall
        (False, False, True, True),   # fast / no_fallback
        (False, False, False, False),  # include_tables=False
    )
    for include_comments in (True, False)
}

# The combinations verified per (output_format, with_metadata) for trafilatura 1.11.
VERIFIED_OUTPUTS = {
    ("txt", False): VERIFIED,
    ("markdown", False): {(True, False, True, False, False), (False, False, True, False, False)},
    ("txt", True): {(False, False, True, False, False)},
    ("json", True): {(True, False, True, False, False), (False, False, True, False, False)},
    ("json", False): {(False, False, True, False, False)},
}
# Metadata, markdown and json are ported for 1.11 only.
_VERIFIED_BY_ENGINE = {"1.11.0": VERIFIED_OUTPUTS, "2.3.1": {("txt", False): VERIFIED}}

_BOOL_KWARGS = {"favor_precision", "favor_recall", "include_tables", "deduplicate", "include_comments",
                "with_metadata"}
_FORMATS = {"txt", "markdown", "json"}
# Arguments that may be passed as long as they hold one of these values.
_NEUTRAL = {"target_language": (None,), "include_images": (False,), "include_formatting": (False, None),
            "include_links": (False,), "only_with_metadata": (False,), "max_tree_size": (None,),
            "url_blacklist": (None,), "author_blacklist": (None,), "settingsfile": (None,), "prune_xpath": (None,),
            "tei_validation": (False,), "config": (None,), "options": (None,)}
_ISO_DAY = re.compile(r"\d{4}-\d{2}-\d{2}", re.ASCII)


def verified_outputs(engine=None):
    """VERIFIED_OUTPUTS for trafilatura `engine` (default: the one `extract` reproduces here, see `engine()`)."""
    return _VERIFIED_BY_ENGINE[engine or _versions.engine()]


def _neutral(key, value):
    return any(value is v or value == v for v in _NEUTRAL.get(key, ()))


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


def _rust_kwargs(kwargs, engine):
    """The `_core.extract` keyword arguments for these extract() options under trafilatura `engine`, or None when the
    call must run in Python."""
    if engine == "1.11.0" and "fast" in kwargs:  # not a 1.11 parameter (its **kwargs ignores it)
        return None
    rust = {"engine": engine, "no_fallback": False}
    for key, value in kwargs.items():
        if key == "deduplicate" and not (value is None or isinstance(value, int)):  # 2.x: a scoped LRUCache
            return None
        if key in ("fast", "no_fallback"):
            rust["no_fallback"] = rust["no_fallback"] or bool(value)
        elif key in _BOOL_KWARGS:
            rust[key] = bool(value)
        elif key == "output_format":
            if value not in _FORMATS:
                return None
            rust[key] = value
        elif key in ("url", "record_id"):
            # only 1.11 is verified with a url (and record_id only matters with metadata)
            if value is not None and (engine != "1.11.0" or not isinstance(value, str)):
                return None
            rust[key] = value
        elif key == "date_extraction_params":
            continue
        elif not _neutral(key, value):
            return None
    output = (rust.get("output_format", "txt"), rust.get("with_metadata", False))
    combo = (rust.get("favor_precision", False), rust.get("favor_recall", False),
             rust.get("include_tables", True), rust["no_fallback"], rust.get("include_comments", True))
    if combo not in _VERIFIED_BY_ENGINE[engine].get(output, ()):
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
    """True when `extract(html, **kwargs)` on a str runs in Rust (given a REPRODUCES pair installed, if any).
    With metadata, a rare document can still need Python (see `extract`)."""
    return _rust_kwargs(kwargs, _versions.engine()) is not None


def _python_extract():
    from ._install import original_extract

    return original_extract()


def extract(filecontent, *args, **kwargs):
    """Drop-in for `trafilatura.extract` of the installed trafilatura (2.3.1 when none is installed): Rust for covered
    calls, Python trafilatura for the rest.

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
    engine = _versions.engine()
    rust = _rust_kwargs(kwargs, engine) if isinstance(filecontent, str) else None
    if rust is not None and rust.get("with_metadata") and python is None:
        rust = None
    if rust is not None and (python is None or not _versions.mismatched_versions(rust.get("with_metadata", False))):
        if engine != "1.11.0" and kwargs.get("no_fallback"):
            warnings.warn('"no_fallback" will be removed, use "fast" instead', DeprecationWarning, stacklevel=2)
        try:
            return _rust_extract(filecontent, **rust)
        except Unsupported:
            if python is None:
                raise
    if python is None:
        if kwargs.get("max_tree_size"):
            raise ValueError('"max_tree_size" will be removed, use settings.cfg instead')
        raise NotImplementedError("trafilatura_rs covers only verified_outputs() configurations; install "
                                  "trafilatura to fall back to it for other calls")
    return python(filecontent, **kwargs)


class Extractor(_RustExtractor):
    """Rust-only extractor with its own dedup state, reproducing the installed trafilatura (see `engine`)."""

    def __new__(cls, favor_precision=False, favor_recall=False, include_tables=True, deduplicate=False,
                no_fallback=False, fast=False, include_comments=True, output_format="txt", with_metadata=False):
        return super().__new__(cls, _versions.engine(), favor_precision, favor_recall, include_tables, deduplicate,
                               no_fallback or fast, include_comments, output_format, with_metadata)
