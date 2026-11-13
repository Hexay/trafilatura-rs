"""The drop-ins besides extract(): parity with the installed trafilatura/htmldate on the fixtures, fallbacks, install()."""
import warnings

import htmldate
import htmldate.core
import pytest
import trafilatura
from lxml import etree
from parity import FAST, TRAFILATURA, pages

import trafilatura_rs
from trafilatura_rs import _core

V1 = TRAFILATURA.startswith("1.")
PAGES = pages()
HTML = PAGES[0]["html"]
_REF = {"bare_extraction": trafilatura.bare_extraction, "baseline": trafilatura.baseline,
        "html2txt": trafilatura.html2txt, "extract_metadata": trafilatura.extract_metadata,
        "find_date": htmldate.find_date}
_FOCUS = [{"favor_precision": True}, {}, {"favor_recall": True}, {FAST: True}]


def canon(x):
    if isinstance(x, etree._Element):
        return "tree", etree.tostring(x, encoding="unicode")
    if hasattr(x, "as_dict") and hasattr(x, "__slots__"):
        return "Document", canon(x.as_dict())
    if isinstance(x, dict):
        return {k: canon(v) for k, v in x.items()}
    if isinstance(x, tuple):
        return tuple(canon(v) for v in x)
    return type(x).__name__, x


def outcome(fn, *args, **kwargs):
    try:
        return canon(fn(*args, **kwargs))
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        return f"EXC:{type(e).__name__}"


def same(name, kwargs, monkeypatch, docs=PAGES):
    """Equal results on every page, with Python rerunning few of them (pages the port hands back)."""
    from trafilatura_rs import _bare, _funcs, _install

    expected = [outcome(_REF[name], p["html"], **kwargs(p)) for p in docs]
    fallbacks = []

    def original(n):
        return (lambda *a, **k: fallbacks.append(a[0]) or _REF[name](*a, **k)) if n == name else _install.original(n)

    for module in (_bare, _funcs):
        monkeypatch.setattr(module, "original", original)
    got = [outcome(getattr(trafilatura_rs, name), p["html"], **kwargs(p)) for p in docs]
    assert [p["i"] for p, e, g in zip(docs, expected, got) if e != g] == []
    assert len(fallbacks) <= len(docs) // 4


@pytest.mark.parametrize("clean", [True, False])
def test_html2txt(monkeypatch, clean):
    assert trafilatura_rs.supports_html2txt(clean)
    same("html2txt", lambda p: {"clean": clean}, monkeypatch)


def test_baseline(monkeypatch):
    same("baseline", lambda p: {}, monkeypatch)


@pytest.mark.parametrize("url", [False, True])
def test_extract_metadata(monkeypatch, url):
    assert trafilatura_rs.supports_extract_metadata()
    same("extract_metadata", lambda p: {"default_url": p["url"] if url else None}, monkeypatch)


@pytest.mark.parametrize("kwargs", [{}, {"extensive_search": False, "original_date": True}])
def test_find_date(monkeypatch, kwargs):
    assert trafilatura_rs.supports_find_date(**kwargs)
    same("find_date", lambda p: {**kwargs, "url": p["url"]}, monkeypatch)
    same("find_date", lambda p: kwargs, monkeypatch)


@pytest.mark.parametrize("focus", _FOCUS)
@pytest.mark.parametrize("extra", [{}, {"include_comments": False}, {"with_metadata": True}])
def test_bare_extraction(monkeypatch, focus, extra):
    kwargs = {**focus, **extra}
    assert trafilatura_rs.supports_bare_extraction(**kwargs)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        same("bare_extraction", lambda p: {**kwargs, "url": p["url"]}, monkeypatch)


def test_bare_extraction_date_params_written_like_python():
    params, ref = ({"max_date": "2026-01-01", "original_date": True} for _ in range(2))
    trafilatura_rs.bare_extraction(HTML, with_metadata=True, date_extraction_params=params)
    _REF["bare_extraction"](HTML, with_metadata=True, date_extraction_params=ref)
    assert params == ref


@pytest.mark.parametrize("kwargs", [{"deduplicate": True}, {"output_format": "xml"}, {"include_links": True},
                                    {"with_metadata": True, "include_comments": False}])
def test_bare_extraction_uncovered_calls_fall_back(kwargs):
    from trafilatura.meta import reset_caches

    assert not trafilatura_rs.supports_bare_extraction(**kwargs)
    reset_caches()
    got = canon(trafilatura_rs.bare_extraction(HTML, **kwargs))
    reset_caches()
    assert got == canon(_REF["bare_extraction"](HTML, **kwargs))
    reset_caches()


def test_unsupported_document_falls_back(monkeypatch):
    def unsupported(*args, **kwargs):
        raise _core.Unsupported("forced")

    for name in ("extract_metadata", "bare_extraction", "find_date"):
        monkeypatch.setattr(_core, name, unsupported)
    assert canon(trafilatura_rs.extract_metadata(HTML)) == canon(_REF["extract_metadata"](HTML))
    assert canon(trafilatura_rs.bare_extraction(HTML)) == canon(_REF["bare_extraction"](HTML))
    assert trafilatura_rs.find_date(HTML) == _REF["find_date"](HTML)


def test_trees_and_urls_run_python(monkeypatch):
    tree = trafilatura.load_html(HTML)
    for name in ("html2txt", "baseline", "extract_metadata", "bare_extraction", "find_date"):
        monkeypatch.setattr(_core, name, lambda *a, **k: pytest.fail("ran Rust"))
    assert trafilatura_rs.html2txt(tree) == _REF["html2txt"](tree)
    assert canon(trafilatura_rs.extract_metadata(tree)) == canon(_REF["extract_metadata"](tree))
    assert trafilatura_rs.find_date(tree) == _REF["find_date"](tree)
    assert not trafilatura_rs.supports_find_date(outputformat="%d.%m.%Y")


def test_install_swaps_every_function():
    import trafilatura.baseline
    import trafilatura.metadata

    trafilatura_rs.install()
    try:
        assert trafilatura.bare_extraction is trafilatura_rs.bare_extraction
        assert trafilatura.core.bare_extraction is trafilatura_rs.bare_extraction  # extract()'s fallback uses it
        assert trafilatura.baseline is trafilatura_rs.baseline and trafilatura.html2txt is trafilatura_rs.html2txt
        assert trafilatura.metadata.find_date is trafilatura_rs.find_date
        assert htmldate.find_date is trafilatura_rs.find_date and htmldate.core.find_date is trafilatura_rs.find_date
        assert trafilatura.extract(HTML, output_format="xml") == trafilatura_rs._install.original("extract")(
            HTML, output_format="xml")
    finally:
        trafilatura_rs.uninstall()
    assert trafilatura.bare_extraction is _REF["bare_extraction"] and htmldate.find_date is _REF["find_date"]


def test_autoinstall_on_htmldate_import():
    from test_install import _python

    code = ("import sys, trafilatura_rs._autoinstall; import htmldate, trafilatura_rs; "
            "print(htmldate.find_date is trafilatura_rs.find_date, 'trafilatura' in sys.modules)")
    assert _python(code) == "True True"


def test_bare_extraction_deprecations_point_at_the_caller():
    if V1:
        pytest.skip("2.x deprecations")
    with pytest.warns(DeprecationWarning, match="no_fallback") as record:
        trafilatura_rs.bare_extraction(HTML, no_fallback=True)
    assert record[0].filename == __file__


def test_bare_extraction_return_type():
    result = trafilatura_rs.bare_extraction(HTML)
    assert isinstance(result, dict) if V1 else isinstance(result, trafilatura.settings.Document)
