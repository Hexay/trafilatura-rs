"""trafilatura_rs.extract == trafilatura.extract (the installed REPRODUCES pair) on the committed fixtures, per
verified config."""
import json
import subprocess
import sys
from pathlib import Path

import pytest
from parity import CONFIGS, FAST, TRAFILATURA, features, kwargs_for, pages, run

# 3 passes so repeated segments cross trafilatura's duplicate threshold (max_repetitions=2).
DEDUP_PASSES = 3
DEDUP_CONFIGS = [c for c in CONFIGS if CONFIGS[c].get("deduplicate")]


def test_installed_pair_is_reproduced():
    import trafilatura_rs

    assert not trafilatura_rs.mismatched_versions()
    assert trafilatura_rs.engine() == TRAFILATURA


def test_every_config_is_verified():
    import trafilatura_rs

    combos = {
        (features(c), (c.get("output_format", "txt"), c.get("with_metadata", False)),
         (c.get("favor_precision", False), c.get("favor_recall", False), c.get("include_tables", True),
          c.get(FAST, False), c["include_comments"]))
        for c in CONFIGS.values()
    }
    verified = {(feats, out, combo) for feats in {features(c) for c in CONFIGS.values()}
                for out, combos in trafilatura_rs.verified_outputs(features=feats).items() for combo in combos}
    assert combos == verified


@pytest.mark.parametrize("cfg", [c for c in CONFIGS if c not in DEDUP_CONFIGS])
def test_parity(cfg):
    diffs = [(p["i"], p["why"]) for p in pages()
             if run("trafilatura_rs", p["html"], kwargs_for(cfg, p)) != run("trafilatura", p["html"], kwargs_for(cfg, p))]
    assert not diffs


def dedup_outputs(engine, cfg):
    worker = Path(__file__).with_name("dedup_worker.py")
    out = subprocess.run([sys.executable, str(worker), engine, cfg, str(DEDUP_PASSES)], check=True,
                         capture_output=True)
    return json.loads(out.stdout)


@pytest.mark.parametrize("cfg", DEDUP_CONFIGS)
def test_parity_dedup_fresh_process(cfg):
    docs = pages()
    ours, ref = dedup_outputs("trafilatura_rs", cfg), dedup_outputs("trafilatura", cfg)
    assert len(ours) == len(ref) == DEDUP_PASSES * len(docs)
    diffs = [(k // len(docs), docs[k % len(docs)]["i"]) for k, (a, b) in enumerate(zip(ours, ref)) if a != b]
    assert not diffs
    assert ref[: len(docs)] != ref[-len(docs) :], "dedup never kicked in; the fixtures no longer exercise it"
