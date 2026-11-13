"""Options outside the config grid run in Rust and match trafilatura on the fixtures: bytes input, options=Extractor
(the CLI path), config / settingsfile, blacklists, only_with_metadata, prune_xpath, date flags, include_formatting
and max_tree_size (1.11). Paths the port leaves to Python must fall back with trafilatura's own result or exception."""
import gzip
from configparser import ConfigParser

import pytest
from lxml.etree import XMLSyntaxError
import trafilatura.core
from parity import TRAFILATURA, pages
from trafilatura.settings import DEFAULT_CONFIG, Extractor

import trafilatura_rs
from trafilatura_rs import _api, _core

V1 = TRAFILATURA.startswith("1.")
PAGES = pages()[:10]
HTML = [p["html"] for p in PAGES]
PY_EXTRACT = trafilatura.core.extract


def outcome(fn, html, kwargs):
    try:
        return fn(html, **kwargs)
    except Exception as e:  # noqa: BLE001
        return f"EXC:{type(e).__name__}"


def assert_rust_parity(inputs, rust=True, **kwargs):
    """Rust takes (or, with rust=False, declines) the call, and the result equals trafilatura's on every input."""
    for html in inputs:
        assert (_api._plan(html, kwargs, trafilatura_rs.engine()) is not None) == rust, kwargs
        assert outcome(trafilatura_rs.extract, html, kwargs) == outcome(PY_EXTRACT, html, kwargs)


def small_config():
    config = ConfigParser()
    config.read_dict({s: dict(DEFAULT_CONFIG[s]) for s in DEFAULT_CONFIG})
    config["DEFAULT"].update({"MIN_EXTRACTED_SIZE": "400", "MIN_OUTPUT_SIZE": "60", "MAX_REPETITIONS": "1",
                              "MIN_DUPLCHECK_SIZE": "40", "MAX_TREE_SIZE": "40", "EXTENSIVE_DATE_SEARCH": "off"})
    return config


@pytest.mark.parametrize("encode", [str.encode, lambda s: s.encode("cp1252", "replace"),
                                   lambda s: gzip.compress(s.encode())])
def test_bytes(encode):
    assert_rust_parity([encode(h) for h in HTML])
    assert_rust_parity([encode(h) for h in HTML], include_comments=False, output_format="json", with_metadata=True)


def test_cli_options():
    assert_rust_parity([h.encode() for h in HTML], options=Extractor())
    markdown = Extractor(output_format="markdown")
    markdown.formatting = False  # 1.11's args_to_extractor without --formatting
    assert_rust_parity(HTML, options=markdown)
    assert _api._plan(HTML[0], {"options": markdown}, trafilatura_rs.engine())[1]["output_format"] == "txt"
    assert_rust_parity(HTML, options=Extractor(output_format="json", with_metadata=True, comments=False))


def test_config_and_settingsfile(tmp_path):
    from trafilatura.meta import reset_caches

    reset_caches()  # both dedup caches must have seen the same documents
    trafilatura_rs.reset_caches()
    assert_rust_parity(HTML, config=small_config(), deduplicate=True)
    assert_rust_parity(HTML, config=small_config(), with_metadata=True, output_format="json")
    path = tmp_path / "settings.cfg"
    with open(path, "w") as f:
        small_config().write(f)
    assert_rust_parity(HTML, settingsfile=str(path), favor_precision=True)
    assert_rust_parity(HTML, rust=False, settingsfile=str(tmp_path / "missing.cfg"))


def test_blacklists_and_only_with_metadata():
    urls = [p["url"] for p in PAGES]
    assert_rust_parity(HTML, url_blacklist=set(urls[::2]), url=urls[0], output_format="json")
    assert_rust_parity(HTML, author_blacklist={"ADMIN", "Name"}, with_metadata=True)
    assert_rust_parity(HTML, author_blacklist={"admin"})  # 2.x implies metadata, 1.11 ignores it
    assert_rust_parity(HTML, only_with_metadata=True)
    assert_rust_parity(HTML, rust=False, url_blacklist="http://example.com/")  # substring test in Python


@pytest.mark.parametrize("xpath", ["//footer", ["//div[contains(@class, 'comment')]", "//p[1]"], "//nav|//h1"])
def test_prune_xpath(xpath):
    assert_rust_parity(HTML, prune_xpath=xpath)


@pytest.mark.parametrize("xpath", ["//@id", "//p/text()", "/html", "//p[re:test(., 'a')]", "count(//p)"])
def test_prune_xpath_left_to_python(xpath):
    html = '<html><body><article id="a"><p>' + "text " * 80 + "</p><p>more</p></article></body></html>"
    rust = _api._plan(html, {"prune_xpath": xpath}, trafilatura_rs.engine())[1]
    with pytest.raises(_core.Unsupported):
        _core.extract(html, **rust)
    assert outcome(trafilatura_rs.extract, html, {"prune_xpath": xpath}) == outcome(PY_EXTRACT, html,
                                                                                    {"prune_xpath": xpath})


def test_invalid_prune_xpath_raises_like_python():
    assert outcome(trafilatura_rs.extract, HTML[0], {"prune_xpath": "//["}) == outcome(PY_EXTRACT, HTML[0],
                                                                                       {"prune_xpath": "//["})


@pytest.mark.parametrize("params", [{"max_date": "2026-06-30"}, {"original_date": True, "extensive_search": False,
                                                                 "max_date": "2026-06-30", "outputformat": "%Y-%m-%d"}])
def test_date_flags(params):
    assert_rust_parity(HTML, with_metadata=True, date_extraction_params=params)


def test_formatting():
    assert_rust_parity(HTML, include_formatting=True)
    assert _api._plan(HTML[0], {"include_formatting": True}, trafilatura_rs.engine())[1]["output_format"] == "markdown"
    assert_rust_parity(HTML, output_format="markdown", include_formatting=False)
    assert_rust_parity(HTML, rust=False, output_format="json", include_formatting=True)


LINKS_PAGE = ("<html><body><article><p>" + "Some text to keep. " * 20 + "</p><p>See <a href='/a?b=1#c'>one</a>, "
              "<a href='//cdn.example.org/x'>two</a>, <a href='http://[::1]/v6'>three</a>, <a>four</a> and "
              "<a href='?q'><img src='/i.png' alt=''></a> <img data-src='//e.org/p.jpg' title='t'>.</p>"
              + "<p>More text to keep around here.</p>" * 5 + "</article></body></html>")


@pytest.mark.parametrize("fmt", ["txt", "markdown", "json", "xml", "html"])
def test_links_and_images(fmt):
    verified = (fmt, False) in trafilatura_rs.verified_outputs(features=("include_links", "include_images"))
    for url in (None, "https://example.com/dir/page;p?x=1#f", "http://[::1]:8/"):
        assert_rust_parity([LINKS_PAGE], rust=verified, include_links=True, include_images=True,
                           output_format=fmt, url=url)
    assert_rust_parity([LINKS_PAGE], rust=(fmt, True) in trafilatura_rs.verified_outputs(), output_format=fmt,
                       with_metadata=True, url="https://example.com/")
    assert_rust_parity([LINKS_PAGE], rust=False, include_links=1, output_format=fmt)
    if fmt not in ("txt", "markdown"):
        assert_rust_parity([LINKS_PAGE], rust=False, output_format=fmt, include_formatting=True)


def test_xml_reparse_error():
    """An undeclared prefix (Word's <st1:city>) survives in a heading; lxml's re-parse then raises XMLSyntaxError."""
    html = ("<html><body><article><h3>Formed: <st1:city>Graz</st1:city></h3><p>" + "Some text to keep. " * 20
            + "</p>" + "<p>More text to keep around here.</p>" * 5 + "</article></body></html>")
    assert_rust_parity([html], output_format="xml")
    with pytest.raises(XMLSyntaxError, match="st1"):
        trafilatura_rs.extract(html, output_format="xml")


def test_max_tree_size():
    assert_rust_parity(HTML, rust=V1, max_tree_size=40)


def test_lone_surrogate_falls_back():
    html = "<html><body><article><p>" + "text \ud800 " * 60 + "</p></article></body></html>"
    assert outcome(trafilatura_rs.extract, html, {}) == outcome(PY_EXTRACT, html, {})
