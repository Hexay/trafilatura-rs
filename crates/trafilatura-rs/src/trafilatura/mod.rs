//! Port of trafilatura 1.11.0 (github.com/adbar/trafilatura, tag v1.11.0), one Rust module per Python module (large ones split by
//! responsibility into `<module>_<part>`).
pub mod baseline;
mod baseline_json;
pub mod core;
pub mod csv_output;
pub mod deduplication;
pub mod document;
pub mod external;
pub mod filters;
pub mod html_output;
pub mod htmlprocessing;
pub mod htmlprocessing_convert;
pub mod json_metadata;
pub(crate) mod json_metadata_authors;
pub mod language;
pub mod links;
pub mod main_extractor;
pub mod main_extractor_comments;
pub mod main_extractor_extract;
pub mod main_extractor_lists;
pub mod main_extractor_table;
#[rustfmt::skip]
pub(crate) mod meta_regexes;
pub mod metadata;
pub(crate) mod metadata_fields;
pub(crate) mod metadata_meta;
#[rustfmt::skip]
mod metadata_xpaths;
#[cfg(test)]
mod metadata_xpaths_test {
    #[test]
    fn metadata_xpaths_take_the_fast_path() {
        use super::metadata_xpaths::*;
        crate::dom::fastpath::assert_planned(
            [&TITLE_XPATHS[..], &AUTHOR_XPATHS, &AUTHOR_DISCARD_XPATHS, &CATEGORIES_XPATHS, &TAGS_XPATHS].concat(),
        );
    }
}
pub mod output;
pub mod settings;
mod tables;
pub mod text;
pub mod utils;
pub mod xml;
pub mod xml_output;
pub(crate) mod xml_unescape;
pub mod xpaths;
