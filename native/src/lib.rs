//! The Editor kit's tab, native: what tabs/editor/ does in a page, in Rust.
//!
//! The parts that are neither Hangar nor the page's behaviour, each ported
//! from its file: `dotenv` (reading a dotenv file and writing back only what
//! changed), `answer` (the only sentences an agent ever gets), `validate`
//! (JSON, YAML, TOML), `diff`, `gitignore` (git's ignore rules) and `logic`
//! (the words in en and fr, params, the picker's order).
//!
//! No slicing here can panic: a panic's message is logged, and Rust's for a
//! cut inside a character quotes the text it cut, which may be a value.

pub mod answer;
pub mod diff;
pub mod dotenv;
pub mod gitignore;
pub mod js;
pub mod logic;
pub mod validate;
