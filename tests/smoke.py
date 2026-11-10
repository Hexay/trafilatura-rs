"""Release smoke test for an installed wheel: one fixture (the BOM + astral page, which goes through iconv) must
match trafilatura under datatrove's config. usage: python tests/smoke.py"""
from parity import CONFIGS, pages, run

page = next(p for p in pages() if p["why"] == "bom_astral")
ours, ref = run("trafex", page["html"], CONFIGS["fineweb"]), run("trafilatura", page["html"], CONFIGS["fineweb"])
assert ours and not ours.startswith("EXC:"), ours
assert ours == ref, (ours[:200], (ref or "")[:200])
print(f"smoke ok: {len(ours)} chars")
