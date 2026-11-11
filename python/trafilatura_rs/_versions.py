import os
from importlib.metadata import PackageNotFoundError, version

# The (trafilatura, lxml) pairs whose output trafilatura_rs reproduces; Python trafilatura's output depends on both.
REPRODUCES = frozenset({("1.11.0", "6.1.3"), ("2.3.1", "6.1.3")})
# The trafilatura version reproduced when trafilatura isn't installed.
DEFAULT_TRAFILATURA = "2.3.1"
# With metadata, also the ported htmldate, the version both trafilatura 1.11.0 and 2.3.1 resolve (courlan, dateparser
# and python-dateutil are called, not ported).
REPRODUCES_METADATA = {"htmldate": "1.11.0"}
# The native date shortcuts (see docs/metadata-formats-scope.md) answer for these versions only; at any other, the
# dateparser and python-dateutil calls all run in Python, so output still follows the installed packages.
REPRODUCES_DATES = {"dateparser": "1.5.0", "python-dateutil": "4.0.0"}


def _installed_version(package):
    try:
        return version(package)
    except PackageNotFoundError:
        return None


def engine():
    """The trafilatura version trafilatura_rs reproduces here: the installed one when supported, else the default."""
    installed = _installed_version("trafilatura")
    return installed if installed in {t for t, _ in REPRODUCES} else DEFAULT_TRAFILATURA


def _pair_mismatch():
    traf, lxml = _installed_version("trafilatura"), _installed_version("lxml")
    if traf is None or (traf, lxml) in REPRODUCES:
        return {}
    if traf not in {t for t, _ in REPRODUCES}:
        mismatch = {"trafilatura": traf}
        return mismatch if lxml in {x for _, x in REPRODUCES} else {**mismatch, "lxml": lxml}
    return {"lxml": lxml}


def mismatched_versions(with_metadata=False):
    """{package: installed version} for each package keeping the installed (trafilatura, lxml) pair out of REPRODUCES
    (and, with metadata, each REPRODUCES_METADATA package installed at another version). When non-empty,
    trafilatura_rs output would differ from what the installed Python trafilatura returns."""
    mismatch = _pair_mismatch()
    if with_metadata:
        installed = {pkg: _installed_version(pkg) for pkg in REPRODUCES_METADATA}
        mismatch |= {pkg: v for pkg, v in installed.items() if v is not None and v != REPRODUCES_METADATA[pkg]}
    return mismatch


def native_dates():
    """Whether the native date shortcuts may stand in for dateparser and python-dateutil (read once per process);
    TRAFILATURA_RS_NATIVE_DATES=0 turns them off."""
    if os.environ.get("TRAFILATURA_RS_NATIVE_DATES", "1") == "0":
        return False
    return all(_installed_version(pkg) == v for pkg, v in REPRODUCES_DATES.items())
