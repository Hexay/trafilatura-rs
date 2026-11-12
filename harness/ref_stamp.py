"""Print the Python and package versions a reference depends on, one line (tools/make_reference.sh <ref>.stamp)."""
import platform
from importlib.metadata import PackageNotFoundError, version

PACKAGES = ["trafilatura", "lxml", "lxml-html-clean", "justext", "courlan", "htmldate", "dateparser",
            "python-dateutil", "charset-normalizer", "regex"]


def main():
    parts = [f"python={platform.python_version()}"]
    for p in PACKAGES:
        try:
            parts.append(f"{p}={version(p)}")
        except PackageNotFoundError:
            pass
    print(" ".join(parts))


if __name__ == "__main__":
    main()
