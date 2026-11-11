//! Port of trafilatura 2.2.0 (github.com/adbar/trafilatura, tag v2.2.0), one Rust module per Python module (large
//! ones split as `<module>_<part>`). Code whose behaviour is unchanged since 1.11 is called from `crate::trafilatura`;
//! see docs/trafilatura-2x-diff.md for what changed.
pub mod baseline;
mod baseline_json;
pub mod core;
pub mod external;
pub mod htmlprocessing;
pub mod htmlprocessing_convert;
mod json;
pub mod main_extractor;
pub mod main_extractor_extract;
mod main_extractor_lists;
pub mod main_extractor_table;
pub mod settings;
pub mod utils;
pub mod xml;
mod xml_markdown;
mod xml_text;
pub mod xpaths;
