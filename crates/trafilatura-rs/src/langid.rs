//! py3langid 0.4.0 `LanguageIdentifier.classify` in raw-score mode (norm_probs off, no min_confidence).
//!
//! numpy scores a text as `log1p(counts as float32) @ nb_ptc[idx] + nb_pc` in float32, through a BLAS whose
//! summation order is unknown. The port computes the scores in f64 together with a bound on how far any float32
//! evaluation can be from them (any summation order, FMA or not, log1p within `LOG_ULPS` ulps), and answers only when
//! one label beats every other label by more than both bounds; otherwise the caller runs Python.
use std::collections::HashMap;
use std::sync::LazyLock;

/// Unit roundoff of float32.
const U: f64 = 1.0 / (1u64 << 24) as f64;
/// Ulps of error allowed for numpy's float32 log1p (libm's is under one).
const LOG_ULPS: f64 = 4.0;
/// Slack over the first-order error terms (their higher-order remainder and the f64 evaluation's own error).
const SLACK: f64 = 1.01;

/// float16 bit patterns as f64.
static F16: LazyLock<Vec<f64>> = LazyLock::new(|| (0..=u16::MAX).map(f16_to_f64).collect());

fn f16_to_f64(bits: u16) -> f64 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((bits >> 10) & 0x1f);
    let frac = f64::from(bits & 0x3ff);
    sign * match exp {
        0 => frac * 2f64.powi(-24),
        31 if frac == 0.0 => f64::INFINITY,
        31 => f64::NAN,
        _ => (1.0 + frac / 1024.0) * 2f64.powi(exp - 15),
    }
}

/// The identifier's arrays: `nb_ptc` (float16 bits, features × classes, row-major), `nb_pc`, `nb_classes`, and the
/// DFA (`tk_nextmove`, `tk_row`, `tk_output`).
pub struct Model {
    ptc: Vec<u16>,
    pc: Vec<f32>,
    classes: Vec<String>,
    /// For each class, the first class with its label (`_alias_pairs`).
    label_of: Vec<usize>,
    nextmove: Vec<u32>,
    row: Vec<u32>,
    output: Vec<i32>,
}

impl Model {
    /// None when the arrays don't fit together.
    pub fn new(
        ptc: Vec<u16>,
        pc: Vec<f32>,
        classes: Vec<String>,
        nextmove: Vec<u32>,
        row: Vec<u32>,
        output: Vec<i32>,
    ) -> Option<Model> {
        let n = classes.len();
        let feats = ptc.len().checked_div(n)?;
        let sound = n > 0
            && pc.len() == n
            && feats * n == ptc.len()
            && row.len() == output.len()
            && !row.is_empty()
            && row.iter().all(|&r| (r as usize + 1) * 256 <= nextmove.len())
            && nextmove.iter().all(|&s| (s as usize) < row.len())
            && output.iter().all(|&f| f < 0 || (f as usize) < feats)
            && ptc.iter().all(|&b| F16[b as usize].is_finite())
            && pc.iter().all(|p| p.is_finite());
        if !sound {
            return None;
        }
        let mut first = HashMap::new();
        let label_of = classes.iter().enumerate().map(|(i, c)| *first.entry(c.as_str()).or_insert(i)).collect();
        Some(Model { ptc, pc, classes, label_of, nextmove, row, output })
    }

    /// `visit_counts`: feature index → count from the DFA walk over `text`.
    fn visits(&self, text: &[u8]) -> HashMap<usize, u32> {
        let (mut state, mut counts) = (0usize, HashMap::new());
        for &b in text {
            state = self.nextmove[((self.row[state] as usize) << 8) + b as usize] as usize;
            if let Ok(f) = usize::try_from(self.output[state]) {
                *counts.entry(f).or_insert(0) += 1;
            }
        }
        counts
    }

    /// `classify(text)[0]` for `_encode(text)` bytes, or None where float32 rounding could change the answer.
    pub fn classify(&self, text: &[u8]) -> Option<&str> {
        let visits = self.visits(text);
        if visits.is_empty() {
            return Some(&self.classes[0]);
        }
        let n = self.classes.len();
        let (mut score, mut abs) = (vec![0f64; n], vec![0f64; n]);
        for (&f, &count) in &visits {
            if count >= 1 << 24 {
                return None;
            }
            let w = f64::from(count).ln_1p();
            for (c, &bits) in self.ptc[f * n..(f + 1) * n].iter().enumerate() {
                let t = w * F16[bits as usize];
                score[c] += t;
                abs[c] += t.abs();
            }
        }
        let m = visits.len() as f64;
        let mut best: HashMap<usize, (f64, f64)> = HashMap::new();
        for c in 0..n {
            let s = score[c] + f64::from(self.pc[c]);
            let bound = (m + 2.0 + 2.0 * LOG_ULPS) * U * SLACK * abs[c] + 2.0 * U * s.abs() + f64::MIN_POSITIVE;
            let entry = best.entry(self.label_of[c]).or_insert((f64::NEG_INFINITY, 0.0));
            if s > entry.0 {
                *entry = (s, bound);
            }
        }
        let (&win, &(s, bound)) = best.iter().max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
        let low = s - bound;
        best.iter().filter(|(&l, _)| l != win).all(|(_, &(s2, b2))| s2 + b2 < low).then(|| self.classes[win].as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{f16_to_f64, Model};

    #[test]
    fn f16_values() {
        assert_eq!(f16_to_f64(0x3c00), 1.0);
        assert_eq!(f16_to_f64(0xc000), -2.0);
        assert_eq!(f16_to_f64(0x0001), 2f64.powi(-24));
        assert_eq!(f16_to_f64(0x7bff), 65504.0);
    }

    /// Two states: byte b'a' reaches state 1, which outputs feature 0; classes "x", "y", "x" (an alias).
    fn model(ptc: [f32; 3]) -> Model {
        let bits = ptc.map(|v| {
            if v == 1.0 {
                0x3c00
            } else if v == -1.0 {
                0xbc00
            } else {
                0
            }
        });
        let mut nextmove = vec![0u32; 512];
        nextmove[b'a' as usize] = 1;
        nextmove[256 + b'a' as usize] = 1;
        let classes = ["x", "y", "x"].map(String::from).to_vec();
        Model::new(bits.to_vec(), vec![0.0; 3], classes, nextmove, vec![0, 1], vec![-1, 0]).expect("model")
    }

    #[test]
    fn classifies_with_margin() {
        assert_eq!(model([-1.0, 1.0, -1.0]).classify(b"aa"), Some("y"));
        assert_eq!(model([-1.0, -1.0, 1.0]).classify(b"a"), Some("x"));
        assert_eq!(model([1.0, 1.0, 0.0]).classify(b"a"), None);
        assert_eq!(model([1.0, 1.0, 0.0]).classify(b"bb"), Some("x"));
    }
}
