//! Port of trafilatura 1.11.0 (github.com/adbar/trafilatura, tag v1.11.0), one Rust module per Python module (large ones split by
//! responsibility into `<module>_<part>`).
pub mod baseline;
mod baseline_json;
pub mod core;
pub mod deduplication;
pub mod document;
pub mod external;
pub mod htmlprocessing;
pub mod htmlprocessing_convert;
pub mod json_metadata;
mod json_metadata_authors;
pub mod main_extractor;
pub mod main_extractor_comments;
pub mod main_extractor_extract;
pub mod main_extractor_lists;
pub mod main_extractor_table;
#[rustfmt::skip]
mod meta_regexes;
pub mod metadata;
mod metadata_fields;
mod metadata_meta;
#[rustfmt::skip]
mod metadata_xpaths;
pub mod output;
pub mod settings;
mod tables;
pub mod text;
pub mod utils;
pub mod xml;
pub(crate) mod xml_unescape;
pub mod xpaths;
