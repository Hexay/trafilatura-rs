"""Shared by the parity tests and the release smoke test: fixtures, configs and the engines being compared. Parity is
checked against whichever trafilatura is installed (a REPRODUCES pair)."""
import json
from importlib.metadata import version
from pathlib import Path

FIXTURES = Path(__file__).parent / "fixtures" / "pages.jsonl"
TRAFILATURA = version("trafilatura")
# 1.11's fast-mode flag; 2.x renamed it (no_fallback still works there, with a DeprecationWarning).
FAST = "no_fallback" if TRAFILATURA.startswith("1.") else "fast"

# extract() keyword sets of trafilatura_rs.VERIFIED_OUTPUTS; fineweb is datatrove's Trafilatura call, `<cfg>_comments`
# adds include_comments=True (trafilatura's default). "url": True passes the page's URL. Mirrors
# harness/ref_stages.py and harness/meta_configs.py.
_BASE = {
    "fineweb": {"favor_precision": True, "deduplicate": True},
    "balanced": {},
    "recall": {"favor_recall": True},
    "fast": {FAST: True},
    "notables": {"include_tables": False},
}
_FOCUS = {"precision": {"favor_precision": True}, "balanced": {}, "recall": {"favor_recall": True}, "fast": {FAST: True}}
_OUTPUTS = {
    "md": {"output_format": "markdown"},
    "meta": {"with_metadata": True, "url": True},
    "json": {"output_format": "json", "with_metadata": True, "url": True, "record_id": "r1"},
    "jsontext": {"output_format": "json", "record_id": "r1"},
}
# deduplicate=True on the precision and recall configs without metadata (with metadata it stays on Python)
_GRID = {
    f"{f}_{o}": {**fv, **ov, "deduplicate": f in ("precision", "recall") and not ov.get("with_metadata")}
    for f, fv in _FOCUS.items() for o, ov in _OUTPUTS.items()
}
_GRID["balanced_mdmeta"] = {"output_format": "markdown", "with_metadata": True, "url": True}
CONFIGS = {
    **{k: {**v, "include_comments": False} for k, v in {**_BASE, **_GRID}.items()},
    **{f"{k}_comments": {**v, "include_comments": True}
       for k, v in {**_BASE, "precision": {"favor_precision": True}, **_GRID}.items()},
}


def pages():
    with open(FIXTURES, encoding="utf-8") as f:
        return [json.loads(line) for line in f]


def kwargs_for(cfg, page):
    kwargs = dict(CONFIGS[cfg])
    if kwargs.pop("url", False):
        kwargs["url"] = page["url"]
    return kwargs


def run(engine, html, kwargs):
    """One extract() call; an exception becomes 'EXC:<type>' since its type is part of the compared output."""
    if engine == "trafilatura_rs":
        import trafilatura_rs

        fn = trafilatura_rs.extract
        assert trafilatura_rs.supports(**kwargs), f"would fall back to Python: {kwargs}"
    else:
        from trafilatura import extract as fn
    try:
        return fn(html, **kwargs)
    except Exception as e:  # noqa: BLE001
        return f"EXC:{type(e).__name__}"
