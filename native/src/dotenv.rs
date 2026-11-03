//! Reads a dotenv file so that saving it changes only what was edited: every
//! line keeps its exact bytes (comments, blanks, order, line endings, quotes)
//! unless its own value changed. dotenv.js, ported: no SDK, no Hangar.

use std::collections::{HashMap, HashSet};

use crate::js::{self, cut, from};

const HINT_MAX: usize = 300;

/// A line of the file, or several when a quoted value spans them.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Pair(Pair),
    Other(Other),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub key: String,
    /// What parsers read: unquoted, unescaped.
    pub value: String,
    /// The indent, `export `, and the key.
    pub prefix: String,
    /// ` = `, as written.
    pub separator: String,
    /// `"`, `'`, `` ` `` or nothing.
    pub quote: String,
    /// What follows the value: blanks and a comment.
    pub trailing: String,
    /// The entry as the file holds it, its end of line apart.
    pub raw: String,
    pub eol: String,
}

/// A line kept as it is: a comment, a blank, what is not `KEY=value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Other {
    pub raw: String,
    pub eol: String,
    pub blank: bool,
    pub comment: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub entries: Vec<Entry>,
    /// The file's most common line ending, for what is added.
    pub eol: String,
}

/// One row per key, in the file's order.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: String,
    /// The value parsers keep: the last line's.
    pub value: String,
    /// How many lines define it.
    pub count: usize,
    /// The comment above its first line.
    pub hint: String,
}

/// Values set by key, in the order they were given: a key the file does not
/// have goes at its end in this order.
pub type Edits = Vec<(String, String)>;

fn is_key_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// `[A-Za-z_][A-Za-z0-9_.-]*`, whole.
pub fn is_key(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty() && is_key_start(bytes[0]) && bytes.iter().all(|byte| is_key_byte(*byte))
}

/// A key from `at` then `[ \t]*=[ \t]*`: where the key and the separator end.
fn key_and_separator(line: &[u8], at: usize) -> Option<(usize, usize)> {
    if !line.get(at).is_some_and(|byte| is_key_start(*byte)) {
        return None;
    }
    let mut key_end = at + 1;
    while line.get(key_end).is_some_and(|byte| is_key_byte(*byte)) {
        key_end += 1;
    }
    let mut i = key_end;
    while line.get(i).is_some_and(|byte| is_blank(*byte)) {
        i += 1;
    }
    if line.get(i) != Some(&b'=') {
        return None;
    }
    i += 1;
    while line.get(i).is_some_and(|byte| is_blank(*byte)) {
        i += 1;
    }
    Some((key_end, i))
}

/// `^([ \t]*(?:export[ \t]+)?)([A-Za-z_][A-Za-z0-9_.-]*)([ \t]*=[ \t]*)`: where
/// the lead, the key and the separator end.
fn assignment(line: &[u8]) -> Option<(usize, usize, usize)> {
    let mut lead = 0;
    while line.get(lead).is_some_and(|byte| is_blank(*byte)) {
        lead += 1;
    }
    if line[lead..].starts_with(b"export") && line.get(lead + 6).is_some_and(|b| is_blank(*b)) {
        let mut after = lead + 6;
        while line.get(after).is_some_and(|byte| is_blank(*byte)) {
            after += 1;
        }
        if let Some((key_end, end)) = key_and_separator(line, after) {
            return Some((after, key_end, end));
        }
    }
    key_and_separator(line, lead).map(|(key_end, end)| (lead, key_end, end))
}

/// `# OLD_TOKEN=abc` is a value put aside, not an explanation: never a hint.
fn is_commented_assignment(line: &str) -> bool {
    let rest = line.trim_start_matches('#');
    rest.len() < line.len() && assignment(rest.as_bytes()).is_some()
}

fn line_end(text: &[u8], at: usize) -> usize {
    text[at.min(text.len())..]
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
        .map_or(text.len(), |offset| at + offset)
}

fn eol_at(text: &[u8], at: usize) -> &'static str {
    match (text.get(at), text.get(at + 1)) {
        (Some(b'\r'), Some(b'\n')) => "\r\n",
        (Some(b'\r'), _) => "\r",
        (Some(b'\n'), _) => "\n",
        _ => "",
    }
}

/// Where the quote opened before `at` closes.
fn closing(text: &[u8], at: usize, quote: u8, escapes: bool) -> Option<usize> {
    let mut i = at;
    while i < text.len() {
        if escapes && text[i] == b'\\' {
            i += 1;
        } else if text[i] == quote {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn unquoted(inner: &str, quote: u8, escapes: bool) -> String {
    if quote == b'"' {
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some(next) => match next {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '"' | '\\' | '$' => out.push(next),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                },
                None => out.push('\\'),
            }
        }
        return out;
    }
    if escapes {
        let escaped = format!("\\{}", quote as char);
        return inner.replace(&escaped, &(quote as char).to_string());
    }
    inner.to_owned()
}

struct Read {
    value: String,
    quote: String,
    trailing: String,
    /// Where the entry ends, for a quoted value: the end of its last line.
    end: Option<usize>,
}

/// A quoted value, over several lines if it says so. A quote counts as closed
/// only when nothing but a comment follows it: otherwise `A="x` would swallow
/// the lines below it, and what is a value would be read as keys.
fn read_quoted(text: &str, start: usize) -> Option<Read> {
    let bytes = text.as_bytes();
    let quote = bytes[start];
    let tries: &[bool] = if quote == b'"' {
        &[true]
    } else {
        &[false, true]
    };
    for &escapes in tries {
        let Some(close) = closing(bytes, start + 1, quote, escapes) else {
            continue;
        };
        let end = line_end(bytes, close + 1);
        let rest = cut(text, close + 1, end);
        let blanks = rest.bytes().take_while(|byte| is_blank(*byte)).count();
        if !(blanks == rest.len() || rest.as_bytes()[blanks] == b'#') {
            continue;
        }
        return Some(Read {
            value: unquoted(cut(text, start + 1, close), quote, escapes),
            quote: (quote as char).to_string(),
            trailing: rest.to_owned(),
            end: Some(end),
        });
    }
    None
}

fn read_bare(rest: &str, after_blank: bool) -> Read {
    let bytes = rest.as_bytes();
    let mut cut_at = bytes.len();
    for (i, byte) in bytes.iter().enumerate() {
        let opens = if i > 0 {
            is_blank(bytes[i - 1])
        } else {
            after_blank
        };
        if *byte == b'#' && opens {
            cut_at = i;
            break;
        }
    }
    let mut end = cut_at;
    while end > 0 && is_blank(bytes[end - 1]) {
        end -= 1;
    }
    Read {
        value: cut(rest, 0, end).to_owned(),
        quote: String::new(),
        trailing: from(rest, end).to_owned(),
        end: None,
    }
}

/// The text, as entries: a pair, or a line kept as it is. [`serialize`] gives
/// back the same text.
pub fn parse(text: &str) -> Doc {
    let bytes = text.as_bytes();
    let mut entries = Vec::new();
    let mut at = 0;
    let (mut crlf, mut lf) = (0, 0);
    while at < bytes.len() {
        let end = line_end(bytes, at);
        let line = cut(text, at, end);
        let mut entry_end = end;
        match assignment(line.as_bytes()) {
            Some((lead_end, key_end, head)) => {
                let start = at + head;
                let quoted = match bytes.get(start) {
                    Some(b'"' | b'\'' | b'`') => read_quoted(text, start),
                    _ => None,
                };
                let separator = cut(line, key_end, head);
                let read = match quoted {
                    Some(read) => read,
                    None => read_bare(cut(text, start, end), separator.ends_with([' ', '\t'])),
                };
                if let Some(quoted_end) = read.end {
                    entry_end = quoted_end;
                }
                entries.push(Entry::Pair(Pair {
                    key: cut(line, lead_end, key_end).to_owned(),
                    value: read.value,
                    prefix: cut(line, 0, key_end).to_owned(),
                    separator: separator.to_owned(),
                    quote: read.quote,
                    trailing: read.trailing,
                    raw: cut(text, at, entry_end).to_owned(),
                    eol: eol_at(bytes, entry_end).to_owned(),
                }));
            }
            None => {
                let trimmed = js::trim(line);
                entries.push(Entry::Other(Other {
                    raw: line.to_owned(),
                    eol: eol_at(bytes, end).to_owned(),
                    blank: trimmed.is_empty(),
                    comment: trimmed.starts_with('#'),
                }));
            }
        }
        let eol = eol_at(bytes, entry_end);
        match eol {
            "\r\n" => crlf += 1,
            "\n" => lf += 1,
            _ => {}
        }
        at = entry_end + eol.len();
    }
    Doc {
        entries,
        eol: if crlf > lf { "\r\n" } else { "\n" }.to_owned(),
    }
}

pub fn serialize(doc: &Doc) -> String {
    let mut out = String::new();
    for entry in &doc.entries {
        let (raw, eol) = match entry {
            Entry::Pair(pair) => (&pair.raw, &pair.eol),
            Entry::Other(other) => (&other.raw, &other.eol),
        };
        out.push_str(raw);
        out.push_str(eol);
    }
    out
}

/// `[A-Za-z0-9_./:@%+,=~-]+`: a value that needs no quotes anywhere.
fn bare_safe(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'.' | b'/' | b':' | b'@' | b'%' | b'+' | b',' | b'=' | b'~' | b'-'
                )
        })
}

/// A value as the file should hold it. Single quotes are the least magical:
/// no escape, no expansion, several lines allowed. A value holding one needs
/// double quotes, and then every parser reads \\ \" \n and \r back.
fn quoted(value: &str, previous: &str) -> String {
    if value.is_empty() {
        return format!("{previous}{previous}");
    }
    if bare_safe(value) {
        return format!("{previous}{value}{previous}");
    }
    if !value.contains('\'') {
        return format!("'{value}'");
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn rendered(pair: &Pair, value: &str) -> String {
    let body = quoted(value, &pair.quote);
    let gap = if !body.is_empty() && pair.trailing.starts_with('#') {
        " "
    } else {
        ""
    };
    format!(
        "{}{}{body}{gap}{}",
        pair.prefix, pair.separator, pair.trailing
    )
}

/// The doc with these values set, each on the last line that defines its
/// key, the one parsers keep. A key the file does not have goes at the end.
/// Nothing else moves.
pub fn with_values(doc: &Doc, edits: &[(String, String)]) -> Doc {
    let mut remaining: Vec<(String, String)> = Vec::new();
    for (key, value) in edits {
        match remaining.iter_mut().find(|(other, _)| other == key) {
            Some(slot) => slot.1 = value.clone(),
            None => remaining.push((key.clone(), value.clone())),
        }
    }
    let mut last: HashMap<&str, usize> = HashMap::new();
    for (index, entry) in doc.entries.iter().enumerate() {
        if let Entry::Pair(pair) = entry {
            last.insert(&pair.key, index);
        }
    }
    let mut entries: Vec<Entry> = doc
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let Entry::Pair(pair) = entry else {
                return entry.clone();
            };
            if last.get(pair.key.as_str()) != Some(&index) {
                return entry.clone();
            }
            let Some(at) = remaining.iter().position(|(key, _)| *key == pair.key) else {
                return entry.clone();
            };
            let (_, value) = remaining.remove(at);
            if value == pair.value {
                return entry.clone();
            }
            let raw = rendered(pair, &value);
            Entry::Pair(Pair {
                value,
                raw,
                ..pair.clone()
            })
        })
        .collect();
    for (key, value) in remaining {
        if let Some(tail) = entries.last_mut() {
            let eol = match tail {
                Entry::Pair(pair) => &mut pair.eol,
                Entry::Other(other) => &mut other.eol,
            };
            if eol.is_empty() {
                *eol = doc.eol.clone();
            }
        }
        let mut fresh = Pair {
            key: key.clone(),
            value: value.clone(),
            prefix: key,
            separator: "=".to_owned(),
            quote: String::new(),
            trailing: String::new(),
            raw: String::new(),
            eol: doc.eol.clone(),
        };
        fresh.raw = rendered(&fresh, &value);
        entries.push(Entry::Pair(fresh));
    }
    Doc {
        entries,
        eol: doc.eol.clone(),
    }
}

/// One row per key, in the file's order: the value parsers keep, the comment
/// above it, how many lines define it.
pub fn rows(doc: &Doc) -> Vec<Row> {
    let mut found: Vec<Row> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut comments: Vec<String> = Vec::new();
    for entry in &doc.entries {
        match entry {
            Entry::Other(other) => {
                let line = js::trim(&other.raw);
                if other.comment && !is_commented_assignment(line) {
                    let text = line.trim_start_matches('#');
                    let text = text.strip_prefix([' ', '\t']).unwrap_or(text);
                    comments.push(text.to_owned());
                } else if !other.comment {
                    comments.clear();
                }
            }
            Entry::Pair(pair) => {
                match index.get(&pair.key) {
                    Some(&at) => {
                        found[at].value = pair.value.clone();
                        found[at].count += 1;
                    }
                    None => {
                        index.insert(pair.key.clone(), found.len());
                        found.push(Row {
                            key: pair.key.clone(),
                            value: pair.value.clone(),
                            count: 1,
                            hint: js::first_chars(&comments.join(" "), HINT_MAX),
                        });
                    }
                }
                comments.clear();
            }
        }
    }
    found
}

/// Every value the file holds, those a later line overrides included.
pub fn all_values(doc: &Doc) -> Vec<String> {
    doc.entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Pair(pair) => Some(pair.value.clone()),
            Entry::Other(_) => None,
        })
        .collect()
}

/// Lines that are neither a pair, a comment nor blank: never shown, kept.
pub fn other_lines(doc: &Doc) -> usize {
    doc.entries
        .iter()
        .filter(|entry| matches!(entry, Entry::Other(other) if !other.blank && !other.comment))
        .count()
}

/// Which keys got a value and which lost theirs, by name. It returns no value.
pub struct Changed {
    pub set: Vec<String>,
    pub emptied: Vec<String>,
}

pub fn changed_keys(before: &Doc, after: &Doc) -> Changed {
    let old: HashMap<String, String> = rows(before)
        .into_iter()
        .map(|row| (row.key, row.value))
        .collect();
    let mut changed = Changed {
        set: Vec::new(),
        emptied: Vec::new(),
    };
    for row in rows(after) {
        match old.get(&row.key) {
            Some(value) if *value == row.value => continue,
            Some(_) if row.value.is_empty() => changed.emptied.push(row.key),
            _ if !row.value.is_empty() => changed.set.push(row.key),
            _ => {}
        }
    }
    changed
}

/// `.env`, `.env.local`, `app.env`, any case.
pub fn is_dotenv_path(path: &str) -> bool {
    let name = js::base_name(path).to_lowercase();
    let after_env = |rest: &str| !rest.is_empty() && !rest.chars().any(js::is_line_terminator);
    if name == ".env" || name.strip_prefix(".env.").is_some_and(after_env) {
        return true;
    }
    name.strip_suffix(".env")
        .and_then(|before| before.chars().last())
        .is_some_and(|c| !js::is_line_terminator(c))
}

/// A template meant to be committed, with no secret to protect.
pub fn is_example_path(path: &str) -> bool {
    if !is_dotenv_path(path) {
        return false;
    }
    let name = js::base_name(path).to_lowercase();
    [
        ".example",
        ".sample",
        ".template",
        ".dist",
        ".default",
        ".defaults",
    ]
    .iter()
    .any(|end| name.ends_with(end))
}

/// Where this file's template may be, the nearest name first.
pub fn example_candidates(path: &str) -> Vec<String> {
    let dir = js::dir_name(path);
    let name = js::base_name(path);
    let names = [
        format!("{name}.example"),
        format!("{name}.sample"),
        ".env.example".to_owned(),
        ".env.sample".to_owned(),
        ".env.template".to_owned(),
        ".env.dist".to_owned(),
    ];
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|other| seen.insert(other.clone()) && other != name)
        .map(|other| format!("{dir}{other}"))
        .collect()
}
