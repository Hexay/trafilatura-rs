//! Byte-exact Rust ports of trafilatura 1.11's text extraction and metadata (with htmldate 1.11) and of
//! trafilatura 2.3.1's text extraction.
// The ports keep Python's if/elif chains branch for branch so they can be diffed against the source.
#![allow(clippy::if_same_then_else)]
pub mod dates;
pub mod dom;
pub mod hooks;
pub mod htmldate;
pub mod justext;
pub mod py;
pub mod readability;
pub mod trafilatura;
pub mod trafilatura2;
