//! trafilatura/deduplication.py: the LRU used by `duplicate_test` (Python keeps it module-global; here it lives in
//! the caller's `Extractor` so the per-worker, document-order state is explicit).
use super::settings::Options;
use super::text::trim;
use crate::dom::Node;
use std::collections::{BTreeMap, HashMap};

pub const LRU_SIZE: usize = 4096;

/// `LRUCache`: plain least-recently-used eviction once `maxsize` keys are held.
#[derive(Default)]
pub struct Lru {
    map: HashMap<String, (i64, u64)>,
    order: BTreeMap<u64, String>,
    tick: u64,
}

impl Lru {
    fn touch(&mut self, key: &str) -> Option<i64> {
        let (value, old) = *self.map.get(key)?;
        self.order.remove(&old);
        self.tick += 1;
        self.order.insert(self.tick, key.to_owned());
        self.map.insert(key.to_owned(), (value, self.tick));
        Some(value)
    }

    /// `get`: the stored count, or -1.
    pub fn get(&mut self, key: &str) -> i64 {
        self.touch(key).unwrap_or(-1)
    }

    pub fn put(&mut self, key: &str, value: i64) {
        if self.touch(key).is_some() {
            self.map.get_mut(key).expect("present").0 = value;
            return;
        }
        if self.map.len() >= LRU_SIZE {
            let (_, oldest) = self.order.pop_first().expect("non-empty when full");
            self.map.remove(&oldest);
        }
        self.tick += 1;
        self.order.insert(self.tick, key.to_owned());
        self.map.insert(key.to_owned(), (value, self.tick));
    }

    pub fn clear(&mut self) {
        *self = Lru::default();
    }
}

fn put_in_cache(lru: &mut Lru, key: &str) {
    let cached = lru.get(key);
    lru.put(key, if cached != -1 { cached + 1 } else { 1 });
}

/// `duplicate_test`.
pub fn duplicate_test(el: Node, options: &Options, lru: &mut Lru) -> bool {
    let test = trim(&el.itertext(true).join(" "));
    if crate::py::len(&test) > options.min_duplcheck_size {
        let cached = lru.get(&test);
        if cached > options.max_repetitions as i64 {
            lru.put(&test, cached + 1);
            return true;
        }
    }
    put_in_cache(lru, &test);
    false
}
