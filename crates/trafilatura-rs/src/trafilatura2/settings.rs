//! trafilatura 2.3.1 settings.py constants. `Extractor` defaults (settings.cfg) are unchanged from 1.11, so
//! `Options`/`Focus` are shared (MAX_TREE_SIZE is empty, i.e. None, in both).
pub use crate::trafilatura::htmlprocessing::CUT_EMPTY_ELEMS;
pub use crate::trafilatura::settings::{Focus, Options};

pub const TAG_CATALOG: &[&str] = &["code", "del", "head", "hi", "lb", "list", "p", "quote"];

pub const MANUALLY_CLEANED: &[&str] = &[
    "aside",
    "embed",
    "fencedframe",
    "footer",
    "form",
    "head",
    "iframe",
    "menu",
    "object",
    "script",
    "applet",
    "audio",
    "canvas",
    "figure",
    "map",
    "picture",
    "svg",
    "video",
    "area",
    "blink",
    "button",
    "datalist",
    "dialog",
    "frame",
    "frameset",
    "fieldset",
    "link",
    "input",
    "label",
    "legend",
    "marquee",
    "math",
    "menuitem",
    "nav",
    "noindex",
    "noscript",
    "optgroup",
    "option",
    "output",
    "param",
    "progress",
    "rp",
    "rt",
    "rtc",
    "select",
    "source",
    "style",
    "track",
    "textarea",
    "time",
    "use",
];

pub const MANUALLY_STRIPPED: &[&str] = &[
    "abbr", "acronym", "address", "bdi", "bdo", "big", "cite", "data", "dfn", "font", "hgroup", "img", "ins", "mark",
    "meta", "nobr", "ruby", "small", "tbody", "template", "tfoot", "thead", "wbr",
];

/// Min length for a repeated span to count as an extraction artifact.
pub const MIN_DUPLICATE_LENGTH: usize = 50;
/// Substring-dedup scans stop past this many accumulated code points.
pub const DEDUPE_SCAN_CAP: usize = 200_000;

pub const INLINE_CONSUMING: &[&str] = &["hi", "ref", "del"];
pub const INLINE_FORMATTABLE: &[&str] = &["hi", "ref", "del", "code"];
pub const INLINE_CARRIED: &[&str] = &["hi", "ref", "del", "code", "graphic"];
