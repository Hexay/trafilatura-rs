"""target_language runs in Rust (the meta-tag check, the jusText stoplist and py3langid's classify, natively while
`trafilatura_rs._langid.model()` vouches for the identifier) and matches trafilatura; patched detectors and
non-str languages are honored or left to Python."""
import py3langid
import pytest
import trafilatura.utils
from parity import FAST
from test_options import HTML, assert_rust_parity

import trafilatura_rs
from trafilatura_rs import _api, _core

GERMAN = ("<html><head>{meta}</head><body><article><h1>Ein Bericht</h1><p>"
          + "Die Stadt hat am Montag beschlossen, die neue Brücke über den Fluss früher als geplant zu öffnen. " * 6
          + "</p><p>Die Bürger freuen sich über die schnellere Verbindung zwischen den beiden Ufern.</p>"
          "</article></body></html>")
PAGES = [GERMAN.format(meta=m) for m in ("", '<meta http-equiv="content-language" content="de-DE">',
                                         '<meta property="og:locale" content="en_US">')] + HTML


@pytest.mark.parametrize("lang", ["en", "de", "fr"])
@pytest.mark.parametrize("kwargs", [{}, {"output_format": "json", "with_metadata": True}, {FAST: True},
                                    {"favor_precision": True, "include_comments": False}])
def test_target_language(lang, kwargs):
    assert_rust_parity(PAGES, target_language=lang, **kwargs)


@pytest.mark.parametrize("lang", ["en", "de"])
def test_no_python_fallback(lang):
    """The stock detector and stoplists never leave the core (Unsupported would rerun the call in Python)."""
    for html in PAGES:
        plan = _api._plan(html, {"target_language": lang}, trafilatura_rs.engine())
        _core.extract(plan[0], **plan[1])


def test_unverified_or_odd_languages():
    assert_rust_parity(PAGES[:3], target_language="")
    assert_rust_parity(PAGES[:3], rust=False, target_language=["de"])
    assert_rust_parity(PAGES[:3], rust=False, target_language="de", output_format="xml")


def test_patched_classifier_is_called(monkeypatch):
    monkeypatch.setattr(trafilatura.utils, "language_classifier", lambda text, comments: "xx")
    assert_rust_parity(PAGES[:3], target_language="xx")
    assert_rust_parity(PAGES[:3], target_language="de")


def test_detector_off_checks_meta_only(monkeypatch):
    monkeypatch.setattr(trafilatura.core, "LANGID_FLAG", False)
    monkeypatch.setattr(trafilatura.utils, "LANGID_FLAG", False)
    assert_rust_parity(PAGES[:3], target_language="de")
    assert_rust_parity(PAGES[:3], target_language="en")


@pytest.mark.parametrize("text", ["", "Hello world, this is English.", "DAS IST EIN DEUTSCHER SATZ.", "Ça va très bien",
                                  "日本語のテキストです", "12345 !!!", "a"])
def test_native_classify(text):
    native = _core._native_language(text)
    assert native in ("defer", py3langid.classify(text)[0])
