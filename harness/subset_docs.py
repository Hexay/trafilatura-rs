"""Copy the docs whose id is listed in ids.txt (one per line) out of a corpus file, in corpus order.
  subset_docs.py ids.txt docs.jsonl out.jsonl"""
import re
import sys

ID = re.compile(rb'^\{"i": (\d+),')


def main():
    ids_path, docs_path, out_path = sys.argv[1:4]
    with open(ids_path, encoding="utf-8") as f:
        want = {int(line) for line in f if line.strip()}
    with open(docs_path, "rb") as src, open(out_path, "wb") as out:
        for line in src:
            m = ID.match(line)
            if m and int(m.group(1)) in want:
                out.write(line)
                want.discard(int(m.group(1)))
                if not want:
                    break


if __name__ == "__main__":
    main()
