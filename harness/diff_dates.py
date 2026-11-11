"""Differential test of the native date shortcuts (crates/trafilatura-rs/src/dates) against Python.
  diff_dates.py collect <docs.jsonl> <strings.jsonl> <cfg>...   log every string the corpus feeds into htmldate's
                                                                 dateutil_parse / has_date_cue (/ _external_date)
  diff_dates.py check <strings.jsonl>                            compare every decided native result with Python
Every string is tried with both dateutil defaults (pinned day, today) and as a dateparser input; exit 1 on a
mismatch."""
import json
import os
import sys
from collections import Counter
from datetime import datetime

sys.path.insert(0, os.path.dirname(__file__))


def collect(docs_path, out_path, cfgs):
    os.environ["TRAFILATURA_RS_NATIVE_DATES"] = "0"
    import htmldate.extractors as ex

    import ref_meta

    ref_meta.pin_wall_clock()
    seen = set()
    cue, parse = ex.has_date_cue, ex.dateutil_parse

    def logged_cue(s, lo, hi):
        seen.add((s, lo.year, hi.year))
        return cue(s, lo, hi)

    def logged_parse(s, **kwargs):
        seen.add((s, None, None))
        return parse(s, **kwargs)

    ex.has_date_cue, ex.dateutil_parse = logged_cue, logged_parse
    extract = ref_meta.rust_extract()
    for cfg in cfgs:
        for doc in ref_meta.docs(docs_path):
            ref_meta.output(extract, doc, cfg)
    with open(out_path, "w", encoding="utf-8") as out:
        for s, lo, hi in sorted(seen, key=lambda r: (r[0], r[1] or 0, r[2] or 0)):
            out.write(json.dumps({"s": s, "min": lo, "max": hi}, ensure_ascii=False) + "\n")
    print(f"{len(seen)} distinct calls")


def as_tuple(d):
    off = d.utcoffset()
    off = None if off is None else off.days * 86400 + off.seconds
    return (d.year, d.month, d.day, d.hour, d.minute, d.second, d.microsecond, off)


def py_dateutil(s, default):
    from dateutil.parser import parse

    kwargs = {"fuzzy": False} if default is None else {"fuzzy": False, "default": default}
    try:
        return as_tuple(parse(s, **kwargs))
    except (OverflowError, TypeError, ValueError):
        return None


def py_external(s, lo, hi):
    import htmldate.extractors as ex

    if not ex.has_date_cue(s, datetime(lo, 1, 1), datetime(hi, 1, 1)):
        return None
    return ex._external_date(s)


def check(path):
    from trafilatura_rs import _core

    from ref_meta import PINNED_DAY

    import htmldate.extractors as ex

    pinned = datetime.fromisoformat(PINNED_DAY)
    rows = [json.loads(line) for line in open(path, encoding="utf-8")]
    ranges = {(r["min"], r["max"]) for r in rows if r["min"] is not None} or {(1995, 2026)}
    stats, bad = Counter(), []
    for s in sorted({r["s"] for r in rows}):
        for name, default in (("dateutil pinned", pinned), ("dateutil today", None)):
            native = _core._native_dateutil(s, default)
            stats[name, "defer" if native == "defer" else "native"] += 1
            if native != "defer" and native != (expected := py_dateutil(s, default)):
                bad.append((name, s, native, expected))
    for s, lo, hi in sorted({(r["s"], lo, hi) for r in rows for lo, hi in ranges}):
        cue, none = _core._native_external_date(s, lo, hi)
        stats["has_date_cue", "defer" if cue is None else "native"] += 1
        if cue is not None and cue != (expected := ex.has_date_cue(s, datetime(lo, 1, 1), datetime(hi, 1, 1))):
            bad.append(("has_date_cue", s, cue, expected))
        stats["external_date", "native None" if none else "defer"] += 1
        if none and (expected := py_external(s, lo, hi)) is not None:
            bad.append(("external_date", (s, lo, hi), None, expected))
    for (name, kind), n in sorted(stats.items()):
        print(f"{name:16} {kind:12} {n}")
    for b in bad[:50]:
        print("MISMATCH", *map(repr, b))
    print(f"{len(bad)} mismatches")
    return 1 if bad else 0


if __name__ == "__main__":
    if sys.argv[1] == "collect":
        sys.exit(collect(sys.argv[2], sys.argv[3], sys.argv[4:]))
    sys.exit(check(sys.argv[2]))
