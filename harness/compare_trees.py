"""extract()/bare_extraction() given lxml trees: the installed Python trafilatura vs trafilatura_rs, in one process,
each on its own tree built the same way from every record.
  compare_trees.py docs.jsonl <cfg> <source> [bad]   identical/mismatching outputs, fallbacks, caller trees left in
                                                     another state than Python leaves them; bad gets mismatching ids
  compare_trees.py list                              configs and sources
cfg: a ref_stages.py final config (fineweb, balanced, ...), a meta_configs.py one (balanced_json, ...) or a
ref_funcs.py bare_* one. A fallback is a call trafilatura_rs reran in Python (1.11.0: every tree call). Outputs
compare as in ref_funcs.py (trees by tostring, exceptions by type). CPU times are indicative; speedups come from
tools/bench.sh."""
import json
import sys
import time

import lxml.html
import trafilatura
import trafilatura_rs
from lxml import etree
from trafilatura import deduplication as dedup
from meta_configs import CONFIGS as META, kwargs_for as meta_kwargs
from ref_funcs import CONFIGS as FUNCS, canon, kwargs_for as funcs_kwargs, python_function
from ref_meta import pin_wall_clock
from ref_stages import CONFIGS as FINAL
from trafilatura_rs import _api, _bare

SOURCES = {
    "fromstring": lxml.html.fromstring,
    "document": lxml.html.document_fromstring,
    "load_html": trafilatura.load_html,
    "etree_html": lambda html: etree.fromstring(html, etree.HTMLParser()),
    "elementtree": lambda html: lxml.html.fromstring(html).getroottree(),
}
RUST_CALLS = []


def _counting(module, name):
    """Wrap `module.name` to record each call that returned (Rust answered, no fallback)."""
    fn = getattr(module, name)

    def run(*args, **kwargs):
        result = fn(*args, **kwargs)
        RUST_CALLS.append(1)
        return result

    setattr(module, name, run)


def calls(cfg):
    """(python function, trafilatura_rs function, doc -> keywords) for config `cfg`."""
    if cfg in FUNCS:
        name = FUNCS[cfg][0]
        return python_function(name), getattr(trafilatura_rs, name), lambda doc: funcs_kwargs(cfg, doc)
    if cfg in FINAL:
        kw = {("fast" if k == "no_fallback" and trafilatura_rs.engine() != "1.11.0" else k): v
              for k, v in FINAL[cfg].items()}
        return python_function("extract"), trafilatura_rs.extract, lambda doc: kw
    return python_function("extract"), trafilatura_rs.extract, lambda doc: meta_kwargs(cfg, doc)[1]


def dedup_caches():
    """One trafilatura `LRU_TEST` per side: on fallback both sides run Python, which would share the global one."""
    return {side: type(dedup.LRU_TEST)(maxsize=dedup.LRU_SIZE) for side in ("python", "rust")}


def outcome(fn, tree, kw, lru):
    dedup.LRU_TEST = lru
    start = time.process_time()
    try:
        result = fn(tree, **kw)
    except Exception as e:  # noqa: BLE001 - the exception type is part of the compared output
        result = f"EXC:{type(e).__name__}"
    return canon(result, []), time.process_time() - start


def tostring(tree):
    return etree.tostring(tree, encoding="unicode")


def main():
    if sys.argv[1] == "list":
        print("configs:", *FINAL, *META, *(c for c in FUNCS if c.startswith("bare")), "\nsources:", *SOURCES)
        return
    path, cfg, source = sys.argv[1:4]
    pin_wall_clock()
    _counting(_api, "_rust_extract")
    _counting(_bare, "run_rust")
    python, rust, kwargs = calls(cfg)
    make = SOURCES[source]
    n = skipped = fallbacks = changed = 0
    bad, tree_bad, t_py, t_rs = [], [], 0.0, 0.0
    lru = dedup_caches()
    with open(path, encoding="utf-8") as f:
        for line in f:
            doc = json.loads(line)
            try:
                py_tree, rs_tree = make(doc["html"]), make(doc["html"])
            except Exception:  # noqa: BLE001 - no tree to test
                py_tree = None
            if py_tree is None:
                skipped += 1
                continue
            n += 1
            kw, before = kwargs(doc), tostring(rs_tree)
            py_out, dt = outcome(python, py_tree, kw, lru["python"])
            t_py += dt
            del RUST_CALLS[:]
            rs_out, dt = outcome(rust, rs_tree, kw, lru["rust"])
            t_rs += dt
            fallbacks += not RUST_CALLS
            if py_out != rs_out:
                bad.append(doc["i"])
            if tostring(py_tree) != tostring(rs_tree):
                tree_bad.append(doc["i"])
            changed += tostring(rs_tree) != before
    print(f"{trafilatura.__version__} {cfg} {source}: trees {n} (no tree: {skipped}), identical {n - len(bad)}, "
          f"mismatches {len(bad)} first={bad[:10]}, fallbacks {fallbacks}, caller tree differs from Python's "
          f"{len(tree_bad)} first={tree_bad[:10]}, caller tree changed {changed}; CPU python "
          f"{t_py / max(n, 1) * 1e3:.2f} ms/doc, trafilatura_rs {t_rs / max(n, 1) * 1e3:.2f} ms/doc")
    if len(sys.argv) > 4:
        with open(sys.argv[4], "w", encoding="utf-8") as f:
            f.writelines(f"{i}\n" for i in bad + tree_bad)


if __name__ == "__main__":
    main()
