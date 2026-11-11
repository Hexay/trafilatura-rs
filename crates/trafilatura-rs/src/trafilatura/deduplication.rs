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

/// `sample_tokens(inputstring, length=64)`.
fn sample_tokens(input: &str) -> Vec<&str> {
    const LENGTH: usize = 64;
    let tokens: Vec<&str> = crate::py::split(input)
        .map(|t| t.trim_matches(|c: char| c.is_ascii_punctuation()))
        .filter(|t| !t.is_empty() && t.chars().all(crate::py::is_alnum))
        .collect();
    let mut sample = Vec::new();
    for i in (0..=4).rev() {
        sample = tokens.iter().copied().filter(|t| crate::py::len(t) > i).collect();
        if sample.len() * 2 >= LENGTH {
            return sample;
        }
    }
    sample
}

/// `content_fingerprint`: hex of a 64-bit Charikar simhash over blake2b-8 token hashes (`Simhash.to_hex`).
pub fn content_fingerprint(content: &str) -> String {
    simhash_hex(&sample_tokens(content))
}

/// `Simhash(...).to_hex()` over already sampled tokens.
pub fn simhash_hex(tokens: &[&str]) -> String {
    use blake2::digest::consts::U8;
    use blake2::{Blake2b, Digest};
    let mut vector = [0i64; 64];
    for token in tokens {
        let digest = Blake2b::<U8>::digest(token.as_bytes());
        let hash = u64::from_be_bytes(digest.as_slice().try_into().expect("8 bytes"));
        for (i, v) in vector.iter_mut().enumerate() {
            *v += if hash & (1 << i) != 0 { 1 } else { -1 };
        }
    }
    let hash = vector.iter().enumerate().filter(|(_, v)| **v >= 0).fold(0u64, |h, (i, _)| h | 1 << i);
    format!("{hash:x}")
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
