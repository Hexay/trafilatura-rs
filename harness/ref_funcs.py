"""Parity and CPU time of the drop-ins besides extract(): the installed Python function vs trafilatura_rs's Rust path
(no fallback), in one process, document by document.
  ref_funcs.py run docs.jsonl out_dir <cfg> [cfg ...]   compare; mismatches go to out_dir/<cfg>.diff.jsonl
  ref_funcs.py show docs.jsonl <i> <cfg>                 print both results of one doc
  ref_funcs.py list                                      config names
Results compare by value; lxml trees by etree.tostring (and, separately, by the element classes in them). Exceptions
compare as "EXC:<type>"; a Rust `Unsupported` (the drop-in reruns Python) counts as a fallback, not a mismatch."""
import json
import os
import sys
import time

import trafilatura_rs
from lxml import etree
from trafilatura_rs import _bare, _core, _funcs

V1 = trafilatura_rs.engine() == "1.11.0"
FAST = "no_fallback" if V1 else "fast"
_FOCUS = {"precision": {"favor_precision": True}, "balanced": {}, "recall": {"favor_recall": True}, "fast": {FAST: True}}


def _bare_configs():
    configs = {}
    for focus, kw in _FOCUS.items():
        configs[f"bare_{focus}"] = ("bare_extraction", kw)
        configs[f"bare_{focus}_nocomments"] = ("bare_extraction", {**kw, "include_comments": False})
        configs[f"bare_{focus}_meta"] = ("bare_extraction", {**kw, "with_metadata": True, "url": True})
    return configs


# name -> (function, keywords); url=True passes the record's URL
CONFIGS = {
    "html2txt": ("html2txt", {}),
    "html2txt_noclean": ("html2txt", {"clean": False}),
    "baseline": ("baseline", {}),
    "metadata": ("extract_metadata", {}),
    "metadata_url": ("extract_metadata", {"default_url": True}),
    "find_date": ("find_date", {}),
    "find_date_url": ("find_date", {"url": True}),
    "find_date_fast": ("find_date", {"extensive_search": False, "original_date": True}),
    **_bare_configs(),
}


def python_function(name):
    from trafilatura_rs._install import original

    return original(name)


def _rust_bare(html, **kw):
    engine = trafilatura_rs.engine()
    planned = _bare._plan(html, kw, engine)
    assert planned is not None, f"no Rust path: {kw}"
    html, rust, date_params = planned
    return _bare.run_rust(html, rust, engine, kw.get("as_dict", V1), date_params)


def _rust_metadata(html, default_url=None):
    engine = trafilatura_rs.engine()
    kwargs = _funcs._metadata_kwargs(default_url, None, True, None)
    return _funcs.metadata_document(_core.extract_metadata(html, engine, **kwargs), engine, None)


def _rust_find_date(html, **kw):
    planned = _funcs._find_date_kwargs(html, kw)
    assert planned is not None, f"no Rust path: {kw}"
    return _core.find_date(planned[0], **planned[1])


RUST = {
    "html2txt": lambda html, clean=True: _core.html2txt(html, trafilatura_rs.engine(), clean),
    "baseline": lambda html: _funcs._body(*_core.baseline(html, trafilatura_rs.engine())),
    "extract_metadata": _rust_metadata,
    "find_date": _rust_find_date,
    "bare_extraction": _rust_bare,
}


def canon(x, classes):
    """A JSON-able form of a result; with `classes`, trees also record their element classes."""
    if isinstance(x, etree._Element):
        out = {"tree": etree.tostring(x, encoding="unicode")}
        if classes:
            out["classes"] = sorted({type(e).__name__ for e in x.iter()})
        return out
    if hasattr(x, "__slots__") and hasattr(x, "as_dict"):
        return {"Document": {k: canon(v, classes) for k, v in x.as_dict().items()}}
    if isinstance(x, dict):
        return {"dict": {k: canon(v, classes) for k, v in x.items()}}
    if isinstance(x, (list, tuple)):
        return {type(x).__name__: [canon(v, classes) for v in x]}
    return {type(x).__name__: x}


def outcome(fn, html, kw):
    start = time.process_time()
    try:
        result = fn(html, **kw)
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        result = f"EXC:{type(e).__name__}"
    return result, time.process_time() - start


def kwargs_for(cfg, doc):
    return {k: (doc["url"] if v is True and k in ("url", "default_url") else v) for k, v in CONFIGS[cfg][1].items()}


def docs(path):
    with open(path, encoding="utf-8") as f:
        yield from map(json.loads, f)


def run(path, out_dir, cfgs):
    stats = {c: {"ok": 0, "tree_classes": 0, "bad": 0, "fallback": 0, "py": 0.0, "rs": 0.0} for c in cfgs}
    diffs = {c: open(os.path.join(out_dir, f"{c}.diff.jsonl"), "w", encoding="utf-8") for c in cfgs}
    for doc in docs(path):
        for cfg in cfgs:
            name, kw, s = CONFIGS[cfg][0], kwargs_for(cfg, doc), stats[cfg]
            rs, t_rs = outcome(RUST[name], doc["html"], kw)
            py, t_py = outcome(python_function(name), doc["html"], kw)
            s["rs"] += t_rs
            s["py"] += t_py
            if rs == "EXC:Unsupported":
                s["fallback"] += 1
            elif canon(rs, False) != canon(py, False):
                s["bad"] += 1
                diffs[cfg].write(json.dumps({"i": doc["i"], "py": canon(py, True), "rs": canon(rs, True)}) + "\n")
            else:
                s["ok"] += 1
                s["tree_classes"] += canon(rs, True) != canon(py, True)
    for cfg, s in stats.items():
        diffs[cfg].close()
        n = s["ok"] + s["bad"] + s["fallback"]
        print(f"{cfg}: identical {s['ok']}/{n}, mismatches {s['bad']}, fallbacks {s['fallback']}, "
              f"same trees but other element classes {s['tree_classes']}; CPU python {s['py'] / n * 1e3:.2f} ms/doc, "
              f"rust {s['rs'] / n * 1e3:.2f} ms/doc, speedup {s['py'] / max(s['rs'], 1e-9):.1f}x")


def main():
    mode = sys.argv[1]
    if mode == "list":
        print(" ".join(CONFIGS))
    elif mode == "show":
        path, i, cfg = sys.argv[2], int(sys.argv[3]), sys.argv[4]
        doc = next(d for d in docs(path) if d["i"] == i)
        name, kw = CONFIGS[cfg][0], kwargs_for(cfg, doc)
        for side, fn in (("python", python_function(name)), ("rust", RUST[name])):
            print(f"== {side}\n{json.dumps(canon(outcome(fn, doc['html'], kw)[0], True), ensure_ascii=False)}")
    elif mode == "run":
        os.makedirs(sys.argv[3], exist_ok=True)
        run(sys.argv[2], sys.argv[3], sys.argv[4:])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
