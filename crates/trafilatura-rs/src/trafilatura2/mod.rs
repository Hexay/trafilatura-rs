//! Port of trafilatura 2.3.1 (github.com/adbar/trafilatura, tag v2.3.1), one Rust module per Python module (large
//! ones split as `<module>_<part>`). Code whose behaviour is unchanged since 1.11 is called from
//! `crate::trafilatura`; see docs/trafilatura-2x-diff.md and docs/trafilatura-23-diff.md for what changed.
pub mod baseline;
mod baseline_json;
pub mod core;
pub mod deduplication;
pub mod external;
pub mod htmlprocessing;
pub mod htmlprocessing_convert;
mod json;
pub mod main_extractor;
pub mod main_extractor_comments;
pub mod main_extractor_extract;
mod main_extractor_lists;
pub mod main_extractor_table;
#[rustfmt::skip]
mod meta_regexes;
pub mod json_metadata;
mod json_metadata_authors;
pub mod metadata;
mod metadata_fields;
mod metadata_meta;
#[rustfmt::skip]
mod metadata_xpaths;
pub mod output;
pub mod settings;
pub mod utils;
pub mod xml;
mod xml_md;
mod xml_output;
mod xml_text;
pub mod xpaths;
