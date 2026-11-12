"""Drop-ins for trafilatura's `html2txt`, `baseline` and `extract_metadata` and htmldate's `find_date`, with the
`extract` discipline: str/bytes input on a REPRODUCES pair runs in Rust; anything else, and any document the port
declines, runs the installed Python function, so results never change."""
import importlib
import sys

from . import _core, _versions
from ._api import bind, decoded
from ._install import original
from ._options import _dates, _strings

_FIND_DATE_DEFAULTS = {"extensive_search": True, "original_date": False, "outputformat": "%Y-%m-%d", "url": None,
                       "verbose": False, "min_date": None, "max_date": None, "deferred_url_extractor": False}


def call(name, run, args, kwargs=None):
    """`run()` when given and it succeeds, else the original Python `name(*args, **kwargs)`."""
    python = original(name)
    if run is not None:
        try:
            return run()
        except Exception:  # noqa: BLE001 - Python reruns the call and raises what it raises
            if python is None:
                raise
    if python is None:
        raise NotImplementedError(f"trafilatura_rs covers only str/bytes input in {name}; install the package it "
                                  "comes from to fall back to it")
    return python(*args, **(kwargs or {}))


def _html(content):
    """(engine, html str) when trafilatura `engine`'s Rust port may run on `content`, else None."""
    if _versions.mismatched_versions():
        return None
    engine = _versions.engine()
    html = decoded(content, engine)
    return None if html is None else (engine, html)


def supports_html2txt(clean=True):
    """True when `html2txt(html, clean)` on a str runs in Rust (bytes also need trafilatura installed)."""
    return not _versions.mismatched_versions()


def html2txt(content, clean=True):
    """Drop-in for `trafilatura.html2txt`."""
    planned = _html(content)
    run = None if planned is None else lambda: _core.html2txt(planned[1], planned[0], bool(clean))
    return call("html2txt", run, (content, clean))


def supports_baseline():
    """True when `baseline(html)` on a str runs in Rust (bytes also need trafilatura installed)."""
    return not _versions.mismatched_versions()


def _body(paragraphs, text):
    from lxml.etree import Element, SubElement

    body = Element("body")
    for paragraph in paragraphs:
        SubElement(body, "p").text = paragraph
    return body, text, len(text)


def baseline(filecontent):
    """Drop-in for `trafilatura.baseline`: (lxml `<body>` of `<p>` elements, text, len(text))."""
    planned = _html(filecontent)
    run = None if planned is None else lambda: _body(*_core.baseline(planned[1], planned[0]))
    return call("baseline", run, (filecontent,))


def metadata_ready():
    """Whether metadata may run in Rust here: trafilatura installed (Python callbacks), Python 3.11+ (fromisoformat),
    REPRODUCES_METADATA installed."""
    return (sys.version_info >= (3, 11) and original("extract_metadata") is not None
            and not _versions.mismatched_versions(with_metadata=True))


def _metadata_kwargs(default_url, date_config, extensive, author_blacklist):
    """`_core.extract_metadata` keywords for these arguments, or None."""
    if not (default_url is None or isinstance(default_url, str)) or type(extensive) is not bool:
        return None
    dates = _dates(date_config or None)  # falsy: set_date_params(extensive)
    authors = _strings(author_blacklist)
    if dates is None or authors is None:
        return None
    max_date, flags = dates
    if not date_config:
        flags = {} if extensive else {"extensive_search": False}
    return {"max_date": max_date, "default_url": default_url, "author_blacklist": [a.lower() for a in authors],
            **flags}


def supports_extract_metadata(default_url=None, date_config=None, extensive=True, author_blacklist=None):
    """True when `extract_metadata(html, ...)` on a str runs in Rust (a rare document can still need Python)."""
    return metadata_ready() and _metadata_kwargs(default_url, date_config, extensive, author_blacklist) is not None


def document(fields):
    """The installed trafilatura's Document with these slots set; None leaves the constructor's default (2.x starts
    `body`/`commentsbody` as empty elements, which trafilatura only overwrites with trees)."""
    doc = importlib.import_module("trafilatura.settings").Document()
    for slot in doc.__slots__:
        if fields.get(slot) is not None:
            setattr(doc, slot, fields[slot])
    return doc


def metadata_document(fields, engine, date_config):
    """extract_metadata's return value from `_core.extract_metadata`'s slots."""
    if fields is None:
        return None if engine == "1.11.0" else document({})
    if engine == "1.11.0" and date_config:  # 1.11 writes the date search's URL into the caller's dict
        date_config["url"] = fields["date_url"]
    return document(fields)


def extract_metadata(filecontent, default_url=None, date_config=None, extensive=True, author_blacklist=None):
    """Drop-in for `trafilatura.extract_metadata` of the installed trafilatura."""
    args = (filecontent, default_url, date_config, extensive, author_blacklist)
    planned = _html(filecontent)
    kwargs = _metadata_kwargs(*args[1:])
    if planned is None or kwargs is None or not metadata_ready():
        return call("extract_metadata", None, args)
    engine, html = planned
    return call("extract_metadata",
                lambda: metadata_document(_core.extract_metadata(html, engine, **kwargs), engine, date_config), args)


def _find_date_kwargs(htmlobject, kwargs):
    """(html str, `_core.find_date` keywords) for `find_date(htmlobject, **kwargs)`, or None."""
    if not kwargs.keys() <= _FIND_DATE_DEFAULTS.keys() or _versions.htmldate_mismatch() or sys.version_info < (3, 11):
        return None
    kw = {**_FIND_DATE_DEFAULTS, **kwargs}
    if kw["outputformat"] != "%Y-%m-%d" or kw["verbose"] or kw["min_date"] is not None or kw["deferred_url_extractor"]:
        return None
    if type(kw["extensive_search"]) is not bool or type(kw["original_date"]) is not bool:
        return None
    if not all(v is None or isinstance(v, str) for v in (kw["url"], kw["max_date"])):
        return None
    if isinstance(htmlobject, bytes):
        htmlobject = importlib.import_module("htmldate.utils").decode_file(htmlobject)
    elif not isinstance(htmlobject, str) or (htmlobject.startswith("http") and " " not in htmlobject):
        return None  # lxml trees, and URLs htmldate downloads
    return htmlobject, {k: kw[k] for k in ("extensive_search", "original_date", "url", "max_date")}


def supports_find_date(**kwargs):
    """True when htmldate's `find_date(html, **kwargs)` on a str (not a URL) runs in Rust."""
    return _find_date_kwargs("", kwargs) is not None


def find_date(htmlobject, *args, **kwargs):
    """Drop-in for `htmldate.find_date` (htmldate 1.11.0)."""
    kwargs = bind(original("find_date"), htmlobject, args, kwargs)
    try:
        planned = _find_date_kwargs(htmlobject, kwargs)
    except Exception:  # noqa: BLE001 - htmldate's decode_file raised; Python raises it again
        planned = None
    run = None if planned is None else lambda: _core.find_date(planned[0], **planned[1])
    return call("find_date", run, (htmlobject,), kwargs)
