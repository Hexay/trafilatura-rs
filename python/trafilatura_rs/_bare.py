"""Drop-in for trafilatura's `bare_extraction`. Covered calls run in Rust; the Document (1.11: its dict by default) is
built from the Rust slots, and `body`/`commentsbody` are lxml trees rebuilt element by element (tag, attributes,
text, tail) from the Rust trees, then checked: `etree.tostring` of each must equal the Rust tree's. A tree failing
that check reruns the call in Python, as does anything else the port doesn't cover."""
import importlib

from . import _core, _versions
from ._api import _BALANCED, _FAST, _PRECISION, _RECALL, _with_comments, bind, decoded
from ._funcs import call, document, metadata_ready
from ._install import original
from ._options import core_kwargs

_FOCUS = (_PRECISION, _BALANCED, _RECALL, _FAST)
# (favor_precision, favor_recall, include_tables, fast, include_comments) with full-corpus parity of every slot
# (trees by etree.tostring), per (output_format, with_metadata), for trafilatura 1.11.0 and 2.3.1 alike.
VERIFIED_BARE = {
    ("python", False): _with_comments(*_FOCUS),
    ("python", True): frozenset((*options, True) for options in _FOCUS),
}


def _plan(filecontent, kwargs, engine):
    """(html str, `_core.bare_extraction` keywords, date_params) for `bare_extraction(filecontent, **kwargs)`, or
    None. Deduplication stays on Python: a document rerun there would update Python's cache instead of ours."""
    planned = core_kwargs(kwargs, engine, bare=True)
    if planned is None:
        return None
    rust, max_file_size = planned
    output = ("python" if rust["python_format"] else rust["output_format"], rust["with_metadata"])
    combo = (rust["favor_precision"], rust["favor_recall"], rust["include_tables"], rust["no_fallback"],
             rust["include_comments"])
    if combo not in VERIFIED_BARE.get(output, ()) or rust.pop("deduplicate") or (output[1] and not metadata_ready()):
        return None
    html = decoded(filecontent, engine, max_file_size)
    return None if html is None else (html, rust, rust.pop("date_params"))


def supports_bare_extraction(**kwargs):
    """True when `bare_extraction(html, **kwargs)` on a str runs in Rust (given a REPRODUCES pair installed); with
    metadata it also needs trafilatura installed and a rare document can still need Python."""
    return not _versions.mismatched_versions() and _plan("", kwargs, _versions.engine()) is not None


def _fill(element, attrs, text, tail, children):
    from lxml.etree import SubElement

    for name, value in attrs:
        element.set(name, value)
    if text is not None:
        element.text = text
    if tail is not None:
        element.tail = tail
    for tag, *rest in children:
        _fill(SubElement(element, tag), *rest)


def _tree(spec, xml):
    """The lxml tree `spec` describes, checked against the Rust tree's serialization `xml`."""
    from lxml.etree import Element, tostring

    tag, *rest = spec
    root = Element(tag)
    _fill(root, *rest)
    if tostring(root, encoding="unicode") != xml:
        raise ValueError("rebuilt tree differs from the Rust tree")
    return root


def _result(fields, engine, as_dict, date_params):
    if fields is None:
        return None
    for key in ("body", "commentsbody"):
        if fields[key] is not None:
            fields[key] = _tree(*fields[key])
    if engine == "1.11.0" and isinstance(date_params, dict) and date_params:  # 1.11 writes the date search's URL
        date_params["url"] = fields["date_url"]
    doc = document(fields)
    return doc.as_dict() if as_dict else doc


def run_rust(html, rust, engine, as_dict, date_params):
    """The covered call in Rust; raises where it needs Python."""
    return _result(_core.bare_extraction(html, **rust), engine, as_dict, rust["with_metadata"] and date_params)


def bare_extraction(filecontent, *args, **kwargs):
    """Drop-in for `trafilatura.bare_extraction` of the installed trafilatura: Rust for the calls in VERIFIED_BARE,
    Python trafilatura for the rest. Needs trafilatura installed (it builds trafilatura's Document)."""
    python = original("bare_extraction")
    kwargs = bind(python, filecontent, args, kwargs)
    engine = _versions.engine()
    planned = None
    if python is not None and not _versions.mismatched_versions(kwargs.get("with_metadata", False)):
        planned = _plan(filecontent, kwargs, engine)
    if planned is None:
        return call("bare_extraction", None, (filecontent,), kwargs)
    html, rust, date_params = planned
    try:
        result = run_rust(html, rust, engine, kwargs.get("as_dict", engine == "1.11.0"), date_params)
    except Exception:  # noqa: BLE001 - Python reruns the call and raises what it raises
        return python(filecontent, **kwargs)
    if engine != "1.11.0":  # 2.x's DeprecationWarnings, attributed to our caller as Python's are
        importlib.import_module("trafilatura.core")._check_deprecation(
            kwargs.get("fast", False), no_fallback=kwargs.get("no_fallback", False),
            as_dict=kwargs.get("as_dict", False), stacklevel=3)
    return result
