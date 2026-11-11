"""Shared by the parity tests and the release smoke test: fixtures, configs and the engines being compared."""
import json
from pathlib import Path

FIXTURES = Path(__file__).parent / "fixtures" / "pages.jsonl"

# extract() keyword sets of trafilatura_rs.VERIFIED; fineweb is datatrove's Trafilatura call, `<cfg>_comments` adds
# include_comments=True (trafilatura's default). Mirrors harness/ref_stages.py.
_BASE = {
    "fineweb": {"favor_precision": True, "deduplicate": True},
    "balanced": {},
    "recall": {"favor_recall": True},
    "fast": {"no_fallback": True},
    "notables": {"include_tables": False},
}
CONFIGS = {
    **{k: {**v, "include_comments": False} for k, v in _BASE.items()},
    **{f"{k}_comments": {**v, "include_comments": True}
       for k, v in {**_BASE, "precision": {"favor_precision": True}}.items()},
}


def pages():
    with open(FIXTURES, encoding="utf-8") as f:
        return [json.loads(line) for line in f]


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
