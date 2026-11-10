"""Shared by the parity tests and the release smoke test: fixtures, configs and the engines being compared."""
import json
from pathlib import Path

FIXTURES = Path(__file__).parent / "fixtures" / "pages.jsonl"

# extract() keyword sets of trafex.VERIFIED; fineweb is datatrove's Trafilatura call. Mirrors harness/ref_stages.py.
CONFIGS = {
    "fineweb": {"favor_precision": True, "deduplicate": True},
    "balanced": {},
    "recall": {"favor_recall": True},
    "fast": {"no_fallback": True},
    "notables": {"include_tables": False},
}


def pages():
    with open(FIXTURES, encoding="utf-8") as f:
        return [json.loads(line) for line in f]


def run(engine, html, kwargs):
    """One extract() call; an exception becomes 'EXC:<type>' since its type is part of the compared output."""
    if engine == "trafex":
        import trafex

        fn = trafex.extract
    else:
        from trafilatura import extract as fn
    try:
        return fn(html, include_comments=False, **kwargs)
    except Exception as e:  # noqa: BLE001
        return f"EXC:{type(e).__name__}"
