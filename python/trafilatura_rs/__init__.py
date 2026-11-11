"""Byte-exact Rust port of trafilatura 1.11's text extraction.

`extract()` is a drop-in for `trafilatura.extract`: calls covered by `VERIFIED_OUTPUTS` (txt, markdown and json
output, optionally with metadata) run in Rust, everything else runs Python trafilatura. `install()` (or
`TRAFILATURA_RS=1`, or `python -m trafilatura_rs script.py`) routes existing `trafilatura.extract` calls through it.
Output is byte-identical to trafilatura 1.11.0 + lxml 6.1.3 (`REPRODUCES`, plus `REPRODUCES_METADATA` for
metadata); with other versions installed, those calls stay on Python trafilatura.
"""

from ._api import VERIFIED, VERIFIED_OUTPUTS, extract, supports
from ._core import Extractor, reset_caches
from ._install import install, is_installed, uninstall
from ._versions import REPRODUCES, REPRODUCES_METADATA, mismatched_versions

__all__ = ["Extractor", "REPRODUCES", "REPRODUCES_METADATA", "VERIFIED", "VERIFIED_OUTPUTS", "extract", "install",
           "is_installed", "mismatched_versions", "reset_caches", "supports", "uninstall"]
