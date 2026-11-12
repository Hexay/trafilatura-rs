"""Parity for the markdown / metadata / json configs and the option extras (harness/meta_configs.py): the installed
Python trafilatura (1.11 in .venv, 2.3.1 in .venv-t23) vs the Rust core reproducing it (through the extension module,
so htmldate's dateutil/dateparser leaves run as callbacks into Python).
  ref_meta.py ref  docs.jsonl out.jsonl <cfg>    trafilatura.extract(input, **kwargs_for(cfg, doc))
  ref_meta.py rust docs.jsonl out.jsonl <cfg>    the same call's Rust plan (trafilatura_rs._api), no fallback
  ref_meta.py show docs.jsonl <i> <cfg>          print both outputs of one doc
  ref_meta.py list                               config names
`bytes` configs need a docs file with raw payloads (harness/export_raw.py). Wall-clock inputs are pinned in both
modes: htmldate's max_date via date_extraction_params and dateutil's default for missing fields (see
docs/metadata-formats-scope.md)."""
import base64
import json
import sys
from datetime import datetime
from functools import partial

from meta_configs import CONFIGS, PINNED_DAY, kwargs_for


def pin_wall_clock():
    import htmldate.extractors as ex

    ex.dateutil_parse = partial(ex.dateutil_parse, default=datetime.fromisoformat(PINNED_DAY))


def python_extract():
    from trafilatura import extract

    return extract


def rust_extract():
    import trafilatura_rs

    def run(html, **kw):
        plan = trafilatura_rs._api._plan(html, kw, trafilatura_rs.engine(), verified=False)
        assert plan is not None, f"no Rust code path: {kw}"
        return trafilatura_rs._core.extract(plan[0], **plan[1])

    return run


def docs(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            d = json.loads(line)
            if "raw_b64" in d:
                d["raw"] = base64.b64decode(d.pop("raw_b64"))
            yield d


def output(extract, doc, cfg):
    html, kw = kwargs_for(cfg, doc)
    try:
        return extract(html, **kw)
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        return f"EXC:{type(e).__name__}"


def main():
    mode = sys.argv[1]
    if mode == "list":
        print(" ".join(CONFIGS))
        return
    path = sys.argv[2]
    pin_wall_clock()
    if mode == "show":
        i, cfg = int(sys.argv[3]), sys.argv[4]
        doc = next(d for d in docs(path) if d["i"] == i)
        for name, ex in (("python", python_extract()), ("rust", rust_extract())):
            print(f"== {name}\n{output(ex, doc, cfg)}")
        return
    out_path, cfg = sys.argv[3:5]
    extract = python_extract() if mode == "ref" else rust_extract()
    with open(out_path, "w", encoding="utf-8") as out:
        for d in docs(path):
            out.write(json.dumps({"i": d["i"], "out": output(extract, d, cfg)}) + "\n")


if __name__ == "__main__":
    main()
