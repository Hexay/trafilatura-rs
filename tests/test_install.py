"""install() only patches when the installed trafilatura/lxml are the versions trafilatura_rs reproduces."""

import pytest
import trafilatura
import trafilatura.core

import trafilatura_rs


@pytest.fixture
def restore_extract():
    original = trafilatura.core.extract
    yield
    trafilatura.extract = trafilatura.core.extract = original


def test_matching_versions_patch(restore_extract):
    assert trafilatura_rs.mismatched_versions() == {}
    original = trafilatura_rs.install()
    assert original is not None and trafilatura.extract is not original


@pytest.mark.parametrize("pkg,ver", [("trafilatura", "2.3.1"), ("lxml", "5.3.0")])
def test_mismatched_version_leaves_trafilatura_alone(monkeypatch, restore_extract, pkg, ver):
    real = trafilatura_rs._installed_version
    monkeypatch.setattr(trafilatura_rs, "_installed_version", lambda p: ver if p == pkg else real(p))
    assert trafilatura_rs.mismatched_versions() == {pkg: ver}
    before = trafilatura.extract
    with pytest.warns(RuntimeWarning, match="left unpatched"):
        assert trafilatura_rs.install() is None
    assert trafilatura.extract is before
