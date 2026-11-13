"""Imported at interpreter startup by `trafilatura_rs.pth` when TRAFILATURA_RS is set (and not "0").

Runs install() right after trafilatura or htmldate is first imported, so processes that use neither don't pay for it.
"""

import sys
from importlib.abc import MetaPathFinder
from importlib.util import find_spec

_WATCHED = ("trafilatura", "htmldate")


def _install():
    from trafilatura_rs import install

    install()


class _InstallAfterImport(MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name not in _WATCHED:
            return None
        sys.meta_path.remove(self)
        spec = find_spec(name)
        if spec is None or spec.loader is None:
            return spec
        exec_module = spec.loader.exec_module

        def exec_then_install(module):
            exec_module(module)
            _install()

        spec.loader.exec_module = exec_then_install
        return spec


if any(name in sys.modules for name in _WATCHED):
    _install()
else:
    sys.meta_path.insert(0, _InstallAfterImport())
