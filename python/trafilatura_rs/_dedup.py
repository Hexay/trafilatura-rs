"""One deduplication cache for both paths. trafilatura keeps a process-global LRU (`trafilatura.deduplication.LRU_TEST`);
trafilatura_rs keeps its own in Rust. A deduplicating call that reruns in Python (a document the port declines, or
any call it doesn't cover) runs with `LRU_TEST` pointed at the Rust cache, and a Rust call whose result is discarded
for such a rerun first undoes its own updates (`_core._dedup_undo`; `Unsupported` raised in Rust already left the
cache untouched). So the cache sees every document once, in call order, whichever path ran it, as in Python.
`lock()` serializes deduplicating calls so an undo or a swap never interleaves with another call."""
import contextlib
import importlib
import threading

from . import _core

_LOCK = threading.RLock()


class SharedCache:
    """trafilatura's `LRUCache` interface (1.11: get/put, 2.x: increment) over the Rust cache."""

    def __bool__(self):
        return True

    def get(self, key):
        return _core._dedup_get(key)

    def put(self, key, value):
        _core._dedup_put(key, value)

    def increment(self, key):
        return _core._dedup_increment(key)

    def clear(self):
        _core.reset_caches()


_SHARED = SharedCache()


def requested(kwargs):
    """Whether extract() or bare_extraction() with these arguments deduplicates (directly or through `options=`)."""
    return bool(kwargs.get("deduplicate") or getattr(kwargs.get("options"), "dedup", False))


def lock(dedup):
    """The dedup lock when `dedup`, else a no-op context."""
    return _LOCK if dedup else contextlib.nullcontext()


@contextlib.contextmanager
def python_uses_rust_cache(dedup):
    """When `dedup`, point trafilatura's LRU_TEST at the Rust cache for the duration (under the dedup lock)."""
    try:
        module = importlib.import_module("trafilatura.deduplication") if dedup else None
    except ImportError:
        module = None
    if module is None:
        yield
        return
    with _LOCK:
        saved, module.LRU_TEST = module.LRU_TEST, _SHARED
        try:
            yield
        finally:
            module.LRU_TEST = saved


@contextlib.contextmanager
def undo_on_error(dedup):
    """Around a Rust call and the Python checks of its result: on an exception, undo that call's dedup updates (the
    caller reruns it in Python). Use under the dedup lock."""
    if not dedup:
        yield
        return
    _core._dedup_commit()  # an earlier call's updates must stay, even if this one fails before reaching the cache
    try:
        yield
    except BaseException:
        _core._dedup_undo()
        raise
