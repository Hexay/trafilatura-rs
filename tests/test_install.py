"""Adoption paths: install()/uninstall(), the drop-in extract(), TRAFILATURA_RS=1 and `python -m trafilatura_rs`."""

import os
import subprocess
import sys

import pytest
import trafilatura
import trafilatura.core
from parity import pages

import trafilatura_rs

HTML = pages()[0]["html"]
_REF = {"extract": trafilatura.core.extract}  # in a dict: install() swaps module attributes holding the original


@pytest.fixture(autouse=True)
def clean_install():
    trafilatura_rs.uninstall()
    yield
    trafilatura_rs.uninstall()
    assert trafilatura.extract is _REF["extract"]


def test_install_patches_earlier_imported_copies():
    from trafilatura import extract as early

    assert trafilatura_rs.install() is _REF["extract"]
    assert trafilatura_rs.install() is _REF["extract"]  # idempotent
    assert trafilatura.extract is trafilatura_rs.extract
    assert early is _REF["extract"]  # a local variable can't be patched; module attributes are
    trafilatura_rs.uninstall()
    assert not trafilatura_rs.is_installed() and trafilatura.extract is _REF["extract"]


@pytest.mark.parametrize("pkg,ver", [("trafilatura", "2.3.1"), ("lxml", "5.3.0")])
def test_mismatched_versions_stay_on_python(monkeypatch, pkg, ver):
    from trafilatura_rs import _versions

    real = _versions._installed_version
    monkeypatch.setattr(_versions, "_installed_version", lambda p: ver if p == pkg else real(p))
    assert trafilatura_rs.mismatched_versions() == {pkg: ver}
    with pytest.warns(RuntimeWarning, match="left unpatched"):
        assert trafilatura_rs.install() is None
    assert trafilatura.extract is _REF["extract"]
    monkeypatch.setattr(trafilatura_rs._api, "_rust_extract", lambda *a, **k: pytest.fail("ran Rust"))
    trafilatura_rs.extract(HTML, include_comments=False)


def test_mismatched_htmldate_keeps_metadata_on_python(monkeypatch):
    from trafilatura_rs import _versions

    real = _versions._installed_version
    monkeypatch.setattr(_versions, "_installed_version", lambda p: "1.9.0" if p == "htmldate" else real(p))
    assert trafilatura_rs.mismatched_versions() == {}
    assert trafilatura_rs.mismatched_versions(with_metadata=True) == {"htmldate": "1.9.0"}
    monkeypatch.setattr(trafilatura_rs._api, "_rust_extract", lambda *a, **k: pytest.fail("ran Rust"))
    trafilatura_rs.extract(HTML, include_comments=False, with_metadata=True)


def test_supports():
    assert trafilatura_rs.supports(include_comments=False, favor_precision=True, deduplicate=True)
    assert trafilatura_rs.supports()  # trafilatura's defaults, include_comments=True
    assert trafilatura_rs.supports(favor_precision=True, deduplicate=True)
    assert not trafilatura_rs.supports(output_format="xml")
    assert not trafilatura_rs.supports(include_comments=False, output_format="xml")
    assert not trafilatura_rs.supports(include_comments=False, favor_precision=True, favor_recall=True)
    assert trafilatura_rs.supports(include_comments=False, output_format="markdown", favor_precision=True)
    assert trafilatura_rs.supports(include_comments=False, output_format="json", with_metadata=True, url="http://a.b/")
    assert not trafilatura_rs.supports(include_comments=False, with_metadata=True, deduplicate=True)
    assert not trafilatura_rs.supports(include_comments=False, date_extraction_params={"original_date": False})


def test_metadata_date_params():
    kwargs = dict(include_comments=False, with_metadata=True, output_format="json",
                  date_extraction_params={"original_date": True, "extensive_search": True, "max_date": "2020-01-01"})
    assert trafilatura_rs.supports(**kwargs)
    assert trafilatura_rs.extract(HTML, **kwargs) == _REF["extract"](HTML, **kwargs)


@pytest.mark.parametrize("kwargs", [dict(favor_precision=True, favor_recall=True), dict(with_metadata=True),
                                    dict(output_format="json"), dict(include_comments=False, output_format="xml"),
                                    dict(include_comments=False, with_metadata=True, deduplicate=True),
                                    dict(include_comments=False, with_metadata=True, favor_recall=True)])
def test_uncovered_calls_fall_back_to_python(kwargs):
    assert trafilatura_rs.extract(HTML, **kwargs) == _REF["extract"](HTML, **kwargs)


def test_unsupported_document_falls_back_to_python(monkeypatch):
    def unsupported(*args, **kwargs):
        raise trafilatura_rs._api.Unsupported("forced")

    monkeypatch.setattr(trafilatura_rs._api, "_rust_extract", unsupported)
    kwargs = dict(include_comments=False, with_metadata=True)
    assert trafilatura_rs.extract(HTML, **kwargs) == _REF["extract"](HTML, **kwargs)


def test_positional_options_bind_like_trafilatura():
    # extract(filecontent, url, record_id, no_fallback, favor_precision, ...)
    assert trafilatura_rs.extract(HTML, None, None, False, True, include_comments=False) == _REF["extract"](
        HTML, favor_precision=True, include_comments=False)


def _python(code, **env):
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True,
                         env={**os.environ, "TRAFILATURA_RS": "0", **env})
    assert out.returncode == 0, out.stderr
    return out.stdout.strip()


def test_autoinstall_waits_for_trafilatura_import():
    code = ("import sys, trafilatura_rs._autoinstall; assert 'trafilatura' not in sys.modules; "
            "from trafilatura import extract; import trafilatura_rs; print(extract is trafilatura_rs.extract)")
    assert _python(code) == "True"


def test_runner_installs_for_unmodified_script(tmp_path):
    script = tmp_path / "s.py"
    script.write_text("import trafilatura, trafilatura_rs, sys\n"
                      "print(trafilatura.extract is trafilatura_rs.extract, sys.argv[1:])\n")
    assert _python(f"import runpy, sys; sys.argv = ['x', {str(script)!r}, 'a']; "
                   "runpy.run_module('trafilatura_rs', run_name='__main__')") == "True ['a']"
