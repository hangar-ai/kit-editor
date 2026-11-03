//! The only text this tab ever sends to an agent. Every sentence is fixed
//! here; what varies is the file's path, key names and counts. It is handed
//! names, never a value: `values` is only there so that a name that is also a
//! value stays unsaid. answer.js, ported.

use std::collections::HashSet;

use crate::js;

const NAME_MAX: usize = 64;
const NAMED_MAX: usize = 40;
/// Shorter values match names by chance: `1` in `KEY1`.
const SHORTEST_INSIDE: usize = 4;
const STAY: &str = "The values stay in the file: do not read them back, use them through the program that loads it.";

/// `[A-Za-z_][A-Za-z0-9_.-]{0,63}`.
fn looks_like_a_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=NAME_MAX).contains(&bytes.len())
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// A value pasted where a key goes (`sk_live_abc=`, or a value that repeats as
/// another line's key) is still a key to any parser. So a name is said only if
/// it looks like one, is one this file or the agent already knew, and holds no
/// value.
pub fn may_name(name: &str, known: &HashSet<String>, values: &[String]) -> bool {
    if !looks_like_a_name(name) || !known.contains(name) {
        return false;
    }
    !values.iter().any(|raw| {
        let value = js::trim(raw);
        !value.is_empty()
            && (value == name || (js::length(value) >= SHORTEST_INSIDE && name.contains(value)))
    })
}

/// The names that may be said, and how many may not.
fn sayable(names: &[String], known: &HashSet<String>, values: &[String]) -> (Vec<String>, usize) {
    let mut named = Vec::new();
    let mut others = 0;
    for name in names {
        if named.len() < NAMED_MAX && may_name(name, known, values) {
            named.push(name.clone());
        } else {
            others += 1;
        }
    }
    (named, others)
}

fn sentence(label: &str, names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!(" {label}: {}.", names.join(", "))
    }
}

fn others(count: usize, verb: &str) -> String {
    match count {
        0 => String::new(),
        1 => format!(" 1 other key {verb}."),
        _ => format!(" {count} other keys {verb}."),
    }
}

/// What a sentence about a dotenv file is told: names, and the values only
/// to stay silent about them.
pub struct Facts<'a> {
    pub path: &'a str,
    pub known: &'a HashSet<String>,
    pub values: &'a [String],
}

/// The person saved a dotenv file.
pub fn dotenv_saved(
    facts: &Facts,
    set: &[String],
    emptied: &[String],
    still_empty: &[String],
) -> String {
    let (set, set_others) = sayable(set, facts.known, facts.values);
    let (emptied, emptied_others) = sayable(emptied, facts.known, facts.values);
    let (empty, empty_others) = sayable(still_empty, facts.known, facts.values);
    format!(
        "The person saved `{}`.{}{}{}{} {STAY}",
        facts.path,
        sentence("Set", &set),
        sentence("Emptied", &emptied),
        sentence("Still empty", &empty),
        others(
            set_others + emptied_others + empty_others,
            "changed or empty"
        ),
    )
}

/// The person looked at a dotenv file and had nothing to change.
pub fn dotenv_checked(facts: &Facts, already_set: &[String], still_empty: &[String]) -> String {
    let (set, set_others) = sayable(already_set, facts.known, facts.values);
    let (empty, empty_others) = sayable(still_empty, facts.known, facts.values);
    format!(
        "The person checked `{}` and changed nothing.{}{}{} {STAY}",
        facts.path,
        sentence("Already set", &set),
        sentence("Still empty", &empty),
        others(set_others + empty_others, "set or empty"),
    )
}

pub fn text_saved(path: &str) -> String {
    format!("The person saved `{path}`: the file changed.")
}

pub fn text_checked(path: &str) -> String {
    format!("The person checked `{path}` and changed nothing.")
}
