//! Is this file broken? JSON is checked whole, so "valid" means valid. TOML is
//! parsed for its syntax, and YAML is only scanned for what is certainly
//! wrong; both answer `partial`, and the page says it is a basic check. An
//! error gives a code the page translates, and a line when one can be told.
//! validate.js, ported.

use std::collections::HashSet;

use crate::js;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checker {
    Json,
    Jsonc,
    Yaml,
    Toml,
}

impl Checker {
    /// How the page names it.
    pub fn name(self) -> &'static str {
        match self {
            Checker::Json => "JSON",
            Checker::Jsonc => "JSONC",
            Checker::Yaml => "YAML",
            Checker::Toml => "TOML",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// None: no check applies, so nothing is claimed.
    pub kind: Option<Checker>,
    pub ok: bool,
    /// A basic check, not a proof.
    pub partial: bool,
    /// What is wrong, as `check.<code>` names it.
    pub code: Option<&'static str>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl Check {
    fn ok(kind: Option<Checker>, partial: bool) -> Self {
        Check {
            kind,
            ok: true,
            partial,
            code: None,
            line: None,
            column: None,
        }
    }

    fn broken(
        kind: Checker,
        partial: bool,
        code: &'static str,
        at: Option<(usize, usize)>,
    ) -> Self {
        Check {
            kind: Some(kind),
            ok: false,
            partial,
            code: Some(code),
            line: at.map(|(line, _)| line),
            column: at.map(|(_, column)| column),
        }
    }
}

fn not_terminated(text: &str) -> bool {
    !text.chars().any(js::is_line_terminator)
}

/// Which check applies.
pub fn checker_for(path: &str) -> Option<Checker> {
    let name = js::base_name(path).to_lowercase();
    if name.ends_with(".jsonc") || name.ends_with(".code-workspace") {
        return Some(Checker::Jsonc);
    }
    if name.ends_with(".json") {
        // These are read with comments and trailing commas by what uses them.
        let config = ["tsconfig", "jsconfig"].iter().any(|start| {
            name.strip_prefix(start).is_some_and(|rest| {
                rest == ".json"
                    || (rest.len() >= 6
                        && rest.starts_with(['.', '-'])
                        && rest.ends_with(".json")
                        && not_terminated(rest))
            })
        });
        if config
            || matches!(
                name.as_str(),
                ".eslintrc.json" | "devcontainer.json" | ".devcontainer.json"
            )
        {
            return Some(Checker::Jsonc);
        }
        let lower = path.to_lowercase();
        let under = |dir: &str| {
            lower.starts_with(&format!("{dir}/")) || lower.contains(&format!("/{dir}/"))
        };
        if under(".vscode") || under(".devcontainer") {
            return Some(Checker::Jsonc);
        }
        return Some(Checker::Json);
    }
    if name.ends_with(".yaml") || name.ends_with(".yml") {
        return Some(Checker::Yaml);
    }
    if name.ends_with(".toml") {
        return Some(Checker::Toml);
    }
    None
}

pub fn validate(path: &str, text: &str) -> Check {
    match checker_for(path) {
        Some(kind @ (Checker::Json | Checker::Jsonc)) => check_json(text, kind),
        Some(Checker::Yaml) => check_yaml(text),
        Some(Checker::Toml) => check_toml(text),
        None => Check::ok(None, false),
    }
}

/* ── JSON ─────────────────────────────────────────────────────────── */

/// Comments and trailing commas blanked, every other character where it was.
fn blank_jsonc(text: &str) -> String {
    let mut out: Vec<char> = text.chars().collect();
    let source = out.clone();
    let blank = |out: &mut Vec<char>, from: usize, to: usize| {
        for c in &mut out[from..to] {
            if *c != '\n' && *c != '\r' {
                *c = ' ';
            }
        }
    };
    let mut last_comma: Option<usize> = None;
    let mut i = 0;
    while i < source.len() {
        let c = source[i];
        let next = source.get(i + 1).copied();
        if c == '"' {
            i += 1;
            while i < source.len() && source[i] != '"' {
                if source[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            last_comma = None;
        } else if c == '/' && next == Some('/') {
            let mut end = i;
            while end < source.len() && source[end] != '\n' {
                end += 1;
            }
            blank(&mut out, i, end);
            i = end.saturating_sub(1).max(i);
        } else if c == '/' && next == Some('*') {
            let close = (i + 2..source.len().saturating_sub(1))
                .find(|&at| source[at] == '*' && source[at + 1] == '/');
            let end = close.map_or(source.len(), |close| close + 2);
            blank(&mut out, i, end);
            i = end - 1;
        } else if c == ',' {
            last_comma = Some(i);
        } else if c == ']' || c == '}' {
            if let Some(comma) = last_comma.take() {
                out[comma] = ' ';
            }
        } else if !js::is_space(c) {
            last_comma = None;
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Where JSON goes wrong, or none when it is valid: the grammar walked
/// without a stack of calls, so that no depth is too deep.
fn json_error_at(text: &[u8]) -> Option<usize> {
    let mut i = 0;
    let blanks = |i: &mut usize| {
        while text
            .get(*i)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            *i += 1;
        }
    };
    let hex = |at: usize, n: usize| {
        text.len() >= at + n && text[at..at + n].iter().all(u8::is_ascii_hexdigit)
    };
    let string = |i: &mut usize| -> Result<(), usize> {
        *i += 1;
        loop {
            match text.get(*i) {
                None => return Err(*i),
                Some(byte) if *byte < 0x20 => return Err(*i),
                Some(b'"') => {
                    *i += 1;
                    return Ok(());
                }
                Some(b'\\') => match text.get(*i + 1) {
                    Some(b'u') if hex(*i + 2, 4) => *i += 6,
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => *i += 2,
                    _ => return Err(*i),
                },
                Some(_) => *i += 1,
            }
        }
    };
    let scalar = |i: &mut usize| -> Result<(), usize> {
        let rest = &text[*i..];
        let number = {
            let mut n = 0;
            if rest.first() == Some(&b'-') {
                n += 1;
            }
            match rest.get(n) {
                Some(b'0') => {
                    n += 1;
                    Some(n)
                }
                Some(b'1'..=b'9') => {
                    n += 1;
                    while rest.get(n).is_some_and(u8::is_ascii_digit) {
                        n += 1;
                    }
                    Some(n)
                }
                _ => None,
            }
            .map(|mut n| {
                if rest.get(n) == Some(&b'.') && rest.get(n + 1).is_some_and(u8::is_ascii_digit) {
                    n += 2;
                    while rest.get(n).is_some_and(u8::is_ascii_digit) {
                        n += 1;
                    }
                }
                if matches!(rest.get(n), Some(b'e' | b'E')) {
                    let sign = usize::from(matches!(rest.get(n + 1), Some(b'+' | b'-')));
                    if rest.get(n + 1 + sign).is_some_and(u8::is_ascii_digit) {
                        n += 1 + sign;
                        while rest.get(n).is_some_and(u8::is_ascii_digit) {
                            n += 1;
                        }
                    }
                }
                n
            })
        };
        let length = number.or_else(|| {
            ["true", "false", "null"]
                .iter()
                .find(|word| rest.starts_with(word.as_bytes()))
                .map(|word| word.len())
        });
        match length {
            Some(length) => {
                *i += length;
                Ok(())
            }
            None => Err(*i),
        }
    };
    // A member's key and colon, `i` at its quote.
    let key = |i: &mut usize| -> Result<(), usize> {
        blanks(i);
        if text.get(*i) != Some(&b'"') {
            return Err(*i);
        }
        string(i)?;
        blanks(i);
        if text.get(*i) != Some(&b':') {
            return Err(*i);
        }
        *i += 1;
        Ok(())
    };
    let walk = |i: &mut usize| -> Result<(), usize> {
        let mut open: Vec<u8> = Vec::new();
        'value: loop {
            blanks(i);
            match text.get(*i) {
                Some(b'"') => string(i)?,
                Some(&bracket @ (b'[' | b'{')) => {
                    let close = if bracket == b'[' { b']' } else { b'}' };
                    *i += 1;
                    blanks(i);
                    if text.get(*i) == Some(&close) {
                        *i += 1;
                    } else {
                        open.push(bracket);
                        if bracket == b'{' {
                            key(i)?;
                        }
                        continue 'value;
                    }
                }
                _ => scalar(i)?,
            }
            // A value ended: close what it ends, or go on to the next one.
            loop {
                let Some(&bracket) = open.last() else {
                    return Ok(());
                };
                let close = if bracket == b'[' { b']' } else { b'}' };
                blanks(i);
                if text.get(*i) == Some(&close) {
                    *i += 1;
                    open.pop();
                    continue;
                }
                if text.get(*i) != Some(&b',') {
                    return Err(*i);
                }
                *i += 1;
                if bracket == b'{' {
                    key(i)?;
                }
                continue 'value;
            }
        }
    };
    match walk(&mut i) {
        Err(at) => Some(at),
        Ok(()) => {
            blanks(&mut i);
            (i < text.len()).then_some(i)
        }
    }
}

fn check_json(text: &str, kind: Checker) -> Check {
    let source = if kind == Checker::Jsonc {
        blank_jsonc(text)
    } else {
        text.to_owned()
    };
    match json_error_at(source.as_bytes()) {
        None => Check::ok(Some(kind), false),
        Some(at) => Check::broken(
            kind,
            false,
            "json",
            Some(js::line_of(source.as_bytes(), at)),
        ),
    }
}

/* ── YAML: only what is certainly wrong ───────────────────────────── */

fn split_lines(text: &str) -> Vec<&str> {
    text.split("\r\n")
        .flat_map(|part| part.split(['\n', '\r']))
        .collect()
}

/// `^[|>][-+0-9]*[ \t]*(#.*)?$`: a block scalar's header.
fn block_header(rest: &[char]) -> bool {
    let mut i = 1;
    while rest
        .get(i)
        .is_some_and(|c| *c == '-' || *c == '+' || c.is_ascii_digit())
    {
        i += 1;
    }
    while rest.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
        i += 1;
    }
    match rest.get(i) {
        None => true,
        Some('#') => !rest[i..].iter().any(|c| js::is_line_terminator(*c)),
        Some(_) => false,
    }
}

fn check_yaml(text: &str) -> Check {
    let fail = |code, line, column| Check::broken(Checker::Yaml, true, code, Some((line, column)));
    let mut flow: Vec<(char, usize, usize)> = Vec::new();
    let mut quote: Option<(char, usize, usize)> = None;
    let mut block_indent: Option<usize> = None;
    // The indent of the line that ended in a plain scalar, which may go on below.
    let mut plain_indent: Option<usize> = None;

    for (n, raw) in split_lines(text).into_iter().enumerate() {
        let line: Vec<char> = raw.chars().collect();
        let mut at = 0;
        // A token may start here: a quote or a bracket means something.
        let mut start = true;
        let indent = line.iter().take_while(|c| **c == ' ').count();

        if quote.is_none() && flow.is_empty() {
            let lead = line
                .iter()
                .take_while(|c| **c == ' ' || **c == '\t')
                .count();
            let rest = &line[lead..];
            if rest.is_empty() {
                continue;
            }
            if let Some(block) = block_indent {
                if indent > block {
                    continue;
                }
                block_indent = None;
            }
            if rest[0] == '#' {
                continue;
            }
            if plain_indent.is_some_and(|plain| indent > plain) {
                continue;
            }
            plain_indent = None;
            if let Some(tab) = line[..lead].iter().position(|c| *c == '\t') {
                return fail("yaml-tab", n + 1, tab + 1);
            }
            if line.first() == Some(&'%') {
                continue;
            }
            at = lead;
            let marker = rest.starts_with(&['-', '-', '-']) || rest.starts_with(&['.', '.', '.']);
            if marker && rest.get(3).is_none_or(|c| *c == ' ' || *c == '\t') {
                at += 3;
            }
        }

        let mut plain = false;
        while at < line.len() {
            let c = line[at];
            if let Some((open, _, _)) = quote {
                if open == '"' && c == '\\' {
                    at += 1;
                } else if c == open {
                    if open == '\'' && line.get(at + 1) == Some(&'\'') {
                        at += 1;
                    } else {
                        quote = None;
                    }
                }
                plain = false;
                start = false;
                at += 1;
                continue;
            }
            let previous = at.checked_sub(1).map(|before| line[before]);
            let next = line.get(at + 1).copied();
            let blank_next = next.is_none_or(|next| next == ' ' || next == '\t');
            if c == ' ' || c == '\t' {
                at += 1;
                continue;
            }
            if c == '#' && previous.is_none_or(|before| before == ' ' || before == '\t') {
                break;
            }
            if start {
                if c == '"' || c == '\'' {
                    quote = Some((c, n + 1, at + 1));
                    start = false;
                    at += 1;
                    continue;
                }
                if c == '[' || c == '{' {
                    flow.push((c, n + 1, at + 1));
                    at += 1;
                    continue;
                }
                if (c == ']' || c == '}') && flow.is_empty() {
                    return fail("yaml-bracket", n + 1, at + 1);
                }
                if flow.is_empty() && (c == '|' || c == '>') && block_header(&line[at..]) {
                    block_indent = Some(indent);
                    plain = false;
                    break;
                }
                if (c == '&' || c == '!') && !blank_next {
                    while line
                        .get(at + 1)
                        .is_some_and(|after| *after != ' ' && *after != '\t')
                    {
                        at += 1;
                    }
                    at += 1;
                    continue;
                }
                if matches!(c, '-' | '?' | ':') && blank_next {
                    at += 1;
                    continue;
                }
                if c == '@' || c == '`' {
                    return fail("yaml-reserved", n + 1, at + 1);
                }
                start = false;
                plain = true;
            }
            if c == ':'
                && (blank_next
                    || (!flow.is_empty() && next.is_some_and(|next| ",[]{}".contains(next))))
            {
                start = true;
                plain = false;
            } else if !flow.is_empty() {
                if c == ',' || (c == ':' && previous == Some('"')) {
                    start = true;
                    plain = false;
                } else if c == ']' || c == '}' {
                    let (open, _, _) = flow.pop().unwrap_or((' ', 0, 0));
                    let closes = if open == '[' { ']' } else { '}' };
                    if closes != c {
                        return fail("yaml-bracket", n + 1, at + 1);
                    }
                    plain = false;
                }
            }
            at += 1;
        }
        plain_indent = (plain && quote.is_none() && flow.is_empty()).then_some(indent);
    }
    if let Some((_, line, column)) = quote {
        return fail("yaml-quote", line, column);
    }
    if let Some((_, line, column)) = flow.last() {
        return fail("yaml-bracket", *line, *column);
    }
    Check::ok(Some(Checker::Yaml), true)
}

/* ── TOML: its syntax, and keys or tables given twice ─────────────── */

struct Toml<'a> {
    text: &'a str,
    b: &'a [u8],
    i: usize,
}

type Stop = (&'static str, usize);

fn digits(b: &[u8], at: usize, n: usize) -> bool {
    b.len() >= at + n && b[at..at + n].iter().all(u8::is_ascii_digit)
}

/// `first(?:_?more)*` from `at`, `first` and `more` the same class: its end.
fn underscored(b: &[u8], at: usize, class: impl Fn(u8) -> bool) -> Option<usize> {
    if !b.get(at).is_some_and(|byte| class(*byte)) {
        return None;
    }
    let mut end = at + 1;
    loop {
        match b.get(end) {
            Some(byte) if class(*byte) => end += 1,
            Some(b'_') if b.get(end + 1).is_some_and(|byte| class(*byte)) => end += 2,
            _ => return Some(end),
        }
    }
}

/// The length of what TOML's numbers and dates read at `at`: the first of
/// the page's alternatives that matches.
fn number_or_date(b: &[u8], at: usize) -> Option<usize> {
    let rest = &b[at..];
    let signed = usize::from(matches!(rest.first(), Some(b'+' | b'-')));
    // [+-]?(?:inf|nan)
    if rest[signed..].starts_with(b"inf") || rest[signed..].starts_with(b"nan") {
        return Some(signed + 3);
    }
    // 0x…, 0o…, 0b…
    for (prefix, class) in [
        (b"0x", u8::is_ascii_hexdigit as fn(&u8) -> bool),
        (b"0o", |byte: &u8| (b'0'..=b'7').contains(byte)),
        (b"0b", |byte: &u8| *byte == b'0' || *byte == b'1'),
    ] {
        if rest.starts_with(prefix)
            && let Some(end) = underscored(rest, 2, |byte| class(&byte))
        {
            return Some(end);
        }
    }
    // \d{4}-\d{2}-\d{2}(?:[Tt ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})?)?
    if digits(rest, 0, 4)
        && rest.get(4) == Some(&b'-')
        && digits(rest, 5, 2)
        && rest.get(7) == Some(&b'-')
        && digits(rest, 8, 2)
    {
        let mut end = 10;
        if matches!(rest.get(10), Some(b'T' | b't' | b' '))
            && digits(rest, 11, 2)
            && rest.get(13) == Some(&b':')
            && digits(rest, 14, 2)
            && rest.get(16) == Some(&b':')
            && digits(rest, 17, 2)
        {
            end = 19;
            if rest.get(end) == Some(&b'.') && digits(rest, end + 1, 1) {
                end += 2;
                while digits(rest, end, 1) {
                    end += 1;
                }
            }
            match rest.get(end) {
                Some(b'Z' | b'z') => end += 1,
                Some(b'+' | b'-')
                    if digits(rest, end + 1, 2)
                        && rest.get(end + 3) == Some(&b':')
                        && digits(rest, end + 4, 2) =>
                {
                    end += 6
                }
                _ => {}
            }
        }
        return Some(end);
    }
    // \d{2}:\d{2}:\d{2}(?:\.\d+)?
    if digits(rest, 0, 2)
        && rest.get(2) == Some(&b':')
        && digits(rest, 3, 2)
        && rest.get(5) == Some(&b':')
        && digits(rest, 6, 2)
    {
        let mut end = 8;
        if rest.get(end) == Some(&b'.') && digits(rest, end + 1, 1) {
            end += 2;
            while digits(rest, end, 1) {
                end += 1;
            }
        }
        return Some(end);
    }
    // [+-]?(?:0|[1-9](?:_?\d)*)(?:\.\d(?:_?\d)*)?(?:[eE][+-]?\d(?:_?\d)*)?
    let digit = |byte: u8| byte.is_ascii_digit();
    let mut end = match rest.get(signed) {
        Some(b'0') => signed + 1,
        Some(b'1'..=b'9') => underscored(rest, signed, digit)?,
        _ => return None,
    };
    if rest.get(end) == Some(&b'.')
        && let Some(after) = underscored(rest, end + 1, digit)
    {
        end = after;
    }
    if matches!(rest.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(rest.get(end + 1), Some(b'+' | b'-')));
        if let Some(after) = underscored(rest, end + 1 + sign, digit) {
            end = after;
        }
    }
    Some(end)
}

impl Toml<'_> {
    fn at(&self, i: usize) -> Option<u8> {
        self.b.get(i).copied()
    }

    fn here(&self) -> Option<u8> {
        self.at(self.i)
    }

    fn starts(&self, what: &[u8]) -> bool {
        self.b[self.i.min(self.b.len())..].starts_with(what)
    }

    fn blanks(&mut self) {
        while matches!(self.here(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }

    fn comment(&mut self) {
        if self.here() == Some(b'#') {
            while self.i < self.b.len() && self.b[self.i] != b'\n' {
                self.i += 1;
            }
        }
    }

    fn at_eol(&self) -> bool {
        self.i >= self.b.len()
            || self.here() == Some(b'\n')
            || (self.here() == Some(b'\r') && self.at(self.i + 1) == Some(b'\n'))
    }

    fn step_eol(&mut self) {
        self.i += if self.here() == Some(b'\r') { 2 } else { 1 };
    }

    fn end_of_line(&mut self) -> Result<(), Stop> {
        self.blanks();
        self.comment();
        if !self.at_eol() {
            return Err(("toml-trailing", self.i));
        }
        self.step_eol();
        Ok(())
    }

    fn skip_blank_lines(&mut self) {
        loop {
            self.blanks();
            self.comment();
            if self.i >= self.b.len() || !self.at_eol() {
                return;
            }
            self.step_eol();
        }
    }

    fn basic_string(&mut self, multi: bool) -> Result<(), Stop> {
        let open = self.i;
        self.i += if multi { 3 } else { 1 };
        loop {
            let Some(c) = self.here() else {
                return Err(("toml-string", open));
            };
            if multi && self.starts(b"\"\"\"") {
                self.i += 3;
                while self.here() == Some(b'"') && self.i - open < self.b.len() {
                    self.i += 1;
                }
                return Ok(());
            }
            if !multi && c == b'"' {
                self.i += 1;
                return Ok(());
            }
            if !multi && (c == b'\n' || c == b'\r') {
                return Err(("toml-string", open));
            }
            if c == b'\\' {
                let hex = |at: usize, n: usize| {
                    self.b.len() >= at + n && self.b[at..at + n].iter().all(u8::is_ascii_hexdigit)
                };
                match self.at(self.i + 1) {
                    Some(b'b' | b't' | b'n' | b'f' | b'r' | b'"' | b'\\') => self.i += 2,
                    Some(b'u') if hex(self.i + 2, 4) => self.i += 6,
                    Some(b'U') if hex(self.i + 2, 8) => self.i += 10,
                    _ if multi && self.line_ending_backslash() => self.i += 2,
                    _ => return Err(("toml-escape", self.i)),
                }
            } else {
                self.i += 1;
            }
        }
    }

    /// `^\\[ \t]*\r?\n` within 200 characters.
    fn line_ending_backslash(&self) -> bool {
        let window = &self.b[self.i..(self.i + 200).min(self.b.len())];
        let mut k = 1;
        while matches!(window.get(k), Some(b' ' | b'\t')) {
            k += 1;
        }
        if window.get(k) == Some(&b'\r') {
            k += 1;
        }
        window.get(k) == Some(&b'\n')
    }

    fn literal_string(&mut self, multi: bool) -> Result<(), Stop> {
        let open = self.i;
        self.i += if multi { 3 } else { 1 };
        loop {
            let Some(c) = self.here() else {
                return Err(("toml-string", open));
            };
            if multi && self.starts(b"'''") {
                self.i += 3;
                while self.here() == Some(b'\'') {
                    self.i += 1;
                }
                return Ok(());
            }
            if !multi && c == b'\'' {
                self.i += 1;
                return Ok(());
            }
            if !multi && (c == b'\n' || c == b'\r') {
                return Err(("toml-string", open));
            }
            self.i += 1;
        }
    }

    /// One part of a key, as written, for telling duplicates.
    fn key_part(&mut self) -> Result<String, Stop> {
        let open = self.i;
        match self.here() {
            Some(b'"') => self.basic_string(false)?,
            Some(b'\'') => self.literal_string(false)?,
            _ => {
                while self
                    .here()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                {
                    self.i += 1;
                }
                if self.i == open {
                    return Err(("toml-key", self.i));
                }
            }
        }
        Ok(js::cut(self.text, open, self.i).to_owned())
    }

    fn key(&mut self) -> Result<Vec<String>, Stop> {
        let mut parts = vec![self.key_part()?];
        loop {
            self.blanks();
            if self.here() != Some(b'.') {
                return Ok(parts);
            }
            self.i += 1;
            self.blanks();
            parts.push(self.key_part()?);
        }
    }

    fn array_gap(&mut self) {
        loop {
            self.blanks();
            self.comment();
            if self.i < self.b.len() && self.at_eol() {
                self.step_eol();
            } else {
                return;
            }
        }
    }

    fn array(&mut self) -> Result<(), Stop> {
        let open = self.i;
        self.i += 1;
        self.array_gap();
        while self.here() != Some(b']') {
            if self.i >= self.b.len() {
                return Err(("toml-value", open));
            }
            self.value()?;
            self.array_gap();
            if self.here() == Some(b',') {
                self.i += 1;
                self.array_gap();
            } else if self.here() != Some(b']') {
                let at = if self.i >= self.b.len() { open } else { self.i };
                return Err(("toml-value", at));
            }
        }
        self.i += 1;
        Ok(())
    }

    fn inline_table(&mut self) -> Result<(), Stop> {
        self.i += 1;
        self.blanks();
        if self.here() == Some(b'}') {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.key()?;
            if self.here() != Some(b'=') {
                return Err(("toml-equals", self.i));
            }
            self.i += 1;
            self.blanks();
            self.value()?;
            self.blanks();
            match self.here() {
                Some(b',') => {
                    self.i += 1;
                    self.blanks();
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => return Err(("toml-value", self.i)),
            }
        }
    }

    fn value(&mut self) -> Result<(), Stop> {
        if self.starts(b"\"\"\"") {
            return self.basic_string(true);
        }
        if self.starts(b"'''") {
            return self.literal_string(true);
        }
        match self.here() {
            Some(b'"') => return self.basic_string(false),
            Some(b'\'') => return self.literal_string(false),
            Some(b'[') => return self.array(),
            Some(b'{') => return self.inline_table(),
            _ => {}
        }
        if self.starts(b"true") {
            self.i += 4;
        } else if self.starts(b"false") {
            self.i += 5;
        } else {
            match number_or_date(self.b, self.i.min(self.b.len())) {
                Some(length) if length > 0 => self.i += length,
                _ => return Err(("toml-value", self.i)),
            }
        }
        if self.i < self.b.len() {
            let ends = js::from(self.text, self.i)
                .chars()
                .next()
                .is_some_and(|c| js::is_space(c) || ",]}#".contains(c));
            if !ends {
                return Err(("toml-value", self.i));
            }
        }
        Ok(())
    }

    fn document(&mut self) -> Result<(), Stop> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut table = String::new();
        loop {
            self.skip_blank_lines();
            if self.i >= self.b.len() {
                return Ok(());
            }
            let start = self.i;
            if self.here() == Some(b'[') {
                let is_array = self.at(self.i + 1) == Some(b'[');
                self.i += if is_array { 2 } else { 1 };
                self.blanks();
                let name = self.key()?.join("\u{0}");
                if self.here() != Some(b']') || (is_array && self.at(self.i + 1) != Some(b']')) {
                    return Err(("toml-header", self.i));
                }
                self.i += if is_array { 2 } else { 1 };
                self.end_of_line()?;
                if is_array {
                    // Each [[item]] is a table of its own: its keys may repeat the last's.
                    let inside = format!("{name}\u{0}");
                    let header = format!("[{name}\u{0}");
                    seen.retain(|other| !other.starts_with(&inside) && !other.starts_with(&header));
                } else if !seen.insert(format!("[{name}]")) {
                    return Err(("toml-duplicate", start));
                }
                table = name;
            } else {
                let name = self.key()?.join("\u{0}");
                if self.here() != Some(b'=') {
                    return Err(("toml-equals", self.i));
                }
                self.i += 1;
                self.blanks();
                self.value()?;
                self.end_of_line()?;
                let whole = if table.is_empty() {
                    name
                } else {
                    format!("{table}\u{0}{name}")
                };
                if !seen.insert(whole) {
                    return Err(("toml-duplicate", start));
                }
            }
        }
    }
}

fn check_toml(text: &str) -> Check {
    let mut toml = Toml {
        text,
        b: text.as_bytes(),
        i: 0,
    };
    match toml.document() {
        Ok(()) => Check::ok(Some(Checker::Toml), true),
        Err((code, at)) => Check::broken(
            Checker::Toml,
            true,
            code,
            Some(js::line_of(text.as_bytes(), at)),
        ),
    }
}
