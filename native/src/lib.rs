//! The Editor kit's tab, native: written in Rust, drawn by Hangar with its
//! own widgets, no page. Built to WebAssembly by scripts/build-native.sh
//! into tabs/editor.wasm, which the manifest's tab `entry` names (schema 2,
//! Hangar 2.0 or later).
//!
//! The parts are the page's, each ported from its file: `dotenv` (reading a
//! dotenv file and writing back only what changed), `answer` (the only
//! sentences an agent ever gets), `validate` (JSON, YAML, TOML), `diff`,
//! `gitignore` (git's ignore rules), `logic` (the words in en and fr, params,
//! the picker's order), `editor` (the behaviour) and `view`.
//!
//! A secret never reaches an agent or a log: the agent's text is built by
//! `answer` from names and counts, this crate logs nothing, a value shows only
//! in its own field (dotted until revealed), and no slicing here can panic,
//! since a panic's message is logged and Rust's quotes the text it cut.

pub mod answer;
pub mod diff;
pub mod dotenv;
pub mod editor;
pub mod gitignore;
pub mod js;
pub mod logic;
pub mod validate;
pub mod view;

pub use editor::{Editor, Message};

hangar_kit_sdk::export_kit!(Editor);
