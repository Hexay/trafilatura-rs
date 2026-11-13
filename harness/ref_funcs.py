"""Parity and CPU time of the drop-ins besides extract(): the installed Python function vs trafilatura_rs's Rust path
(no fallback), document by document (tools/make_reference.sh funcs:<cfg>, tools/parity.sh funcs:<cfg>).
  ref_funcs.py ref docs.jsonl out.jsonl <cfg>        Python outputs and CPU time per doc (the reference)
  ref_funcs.py rust docs.jsonl out.jsonl <cfg>       the same through the Rust path
  ref_funcs.py compare ref.jsonl rust.jsonl [bad]    mismatches, fallbacks and CPU per side; bad gets mismatching ids
  ref_funcs.py show docs.jsonl <i> <cfg>             print both results of one doc
  ref_funcs.py list                                  config names
Results compare by value; lxml trees by etree.tostring (their element classes are reported apart). Exceptions compare
as "EXC:<type>"; a Rust `Unsupported` (the drop-in reruns Python) counts as a fallback, not a mismatch. Rust CPU
time depends on the extension's build profile: speedups need `PROFILE=release tools/dev.sh py`.
The calls keep their default, wall-clock `max_date`; dateutil's default for missing fields is pinned (as in
ref_meta.py), and a `filedate` equal to the run's day is recorded as "<today>", so a reference stays valid on later days."""
import json
import sys
import time
from datetime import date

import trafilatura_rs
from ref_meta import pin_wall_clock
from lxml import etree
from trafilatura_rs import _bare, _core, _funcs

V1 = trafilatura_rs.engine() == "1.11.0"
FAST = "no_fallback" if V1 else "fast"
_FOCUS = {"precision": {"favor_precision": True}, "balanced": {}, "recall": {"favor_recall": True}, "fast": {FAST: True}}
UNSUPPORTED = {"str": "EXC:Unsupported"}


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


def _slots(d, classes):
    today = date.today().isoformat()
    return {k: {"str": "<today>"} if k == "filedate" and v == today else canon(v, classes) for k, v in d.items()}


def canon(x, classes):
    """A JSON-able form of a result; trees by tostring, their element classes appended to `classes`."""
    if isinstance(x, etree._Element):
        classes.append(sorted({type(e).__name__ for e in x.iter()}))
        return {"tree": etree.tostring(x, encoding="unicode")}
    if hasattr(x, "__slots__") and hasattr(x, "as_dict"):
        return {"Document": _slots(x.as_dict(), classes)}
    if isinstance(x, dict):
        return {"dict": _slots(x, classes)}
    if isinstance(x, (list, tuple)):
        return {type(x).__name__: [canon(v, classes) for v in x]}
    return {type(x).__name__: x}


def outcome(fn, html, kw):
    """{"out", "classes", "t"} of `fn(html, **kw)`."""
    start = time.process_time()
    try:
        result = fn(html, **kw)
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        result = f"EXC:{type(e).__name__}"
    t = time.process_time() - start
    classes = []
    return {"out": canon(result, classes), "classes": classes, "t": t}


def kwargs_for(cfg, doc):
    return {k: (doc["url"] if v is True and k in ("url", "default_url") else v) for k, v in CONFIGS[cfg][1].items()}


def docs(path):
    with open(path, encoding="utf-8") as f:
        yield from map(json.loads, f)


def write(path, cfg, side):
    name = CONFIGS[cfg][0]
    fn = python_function(name) if side == "ref" else RUST[name]
    with open(path, "w", encoding="utf-8") as f:
        for doc in docs(sys.argv[2]):
            f.write(json.dumps({"i": doc["i"], **outcome(fn, doc["html"], kwargs_for(cfg, doc))}) + "\n")


def compare(ref_path, rust_path, bad_path):
    ref = {r["i"]: r for r in docs(ref_path)}
    rust = {r["i"]: r for r in docs(rust_path)}
    common = sorted(ref.keys() & rust.keys())
    fallbacks = [i for i in common if rust[i]["out"] == UNSUPPORTED]
    bad = [i for i in common if rust[i]["out"] not in (UNSUPPORTED, ref[i]["out"])]
    classes = sum(ref[i]["out"] == rust[i]["out"] and ref[i]["classes"] != rust[i]["classes"] for i in common)
    py, rs, n = sum(ref[i]["t"] for i in common), sum(rust[i]["t"] for i in common), max(len(common), 1)
    print(f"docs: ref={len(ref)} rust={len(rust)} compared={len(common)}")
    print(f"identical {len(common) - len(bad) - len(fallbacks)}, mismatches {len(bad)} first={bad[:15]}, fallbacks "
          f"{len(fallbacks)}, same trees but other element classes {classes}; CPU python {py / n * 1e3:.2f} ms/doc, "
          f"rust {rs / n * 1e3:.2f} ms/doc, speedup {py / max(rs, 1e-9):.1f}x")
    if bad_path:
        with open(bad_path, "w", encoding="utf-8") as f:
            f.writelines(f"{i}\n" for i in bad)


def main():
    mode = sys.argv[1]
    if mode == "list":
        print(" ".join(CONFIGS))
    elif mode in ("ref", "rust"):
        pin_wall_clock()
        write(sys.argv[3], sys.argv[4], mode)
    elif mode == "compare":
        compare(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)
    elif mode == "show":
        pin_wall_clock()
        path, i, cfg = sys.argv[2], int(sys.argv[3]), sys.argv[4]
        doc = next(d for d in docs(path) if d["i"] == i)
        name, kw = CONFIGS[cfg][0], kwargs_for(cfg, doc)
        for side, fn in (("python", python_function(name)), ("rust", RUST[name])):
            print(f"== {side}\n{json.dumps(outcome(fn, doc['html'], kw), ensure_ascii=False)}")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
