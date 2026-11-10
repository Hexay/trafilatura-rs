"""Write the parity corpus: every URLFilter-passing HTML record of the given WARCs as {"i", "url", "html"} JSONL.
usage: export_docs.py out.jsonl a.warc.gz [b.warc.gz ...]"""
import importlib.machinery
import json
import sys
import types
from pathlib import Path

# python-magic is only needed for records lacking WARC-Identified-Payload-Type; CC-MAIN records all have it.
_magic = types.ModuleType("magic")
_magic.__spec__ = importlib.machinery.ModuleSpec("magic", None)
sys.modules.setdefault("magic", _magic)


def main():
    from datatrove.pipeline.filters import URLFilter
    from datatrove.pipeline.readers import WarcReader

    out, warcs = sys.argv[1], sys.argv[2:]
    n = 0
    with open(out, "w", encoding="utf-8") as f:
        for warc in warcs:
            p = Path(warc).resolve()
            docs = URLFilter().run(WarcReader(str(p.parent), glob_pattern=p.name).run(None, 0, 1), 0, 1)
            for d in docs:
                f.write(json.dumps({"i": n, "url": d.metadata.get("url"), "html": d.text}) + "\n")
                n += 1
    print(f"docs={n}")


if __name__ == "__main__":
    main()
