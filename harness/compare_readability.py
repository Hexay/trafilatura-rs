"""Compare ref_readability.jsonl with the Rust readability_parity output.
  compare_readability.py ref.jsonl rust.jsonl"""
import json
import sys


def load(path):
    out = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                r = json.loads(line)
            except json.JSONDecodeError:  # partial last line of a run still in progress
                continue
            out[r["i"]] = r
    return out


def main():
    ref, rust = load(sys.argv[1]), load(sys.argv[2])
    common = sorted(ref.keys() & rust.keys())
    print(f"docs: ref={len(ref)} rust={len(rust)} compared={len(common)}")
    for field in ("summary", "tr"):
        bad = [i for i in common if ref[i][field] != rust[i][field]]
        print(f"{field} mismatches={len(bad)} first={bad[:15]}")


if __name__ == "__main__":
    main()
