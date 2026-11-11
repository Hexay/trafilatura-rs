"""install(): make every reference to `trafilatura.extract` use `trafilatura_rs.extract`.

It swaps the attribute in every loaded module holding the original function, so `from trafilatura import extract`
copies taken before install() are covered too; later imports read the patched `trafilatura.extract`.
"""

import sys
import warnings

from ._versions import REPRODUCES, mismatched_versions

_original = None
_swapped = []


def original_extract():
    """Python trafilatura's own extract (also after install()), or None when trafilatura isn't installed."""
    if _original is not None:
        return _original
    try:
        import trafilatura.core
    except ImportError:
        return None
    return trafilatura.core.extract


def install():
    """Route `trafilatura.extract` to trafilatura_rs; returns the original function. Idempotent. Does nothing (warns,
    returns None) unless the installed trafilatura and lxml are a pair in REPRODUCES."""
    global _original
    if _original is not None:
        return _original
    import trafilatura.core

    mismatch = mismatched_versions()
    if mismatch:
        pairs = ", ".join(f"trafilatura {t} + lxml {x}" for t, x in sorted(REPRODUCES))
        warnings.warn(f"trafilatura_rs reproduces {pairs} but found {mismatch}; trafilatura.extract left unpatched. "
                      'Install a matching pair with: pip install "trafilatura-rs[trafilatura2]" (2.3.1) or '
                      '"trafilatura-rs[trafilatura]" (1.11.0)', RuntimeWarning, stacklevel=2)
        return None
    from ._api import extract

    _original = trafilatura.core.extract
    for mod_name, mod in list(sys.modules.items()):
        if mod_name.split(".")[0] == "trafilatura_rs":  # our own `_original` must keep the real function
            continue
        try:
            names = [k for k, v in vars(mod).items() if v is _original]
        except TypeError:
            continue
        for name in names:
            setattr(mod, name, extract)
            _swapped.append((mod, name))
    return _original


def uninstall():
    """Undo install()."""
    global _original
    while _swapped:
        mod, name = _swapped.pop()
        setattr(mod, name, _original)
    _original = None


def is_installed():
    return _original is not None
