"""Imported at interpreter startup by `trafilatura_rs.pth` when TRAFILATURA_RS is set (and not "0").

Runs install() right after trafilatura is first imported, so processes that never use trafilatura don't pay for it.
"""

import sys
from importlib.abc import MetaPathFinder
from importlib.util import find_spec


def _install():
    from trafilatura_rs import install

    install()


class _InstallAfterImport(MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name != "trafilatura":
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


if "trafilatura" in sys.modules:
    _install()
else:
    sys.meta_path.insert(0, _InstallAfterImport())
