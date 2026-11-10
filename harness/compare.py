"""Compare reference and Rust harness outputs.
  compare.py stages ref.jsonl rust.jsonl    mismatches per stage (a doc counts at its first differing stage)
  compare.py final ref.jsonl rust.jsonl     extract() output mismatches
A `2` suffix (stages2/final2) compares trafilatura 2.2.0 outputs (stage list from ref2_stages.py)."""
import json
import sys
from collections import Counter

import ref2_stages
import ref_stages


def load(path):
    with open(path, encoding="utf-8") as f:
        return {r["i"]: r for r in map(json.loads, f)}


def main():
    mode, a, b = sys.argv[1:4]
    STAGES = ref2_stages.STAGES if mode.endswith("2") else ref_stages.STAGES
    mode = mode.rstrip("2")
    ref, rust = load(a), load(b)
    common = sorted(ref.keys() & rust.keys())
    print(f"docs: ref={len(ref)} rust={len(rust)} compared={len(common)}")
    if mode == "final":
        bad = [i for i in common if ref[i]["out"] != rust[i]["out"]]
        print(f"final mismatches={len(bad)} first={bad[:15]}")
        return
    first_bad, unported, examples = Counter(), Counter(), {}
    for i in common:
        hr, hs = ref[i]["h"], rust[i]["h"]
        for st in STAGES:
            if st in hr and st not in hs:
                unported[st] += 1
                break
            if hr.get(st) != hs.get(st):
                first_bad[st] += 1
                examples.setdefault(st, []).append(i)
                break
    bad = sum(first_bad.values())
    print(f"identical through every stage the port has: {len(common) - bad}/{len(common)}")
    for st in STAGES:
        if first_bad[st]:
            print(f"  first divergence at {st:<15} {first_bad[st]:6d}  e.g. {examples[st][:8]}")
    if unported:
        print("  stopped at unported stage: " + ", ".join(f"{st}={n}" for st, n in unported.items()))


if __name__ == "__main__":
    main()
