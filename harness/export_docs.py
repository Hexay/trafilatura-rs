"""Write the parity corpus: the first N URLFilter-passing HTML records of the given WARCs as {"i", "url", "html"} JSONL.
usage: export_docs.py [--limit N] out.jsonl a.warc.gz [b.warc.gz ...]"""
import argparse
import json
from pathlib import Path


def main():
    from datatrove.pipeline.filters import URLFilter
    from datatrove.pipeline.readers import WarcReader

    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=0, help="stop after N docs (0 = all)")
    ap.add_argument("out")
    ap.add_argument("warcs", nargs="+")
    args = ap.parse_args()
    n = 0
    with open(args.out, "w", encoding="utf-8") as f:
        for warc in args.warcs:
            p = Path(warc).resolve()
            docs = URLFilter().run(WarcReader(str(p.parent), glob_pattern=p.name).run(None, 0, 1), 0, 1)
            for d in docs:
                f.write(json.dumps({"i": n, "url": d.metadata.get("url"), "id": d.id, "html": d.text}) + "\n")
                n += 1
                if n == args.limit:
                    break
            if n == args.limit:
                break
    print(f"docs={n}")


if __name__ == "__main__":
    main()
