"""trafilatura_rs.extract == trafilatura.extract (1.11.0 + lxml 6.1.3) on the committed fixtures, per VERIFIED
config."""
import json
import subprocess
import sys
from pathlib import Path

import pytest
from parity import CONFIGS, pages, run

# 3 passes so repeated segments cross trafilatura's duplicate threshold (max_repetitions=2).
DEDUP_PASSES = 3


def test_every_config_is_verified():
    import trafilatura_rs

    combos = {
        (c.get("favor_precision", False), c.get("favor_recall", False), c.get("include_tables", True),
         c.get("no_fallback", False))
        for c in CONFIGS.values()
    }
    assert combos == trafilatura_rs.VERIFIED


@pytest.mark.parametrize("cfg", [c for c in CONFIGS if not CONFIGS[c].get("deduplicate")])
def test_parity(cfg):
    kwargs = CONFIGS[cfg]
    diffs = [(p["i"], p["why"]) for p in pages()
             if run("trafilatura_rs", p["html"], kwargs) != run("trafilatura", p["html"], kwargs)]
    assert not diffs


def dedup_outputs(engine):
    worker = Path(__file__).with_name("dedup_worker.py")
    out = subprocess.run([sys.executable, str(worker), engine, str(DEDUP_PASSES)], check=True, capture_output=True)
    return json.loads(out.stdout)


def test_parity_fineweb_dedup_fresh_process():
    docs = pages()
    ours, ref = dedup_outputs("trafilatura_rs"), dedup_outputs("trafilatura")
    assert len(ours) == len(ref) == DEDUP_PASSES * len(docs)
    diffs = [(k // len(docs), docs[k % len(docs)]["i"]) for k, (a, b) in enumerate(zip(ours, ref)) if a != b]
    assert not diffs
    assert ref[: len(docs)] != ref[-len(docs) :], "dedup never kicked in; the fixtures no longer exercise it"
