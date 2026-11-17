"""Whether the native py3langid classify (trafilatura_rs::langid) may stand in for trafilatura's
`language_classifier`: py3langid at REPRODUCES_LANGID, unpatched, with its identifier in raw-score mode."""

import os
import sys

from ._versions import REPRODUCES_LANGID, _installed_version

_STOCK_METHODS = ("classify", "_decide", "_raw_score", "_sparse_score", "_encode")


def _is(function, module, qualname):
    return getattr(function, "__module__", None) == module and getattr(function, "__qualname__", None) == qualname


def _stock_identifier(langid):
    cls = getattr(langid, "LanguageIdentifier", None)
    if not _is(cls, "py3langid.langid", "LanguageIdentifier"):
        return None
    if not all(_is(getattr(cls, m, None), "py3langid.langid", f"LanguageIdentifier.{m}") for m in _STOCK_METHODS):
        return None
    if not (_is(langid.classify, "py3langid.langid", "classify") and _is(langid.visit_counts, "py3langid.langid",
                                                                          "visit_counts")):
        return None
    ident = langid._get_identifier()
    if type(ident) is not cls or ident._norm_probs or ident.min_confidence is not None:
        return None
    return ident


def model():
    """(nb_ptc, nb_pc, nb_classes, tk_nextmove, tk_row, tk_output) of py3langid's identifier when trafilatura's
    `language_classifier` would run its stock `classify`, else None. TRAFILATURA_RS_NATIVE_LANGID=0 turns it off."""
    if os.environ.get("TRAFILATURA_RS_NATIVE_LANGID", "1") == "0":
        return None
    if _installed_version("py3langid") != REPRODUCES_LANGID:
        return None
    utils, langid = sys.modules.get("trafilatura.utils"), sys.modules.get("py3langid.langid")
    if utils is None or langid is None or utils.LANGID_FLAG is not True:
        return None
    if not _is(utils.language_classifier, "trafilatura.utils", "language_classifier"):
        return None
    if getattr(utils, "py3langid", None) is not sys.modules.get("py3langid"):
        return None
    if utils.py3langid.classify is not langid.classify:
        return None
    ident = _stock_identifier(langid)
    if ident is None:
        return None
    ptc, pc = ident.nb_ptc, ident.nb_pc
    if str(ptc.dtype) != "float16" or ptc.ndim != 2 or str(pc.dtype) != "float32" or pc.ndim != 1:
        return None
    if not ptc.shape[1] == len(pc) == len(ident.nb_classes):
        return None
    return ptc, pc, ident.nb_classes, ident.tk_nextmove, ident.tk_row, ident.tk_output
