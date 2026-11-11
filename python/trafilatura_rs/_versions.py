from importlib.metadata import PackageNotFoundError, version

# The Python versions whose output trafilatura_rs reproduces; Python trafilatura's output depends on both.
REPRODUCES = {"trafilatura": "1.11.0", "lxml": "6.1.3"}
# With metadata, also the ported htmldate (courlan, dateparser and python-dateutil are called, not ported).
REPRODUCES_METADATA = {"htmldate": "1.11.0"}


def _installed_version(package):
    try:
        return version(package)
    except PackageNotFoundError:
        return None


def mismatched_versions(with_metadata=False):
    """{package: installed version} for each package in REPRODUCES (and REPRODUCES_METADATA) installed at a different
    version. When non-empty, trafilatura_rs output would differ from what the installed Python trafilatura returns."""
    pins = {**REPRODUCES, **REPRODUCES_METADATA} if with_metadata else REPRODUCES
    installed = {pkg: _installed_version(pkg) for pkg in pins}
    return {pkg: v for pkg, v in installed.items() if v is not None and v != pins[pkg]}
