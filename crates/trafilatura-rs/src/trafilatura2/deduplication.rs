//! trafilatura 2.3.1 deduplication.py `content_fingerprint` (the `Simhash` is unchanged; token sampling is not).
use crate::py;
use crate::trafilatura::deduplication::simhash_hex;

/// `sample_tokens(inputstring, length=64)`.
fn sample_tokens(input: &str) -> Vec<String> {
    const LENGTH: usize = 64;
    let mut tokens: Vec<String> = py::split(input)
        .map(|t| t.trim_matches(|c: char| c.is_ascii_punctuation()))
        .filter(|t| !t.is_empty() && t.chars().all(py::is_alnum))
        .map(str::to_owned)
        .collect();
    if tokens.is_empty() {
        let spaced: String = input.chars().map(|c| if py::is_punctuation(c) { ' ' } else { c }).collect();
        tokens = py::split(&spaced).filter(|t| t.chars().all(py::is_alnum)).map(str::to_owned).collect();
    }
    for i in (1..=4).rev() {
        let sample: Vec<String> = tokens.iter().filter(|t| py::len(t) > i).cloned().collect();
        if sample.len() * 2 >= LENGTH {
            return sample;
        }
    }
    tokens
}

/// `content_fingerprint`.
pub fn content_fingerprint(content: &str) -> String {
    let tokens = sample_tokens(content);
    simhash_hex(&tokens.iter().map(String::as_str).collect::<Vec<_>>())
}
