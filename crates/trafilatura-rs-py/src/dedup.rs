//! The process-wide deduplication LRU (trafilatura's module-global `LRU_TEST`). python/trafilatura_rs/_dedup.py
//! points trafilatura's `LRU_TEST` here while a call reruns in Python, so both paths update one cache.
use pyo3::prelude::*;
use std::sync::{LazyLock, Mutex, MutexGuard};
use trafilatura_rs::dom::{Error, Result};
use trafilatura_rs::trafilatura::deduplication::Lru;

static GLOBAL_LRU: LazyLock<Mutex<Lru>> = LazyLock::new(|| Mutex::new(Lru::default()));

fn global() -> MutexGuard<'static, Lru> {
    GLOBAL_LRU.lock().expect("lru lock")
}

/// `f` with the global LRU when `dedup`, else a fresh one. A call raising `Unsupported` (rerun in Python) leaves the
/// global LRU as it found it; a finished call stays undoable by `_dedup_undo` until the cache is next used.
pub(crate) fn run<T>(dedup: bool, f: impl FnOnce(&mut Lru) -> Result<T>) -> Result<T> {
    if !dedup {
        return f(&mut Lru::default());
    }
    let mut lru = global();
    lru.begin();
    let result = f(&mut lru);
    if matches!(result, Err(Error::Unsupported(_))) {
        lru.rollback();
    }
    result
}

/// Clear the module-level dedup state (`trafilatura.meta.reset_caches` equivalent for this module).
#[pyfunction]
pub fn reset_caches(py: Python<'_>) {
    py.detach(|| global().clear());
}

/// Undo the dedup updates of the last Rust call (its result is being discarded for a rerun in Python).
#[pyfunction]
pub fn _dedup_undo(py: Python<'_>) {
    py.detach(|| global().rollback());
}

/// Make the last Rust call's dedup updates final, so a later `_dedup_undo` can't reach them.
#[pyfunction]
pub fn _dedup_commit(py: Python<'_>) {
    py.detach(|| global().commit());
}

/// `LRU_TEST.get(key)`.
#[pyfunction]
pub fn _dedup_get(py: Python<'_>, key: &str) -> i64 {
    py.detach(|| {
        let mut lru = global();
        lru.commit();
        lru.get(key)
    })
}

/// `LRU_TEST.put(key, value)`.
#[pyfunction]
pub fn _dedup_put(py: Python<'_>, key: &str, value: i64) {
    py.detach(|| {
        let mut lru = global();
        lru.commit();
        lru.put(key, value);
    })
}

/// 2.x `LRU_TEST.increment(key)`.
#[pyfunction]
pub fn _dedup_increment(py: Python<'_>, key: &str) -> i64 {
    py.detach(|| {
        let mut lru = global();
        lru.commit();
        lru.increment(key)
    })
}
