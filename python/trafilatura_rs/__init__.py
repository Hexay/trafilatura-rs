"""Byte-exact Rust port of trafilatura 1.11's and 2.3's text extraction and metadata.

`extract()` is a drop-in for `trafilatura.extract`: calls covered by `verified_outputs()` run in Rust, everything else
runs Python trafilatura. `bare_extraction`, `baseline`, `html2txt`, `extract_metadata` and htmldate's `find_date` are
drop-ins on the same terms (`supports_*` tell which calls run in Rust). `install()` (or `TRAFILATURA_RS=1`, or
`python -m trafilatura_rs script.py`) routes existing calls to all of them through trafilatura_rs. The engine follows
the installed trafilatura: output is byte-identical to each (trafilatura, lxml) pair in `REPRODUCES` (2.3.1 when
trafilatura isn't installed), plus `REPRODUCES_METADATA` for metadata; with other versions installed, those calls stay
on Python.
"""

from ._api import VERIFIED, VERIFIED_OUTPUTS, Extractor, extract, supports, verified_outputs
from ._bare import VERIFIED_BARE, bare_extraction, supports_bare_extraction
from ._core import reset_caches
from ._funcs import (baseline, extract_metadata, find_date, html2txt, supports_baseline, supports_extract_metadata,
                     supports_find_date, supports_html2txt)
from ._install import install, is_installed, uninstall
from ._versions import REPRODUCES, REPRODUCES_METADATA, engine, mismatched_versions

__all__ = ["Extractor", "REPRODUCES", "REPRODUCES_METADATA", "VERIFIED", "VERIFIED_BARE", "VERIFIED_OUTPUTS",
           "bare_extraction", "baseline", "engine", "extract", "extract_metadata", "find_date", "html2txt", "install",
           "is_installed", "mismatched_versions", "reset_caches", "supports", "supports_bare_extraction",
           "supports_baseline", "supports_extract_metadata", "supports_find_date", "supports_html2txt", "uninstall",
           "verified_outputs"]
