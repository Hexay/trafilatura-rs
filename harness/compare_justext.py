"""Compare ref_justext.jsonl (Python) with the Rust justext_parity output.
  compare_justext.py ref.jsonl rust.jsonl"""
import json
import sys
from collections import Counter


def load(path):
    with open(path, encoding="utf-8") as f:
        return {r["i"]: r["h"] for r in map(json.loads, f)}


def kind(h):
    return "None" if h is None else h if h.startswith("EXC:") else "hash"


def main():
    ref, rust = load(sys.argv[1]), load(sys.argv[2])
    common = sorted(ref.keys() & rust.keys())
    bad = [i for i in common if ref[i] != rust[i]]
    print(f"docs: ref={len(ref)} rust={len(rust)} compared={len(common)} mismatches={len(bad)}")
    print("ref kinds:", dict(Counter(kind(ref[i]) for i in common)))
    print("mismatch kinds (ref->rust):", dict(Counter(f"{kind(ref[i])}->{kind(rust[i])}" for i in bad)))
    print("first:", bad[:20])


if __name__ == "__main__":
    main()
