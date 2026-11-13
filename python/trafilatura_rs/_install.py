"""install(): make every reference to trafilatura's public functions (and htmldate's `find_date`) use trafilatura_rs.

It swaps the attribute in every loaded module holding an original function, so `from trafilatura import extract`
copies taken before install() are covered too; later imports read the patched module attributes. That includes
trafilatura's own internal calls (bare_extraction inside extract, find_date inside extract_metadata, ...): the
drop-ins hand anything they don't cover, such as lxml trees, to the original.
"""

import importlib
import sys
import warnings

from ._versions import REPRODUCES, mismatched_versions

# drop-in name -> (module defining the original, attribute)
TARGETS = {
    "extract": ("trafilatura.core", "extract"),
    "bare_extraction": ("trafilatura.core", "bare_extraction"),
    "baseline": ("trafilatura.baseline", "baseline"),
    "html2txt": ("trafilatura.baseline", "html2txt"),
    "extract_metadata": ("trafilatura.metadata", "extract_metadata"),
    "find_date": ("htmldate.core", "find_date"),
}
_originals = {}
_swapped = []


def original(name):
    """The installed Python function the drop-in `name` stands for (also after install()), or None when its package
    isn't installed."""
    if name in _originals:
        return _originals[name]
    module, attr = TARGETS[name]
    try:
        return getattr(importlib.import_module(module), attr)
    except ImportError:
        return None


def original_extract():
    """Python trafilatura's own extract (also after install()), or None when trafilatura isn't installed."""
    return original("extract")


def _replacements():
    from . import _api, _bare, _funcs

    return {"extract": _api.extract, "bare_extraction": _bare.bare_extraction, "baseline": _funcs.baseline,
            "html2txt": _funcs.html2txt, "extract_metadata": _funcs.extract_metadata, "find_date": _funcs.find_date}


def install():
    """Route trafilatura's `extract`, `bare_extraction`, `baseline`, `html2txt`, `extract_metadata` and htmldate's
    `find_date` to trafilatura_rs; returns the original `extract`. Idempotent. Does nothing (warns, returns None)
    unless the installed trafilatura and lxml are a pair in REPRODUCES."""
    if _originals:
        return _originals.get("extract")
    mismatch = mismatched_versions()
    if mismatch:
        pairs = ", ".join(f"trafilatura {t} + lxml {x}" for t, x in sorted(REPRODUCES))
        warnings.warn(f"trafilatura_rs reproduces {pairs} but found {mismatch}; trafilatura left unpatched. "
                      'Install a matching pair with: pip install "trafilatura-rs[trafilatura2]" (2.3.1) or '
                      '"trafilatura-rs[trafilatura]" (1.11.0)', RuntimeWarning, stacklevel=2)
        return None
    originals = {name: fn for name in TARGETS if (fn := original(name)) is not None}
    if not originals:
        raise ImportError("install() patches trafilatura and htmldate; neither is installed")
    replacements = _replacements()
    _originals.update(originals)
    by_id = {id(fn): replacements[name] for name, fn in originals.items()}
    for mod_name, mod in list(sys.modules.items()):
        if mod_name.split(".")[0] == "trafilatura_rs":  # our own references must keep the real functions
            continue
        try:
            names = [(k, by_id[id(v)]) for k, v in vars(mod).items() if id(v) in by_id]
        except TypeError:
            continue
        for name, replacement in names:
            _swapped.append((mod, name, getattr(mod, name)))
            setattr(mod, name, replacement)
    return _originals.get("extract")


def uninstall():
    """Undo install()."""
    while _swapped:
        mod, name, fn = _swapped.pop()
        setattr(mod, name, fn)
    _originals.clear()


def is_installed():
    return bool(_originals)
