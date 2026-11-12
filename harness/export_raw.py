"""Add each record's raw HTTP payload (warcio's content_stream: de-chunked and content-decoded, as WARC pipelines read
it) to the parity corpus, for the bytes-input configs.
usage: export_raw.py docs.jsonl a.warc.gz out.jsonl     (records matched by WARC-Record-ID)"""
import base64
import json
import sys

from warcio.archiveiterator import ArchiveIterator


def main():
    docs_path, warc, out_path = sys.argv[1:4]
    with open(docs_path, encoding="utf-8") as f:
        docs = {d["id"]: d for d in map(json.loads, f)}
    raw = {}
    with open(warc, "rb") as f:
        for record in ArchiveIterator(f):
            rid = record.rec_headers.get_header("WARC-Record-ID")
            if record.rec_type == "response" and rid in docs:
                raw[rid] = record.content_stream().read()
    with open(out_path, "w", encoding="utf-8") as out:
        for d in sorted(docs.values(), key=lambda d: d["i"]):
            out.write(json.dumps({**d, "raw_b64": base64.b64encode(raw[d["id"]]).decode()}) + "\n")
    print(f"docs={len(docs)} raw={len(raw)}")


if __name__ == "__main__":
    main()
