//! Byte-exact Rust ports of trafilatura 1.11's and 2.2.0's text extraction.
// The ports keep Python's if/elif chains branch for branch so they can be diffed against the source.
#![allow(clippy::if_same_then_else)]
pub mod dom;
pub mod justext;
pub mod py;
pub mod readability;
pub mod trafilatura;
pub mod trafilatura2;
