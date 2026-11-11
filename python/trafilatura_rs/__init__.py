"""Byte-exact Rust port of trafilatura 1.11's and 2.3's text extraction (and 1.11's metadata).

`extract()` is a drop-in for `trafilatura.extract`: calls covered by `verified_outputs()` run in Rust, everything else
runs Python trafilatura. `install()` (or `TRAFILATURA_RS=1`, or `python -m trafilatura_rs script.py`) routes existing
`trafilatura.extract` calls through it. The engine follows the installed trafilatura: output is byte-identical to each
(trafilatura, lxml) pair in `REPRODUCES` (2.3.1 when trafilatura isn't installed), plus `REPRODUCES_METADATA` for
metadata; with other versions installed, those calls stay on Python trafilatura.
"""

from ._api import VERIFIED, VERIFIED_OUTPUTS, Extractor, extract, supports, verified_outputs
from ._core import reset_caches
from ._install import install, is_installed, uninstall
from ._versions import REPRODUCES, REPRODUCES_METADATA, engine, mismatched_versions

__all__ = ["Extractor", "REPRODUCES", "REPRODUCES_METADATA", "VERIFIED", "VERIFIED_OUTPUTS", "engine", "extract",
           "install", "is_installed", "mismatched_versions", "reset_caches", "supports", "uninstall",
           "verified_outputs"]
