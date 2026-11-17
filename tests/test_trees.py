"""extract()/bare_extraction() given lxml trees: parity with the installed trafilatura on the fixtures, the caller's
tree left as Python leaves it, and Rust only where Python works on a copy (2.3.1; 1.11.0 cleans the caller's tree)."""
import warnings

import lxml.html
import pytest
import trafilatura
from lxml import etree
from parity import TRAFILATURA, pages
from test_funcs import canon

import trafilatura_rs
from trafilatura_rs import _api, _core

V1 = TRAFILATURA.startswith("1.")
SOURCES = {"fromstring": lxml.html.fromstring, "document_fromstring": lxml.html.document_fromstring,
           "load_html": trafilatura.load_html}
CALLS = {
    "default": ("extract", {}),
    "fineweb": ("extract", {"favor_precision": True, "include_comments": False}),
    "json_meta": ("extract", {"output_format": "json", "with_metadata": True}),
    "bare": ("bare_extraction", {}),
    "bare_meta": ("bare_extraction", {"with_metadata": True}),
}
# comments next to the content: lxml's comment .text, and readability's ValueError on a comment sibling
COMMENTED = [
    "<html><body><article><p>" + "Some long enough paragraph text here. " * 8 + "</p><div>x<!--c-->y</div>"
    "</article></body></html>",
    '<table id="t"><img src="a.jpg"></table>\n<!-- menu -->\n<td><b>&nbsp;' + "Long enough cell text. " * 8,
]


def outcome(fn, tree, kwargs):
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", DeprecationWarning)
            return canon(fn(tree, **kwargs))
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        return f"EXC:{type(e).__name__}"


def check(monkeypatch, make, name, kwargs, docs):
    """Same results and caller trees as Python on every doc; returns how many calls ran Rust."""
    rust = []
    for module, attr in ((_api, "_rust_extract"), (_core, "bare_extraction")):
        fn = getattr(module, attr)
        monkeypatch.setattr(module, attr, lambda *a, fn=fn, **k: rust.append(1) or fn(*a, **k))
    for html in docs:
        try:
            ours, ref = make(html), make(html)
        except (ValueError, etree.ParserError):  # str with an encoding declaration, or empty: no tree to pass
            continue
        if ours is None:
            continue
        assert outcome(getattr(trafilatura_rs, name), ours, kwargs) == outcome(getattr(trafilatura, name), ref, kwargs)
        assert etree.tostring(ours) == etree.tostring(ref)
    return len(rust)


@pytest.mark.parametrize("source", SOURCES)
@pytest.mark.parametrize("call", CALLS)
def test_tree_parity(monkeypatch, source, call):
    name, kwargs = CALLS[call]
    ran = check(monkeypatch, SOURCES[source], name, kwargs, [p["html"] for p in pages()])
    assert ran == 0 if V1 else ran > 0


@pytest.mark.parametrize("call", ["default", "fineweb", "bare"])
def test_comments_in_trees(monkeypatch, call):
    check(monkeypatch, lxml.html.fromstring, *CALLS[call], COMMENTED)


@pytest.mark.parametrize("make", [lambda h: etree.fromstring(h, etree.HTMLParser()),
                                  lambda h: lxml.html.fromstring(h).getroottree()])
def test_other_trees_run_python(monkeypatch, make):
    html = pages()[0]["html"]
    assert check(monkeypatch, make, "extract", {}, [html]) == 0
    assert trafilatura_rs.extract(make(html)) is None
