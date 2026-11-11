"""Release smoke test for an installed wheel: one fixture (the BOM + astral page, which goes through iconv) must
match trafilatura under datatrove's config and the default call. usage: python tests/smoke.py"""
from parity import CONFIGS, pages, run

page = next(p for p in pages() if p["why"] == "bom_astral")
for cfg in ("fineweb", "balanced_comments"):
    ours = run("trafilatura_rs", page["html"], CONFIGS[cfg])
    ref = run("trafilatura", page["html"], CONFIGS[cfg])
    assert ours and not ours.startswith("EXC:"), ours
    assert ours == ref, (cfg, ours[:200], (ref or "")[:200])
    print(f"smoke ok ({cfg}): {len(ours)} chars")
