"""`python -m trafilatura_rs script.py [args...]`: run an unmodified script with `install()` applied."""

import os
import runpy
import sys

from trafilatura_rs import install


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: python -m trafilatura_rs script.py [args...]  |  python -m trafilatura_rs -m module [args...]")
    install()
    if sys.argv[1] == "-m" and len(sys.argv) > 2:
        sys.argv = sys.argv[2:]
        runpy.run_module(sys.argv[0], run_name="__main__", alter_sys=True)
    else:
        sys.argv = sys.argv[1:]
        sys.path.insert(0, os.path.dirname(os.path.abspath(sys.argv[0])))  # as `python script.py` does
        runpy.run_path(sys.argv[0], run_name="__main__")


main()
