"""datatrove's Trafilatura step with and without trafex.install(): outputs must be identical.
usage: test_install.py docs.jsonl [n]"""
import importlib.metadata  # noqa: F401  datatrove 0.10 uses it without importing it
import json
import sys
import time


def run(ext, pages):
    out, t = [], time.perf_counter()
    for html in pages:
        try:
            out.append(ext.extract(html))
        except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
            out.append(f"EXC:{type(e).__name__}")
    return out, time.perf_counter() - t


def main():
    from datatrove.pipeline.extractors import Trafilatura

    import trafex

    n = int(sys.argv[2]) if len(sys.argv) > 2 else 1000
    with open(sys.argv[1], encoding="utf-8") as f:
        pages = [json.loads(line)["html"] for _, line in zip(range(n), f)]
    ext = Trafilatura(favour_precision=True)
    stock, t_stock = run(ext, pages)
    trafex.install()
    fast, t_fast = run(ext, pages)
    bad = [i for i, (a, b) in enumerate(zip(stock, fast)) if a != b]
    print(f"docs={len(pages)} mismatches={len(bad)} first={bad[:10]}")
    print(f"datatrove Trafilatura.extract: stock {t_stock:.1f}s, trafex {t_fast:.1f}s, {t_stock / t_fast:.1f}x")


if __name__ == "__main__":
    main()
