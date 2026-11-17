"""Differential test of the native py3langid classify (crates/trafilatura-rs/src/langid.rs) against Python.
  diff_langid.py <ref.jsonl>...   every raw_text / text / comments in the references' JSON outputs, with its first
                                  10, 40, 150 and 600 characters and its upper-cased form
Prints how many texts the port decided and deferred; exit 1 on a mismatch."""
import json
import sys
from collections import Counter

import py3langid
import trafilatura.utils  # noqa: F401 - the native classify checks trafilatura's language_classifier

from trafilatura_rs._core import _native_language


def texts(paths):
    for path in paths:
        with open(path, encoding="utf-8") as f:
            for line in f:
                out = json.loads(line).get("out")
                if not out or out.startswith("EXC:"):
                    continue
                doc = json.loads(out)
                for key in ("raw_text", "text", "comments"):
                    text = doc.get(key) or ""
                    if text:
                        yield text
                        yield text.upper()
                        yield from (text[:n] for n in (10, 40, 150, 600) if len(text) > n)


def main(paths):
    stats, seen = Counter(), set()
    for text in texts(paths):
        if text in seen:
            continue
        seen.add(text)
        native = _native_language(text)
        if native == "defer":
            stats["deferred"] += 1
            continue
        stats["decided"] += 1
        expected = py3langid.classify(text)[0]
        if native != expected:
            stats["MISMATCH"] += 1
            print("MISMATCH", native, expected, repr(text[:100]))
    print(dict(stats))
    return 1 if stats["MISMATCH"] else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
