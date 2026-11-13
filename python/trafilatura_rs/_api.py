import importlib
import inspect
import sys
import warnings

from . import _versions
from ._core import Extractor as _RustExtractor
from ._core import Unsupported
from ._core import extract as _rust_extract
from ._options import core_kwargs

# (favor_precision, favor_recall, include_tables, fast) option sets with full-corpus parity, for every REPRODUCES pair.
# fast is no_fallback in trafilatura 1.11.
_PRECISION, _BALANCED, _RECALL, _FAST = (True, False, True, False), (False, False, True, False), \
    (False, True, True, False), (False, False, True, True)
_NO_TABLES = (False, False, False, False)


def _with_comments(*option_sets):
    """Each option set with and without include_comments (trafilatura's default: True)."""
    return frozenset((*options, comments) for options in option_sets for comments in (True, False))


# (favor_precision, favor_recall, include_tables, fast, include_comments) for plain text without metadata.
VERIFIED = _with_comments(_PRECISION, _BALANCED, _RECALL, _FAST, _NO_TABLES)
_FOCUS_GRID = _with_comments(_PRECISION, _BALANCED, _RECALL, _FAST)

# The combinations verified per (output_format, with_metadata), for trafilatura 1.11.0 and 2.3.1 alike. txt with
# include_formatting=True is markdown, and markdown with formatting off is txt (both versions only test formatting).
VERIFIED_OUTPUTS = {
    ("txt", False): VERIFIED,
    ("markdown", False): _FOCUS_GRID,
    ("txt", True): _FOCUS_GRID,
    ("json", True): _FOCUS_GRID,
    ("json", False): _FOCUS_GRID,
    ("markdown", True): _with_comments(_BALANCED),
}
_VERIFIED_BY_ENGINE = {"1.11.0": VERIFIED_OUTPUTS, "2.3.1": VERIFIED_OUTPUTS}


def verified_outputs(engine=None):
    """VERIFIED_OUTPUTS for trafilatura `engine` (default: the one `extract` reproduces here, see `engine()`)."""
    return _VERIFIED_BY_ENGINE[engine or _versions.engine()]


def _rust_kwargs(kwargs, engine, verified=True):
    """(`_core.extract` keywords, max_file_size) for these extract() options under trafilatura `engine`, or None when
    the call must run in Python (or, with `verified`, isn't in VERIFIED_OUTPUTS)."""
    planned = core_kwargs(kwargs, engine)
    if planned is None or not verified:
        return planned
    rust = planned[0]
    output = (rust["output_format"], rust["with_metadata"])
    combo = (rust["favor_precision"], rust["favor_recall"], rust["include_tables"], rust["no_fallback"],
             rust["include_comments"])
    if combo not in _VERIFIED_BY_ENGINE[engine].get(output, ()):
        return None
    # fromisoformat (htmldate) has the full ISO grammar from 3.11; a document that falls back to Python would update
    # Python's dedup cache instead of ours
    if output[1] and (sys.version_info < (3, 11) or rust["deduplicate"]):
        return None
    return planned


def _decode_file():
    try:
        return importlib.import_module("trafilatura.utils").decode_file
    except ImportError:
        return None


def decoded(filecontent, engine, max_file_size=None):
    """`filecontent` as the str trafilatura's `load_html` parses, or None when it isn't str or bytes (lxml trees,
    responses) or decoding raises. bytes are decoded by the installed trafilatura's own `decode_file`."""
    if isinstance(filecontent, str):
        return filecontent
    decode = _decode_file() if isinstance(filecontent, bytes) else None
    if decode is None:
        return None
    try:
        return decode(filecontent) if engine == "1.11.0" else decode(filecontent, max_file_size)
    except Exception:  # noqa: BLE001 - Python raises it again
        return None


def _plan(filecontent, kwargs, engine, verified=True):
    """(html str, `_core.extract` keywords) for `extract(filecontent, **kwargs)`, or None."""
    planned = _rust_kwargs(kwargs, engine, verified)
    if planned is None:
        return None
    html = decoded(filecontent, engine, planned[1])
    return None if html is None else (html, planned[0])


def supports(**kwargs):
    """True when `extract(html, **kwargs)` on a str runs in Rust (given a REPRODUCES pair installed, if any); bytes
    also need trafilatura installed. With metadata, a rare document can still need Python (see `extract`)."""
    return _rust_kwargs(kwargs, _versions.engine()) is not None


def _python_extract():
    from ._install import original_extract

    return original_extract()


def bind(python, filecontent, args, kwargs):
    """`kwargs` with the positional `args` of `python(filecontent, *args, **kwargs)` merged in by name."""
    if not args:
        return kwargs
    if python is None:
        raise TypeError("positional options need trafilatura installed; pass them as keywords")
    signature = inspect.signature(python)
    bound = signature.bind(filecontent, *args, **kwargs).arguments
    extra = bound.pop("kwargs", {})
    bound.pop(next(iter(signature.parameters)))
    return {**bound, **extra}


def extract(filecontent, *args, **kwargs):
    """Drop-in for `trafilatura.extract` of the installed trafilatura (2.3.1 when none is installed): Rust for covered
    calls, Python trafilatura for the rest.

    Without trafilatura installed, only covered calls on str input work (others raise NotImplementedError). With
    trafilatura installed at versions other than `REPRODUCES` (and `REPRODUCES_METADATA` for metadata calls), those
    calls run Python trafilatura, so results never change. A covered call whose document needs a path the port
    leaves to Python (see docs/metadata-formats-scope.md) also runs Python trafilatura. Metadata extraction calls
    back into Python for courlan, dateutil and dateparser, so it needs trafilatura installed.
    """
    python = _python_extract()
    kwargs = bind(python, filecontent, args, kwargs)
    engine = _versions.engine()
    plan = _plan(filecontent, kwargs, engine)
    if plan is not None and plan[1]["with_metadata"] and python is None:
        plan = None
    if plan is not None and (python is None or not _versions.mismatched_versions(plan[1]["with_metadata"])):
        if engine != "1.11.0" and kwargs.get("no_fallback"):
            warnings.warn('"no_fallback" will be removed, use "fast" instead', DeprecationWarning, stacklevel=2)
        try:
            return _rust_extract(plan[0], **plan[1])
        except (Unsupported, UnicodeEncodeError):  # lone surrogates: lxml's parser takes them, Rust strings can't
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
