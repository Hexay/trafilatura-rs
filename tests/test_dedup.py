"""deduplicate=True keeps one cache across Rust calls and the documents rerun in Python, in call order, so a sequence of
calls gives trafilatura's results whichever path each document takes."""
import pytest
import trafilatura
import trafilatura.meta
from lxml import etree
from parity import TRAFILATURA

import trafilatura_rs
from trafilatura_rs import _core

V1 = TRAFILATURA.startswith("1.")
SHARED = "<p>" + "A paragraph shared by every page here, long enough for the duplicate test to count it. " * 2 + "</p>"
FILLER = "".join(f"<p>Page-specific text number {i}, " + "with some more words to pass the size checks. " * 3 + "</p>"
                 for i in range(4))
PAGE = f"<html><head><title>Title</title></head><body><article>{SHARED}{FILLER}</article></body></html>"
# lone surrogate: Rust strings can't take it, so the document reruns in Python
SURROGATE = PAGE.replace("Page-specific", "Page-\ud800specific", 1)
# a tag name lxml's API refuses: bare_extraction's rebuilt tree fails after Rust ran, so the call reruns in Python
BAD_TAG = PAGE.replace("<article>", '<article><h2><br>Head<ahref="http://e.bloくg.org/x.jpg/"><br>line</h2>', 1)


def canon(x):
    if isinstance(x, etree._Element):
        return etree.tostring(x, encoding="unicode")
    if hasattr(x, "as_dict") and hasattr(x, "__slots__"):
        return canon(x.as_dict())
    if isinstance(x, dict):
        return {k: canon(v) for k, v in x.items() if k != "filedate"}
    return x


def results(fn, docs, kwargs):
    trafilatura.meta.reset_caches()
    trafilatura_rs.reset_caches()
    out = []
    for html in docs:
        try:
            out.append(canon(fn(html, **kwargs)))
        except Exception as e:  # noqa: BLE001
            out.append(f"EXC:{type(e).__name__}")
    return out


def assert_same(name, docs, kwargs):
    ours = results(getattr(trafilatura_rs, name), docs, kwargs)
    ref = results(getattr(trafilatura, name), docs, kwargs)
    assert ours == ref
    assert len(set(map(str, ref))) > 1, "dedup never kicked in"


KWARGS = [{"deduplicate": True}, {"deduplicate": True, "with_metadata": True, "url": "https://example.org/a"},
          {"deduplicate": True, "output_format": "json", "with_metadata": True, "favor_precision": True}]


@pytest.mark.parametrize("kwargs", KWARGS)
def test_extract_python_rerun_shares_the_cache(kwargs):
    assert trafilatura_rs.supports(**kwargs)
    assert_same("extract", [SURROGATE, SURROGATE, PAGE, PAGE, SURROGATE, PAGE], kwargs)


def test_extract_uncovered_call_shares_the_cache():
    uncovered = {"deduplicate": True, "target_language": "en", "output_format": "xml"}
    assert not trafilatura_rs.supports(**uncovered)
    calls = [{"deduplicate": True}, uncovered, {"deduplicate": True}, uncovered, {"deduplicate": True}]

    def run(extract):
        trafilatura.meta.reset_caches()
        trafilatura_rs.reset_caches()
        return [extract(PAGE, **kwargs) for kwargs in calls]

    assert run(trafilatura_rs.extract) == run(trafilatura.extract)


@pytest.mark.parametrize("kwargs", [{"deduplicate": True}, {"deduplicate": True, "with_metadata": True},
                                    {"deduplicate": True, "include_links": True, "url": "https://example.org/"}])
def test_bare_extraction_rerun_undoes_rust_updates(kwargs):
    from trafilatura_rs import _bare

    assert trafilatura_rs.supports_bare_extraction(**kwargs)
    html, rust, date_params = _bare._plan(BAD_TAG, kwargs, trafilatura_rs.engine())
    with pytest.raises(_core.Unsupported):  # the case under test: Rust ran, its result is discarded
        _bare.run_rust(html, rust, trafilatura_rs.engine(), V1, date_params)
    assert_same("bare_extraction", [BAD_TAG, BAD_TAG, PAGE, BAD_TAG, PAGE, PAGE], kwargs)


def test_undo_only_reaches_the_failed_call():
    trafilatura_rs.reset_caches()
    _core.extract(PAGE, trafilatura_rs.engine(), deduplicate=True)
    _core._dedup_commit()
    _core._dedup_undo()  # nothing pending: the first call's updates stay
    _core.extract(PAGE, trafilatura_rs.engine(), deduplicate=True)
    _core.extract(PAGE, trafilatura_rs.engine(), deduplicate=True)
    assert _core.extract(PAGE, trafilatura_rs.engine(), deduplicate=True) != _core.extract(PAGE, trafilatura_rs.engine())
