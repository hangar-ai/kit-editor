//! Whether .gitignore files ignore a path, by git's own rules. What git also
//! reads and a tab cannot (.git/info/exclude, the global excludes, whether the
//! file is already tracked) is said in the README. gitignore.js, ported: the
//! page turned each pattern into a regular expression; this matches the same
//! strings with the same rules, without one.

use crate::js;

/// A character class's member.
#[derive(Debug, Clone, PartialEq)]
enum Member {
    One(char),
    Range(char, char),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Literal(char),
    /// `*`: `[^/]*`.
    Star,
    /// `?`: `[^/]`.
    One,
    /// `[…]`.
    Class {
        negated: bool,
        members: Vec<Member>,
    },
    /// `**/`, and what an unanchored pattern starts with: `(?:.*/)?`.
    Dirs,
    /// A trailing `/**`: `.+`.
    Rest,
}

#[derive(Debug, Clone, PartialEq)]
struct Pattern {
    tokens: Vec<Token>,
}

/// What the page escaped before putting a character in a class: an escaped
/// `-` is no range.
fn special(c: char) -> bool {
    "\\^$.*+?()[]{}|/-".contains(c)
}

/// A character class at `at`, and where it ends; none if it never closes or
/// is no class a regular expression takes.
fn char_class(glob: &[char], at: usize) -> Option<Result<(Token, usize), ()>> {
    let mut i = at + 1;
    let mut negated = false;
    // Its characters, and whether each is an escaped one.
    let mut atoms: Vec<(char, bool)> = Vec::new();
    if matches!(glob.get(i), Some('!' | '^')) {
        negated = true;
        i += 1;
    }
    if glob.get(i) == Some(&']') {
        atoms.push((']', true));
        i += 1;
    }
    let mut end = None;
    while i < glob.len() {
        let c = glob[i];
        if c == ']' {
            end = Some(i);
            break;
        }
        if c == '\\' && i + 1 < glob.len() {
            i += 1;
            atoms.push((glob[i], special(glob[i])));
        } else if c == '[' {
            atoms.push(('[', true));
        } else {
            atoms.push((c, false));
        }
        i += 1;
    }
    let end = end?;
    let mut members = Vec::new();
    let mut k = 0;
    while k < atoms.len() {
        let (c, _) = atoms[k];
        if k + 2 < atoms.len() && atoms[k + 1] == ('-', false) {
            let to = atoms[k + 2].0;
            if to < c {
                // A range out of order: the page's RegExp threw, and the rule went.
                return Some(Err(()));
            }
            members.push(Member::Range(c, to));
            k += 3;
        } else {
            members.push(Member::One(c));
            k += 1;
        }
    }
    Some(Ok((Token::Class { negated, members }, end)))
}

fn compile(glob: &str) -> Option<Pattern> {
    let anchored = glob.contains('/');
    let body: Vec<char> = glob.strip_prefix('/').unwrap_or(glob).chars().collect();
    let mut tokens = Vec::new();
    if !anchored {
        tokens.push(Token::Dirs);
    }
    let mut i = 0;
    while i < body.len() {
        let c = body[i];
        if c == '*' && body.get(i + 1) == Some(&'*') && (i == 0 || body[i - 1] == '/') {
            if body.get(i + 2) == Some(&'/') {
                tokens.push(Token::Dirs);
                i += 3;
                continue;
            }
            if i + 2 == body.len() {
                tokens.push(Token::Rest);
                i += 2;
                continue;
            }
        }
        if c == '*' {
            while body.get(i + 1) == Some(&'*') {
                i += 1;
            }
            tokens.push(Token::Star);
        } else if c == '?' {
            tokens.push(Token::One);
        } else if c == '\\' && i + 1 < body.len() {
            i += 1;
            tokens.push(Token::Literal(body[i]));
        } else if c == '[' {
            match char_class(&body, i) {
                Some(Ok((class, end))) => {
                    tokens.push(class);
                    i = end;
                }
                // Never closes: git's wildmatch matches nothing.
                None | Some(Err(())) => return None,
            }
        } else {
            tokens.push(Token::Literal(c));
        }
        i += 1;
    }
    Some(Pattern { tokens })
}

impl Pattern {
    fn matches(&self, path: &str) -> bool {
        let chars: Vec<char> = path.chars().collect();
        let mut memo = vec![None; (self.tokens.len() + 1) * (chars.len() + 1)];
        self.from(0, 0, &chars, &mut memo)
    }

    fn from(&self, k: usize, j: usize, s: &[char], memo: &mut Vec<Option<bool>>) -> bool {
        let slot = k * (s.len() + 1) + j;
        if let Some(known) = memo[slot] {
            return known;
        }
        let answer = match self.tokens.get(k) {
            None => j == s.len(),
            Some(Token::Literal(c)) => s.get(j) == Some(c) && self.from(k + 1, j + 1, s, memo),
            Some(Token::One) => {
                s.get(j).is_some_and(|c| *c != '/') && self.from(k + 1, j + 1, s, memo)
            }
            Some(Token::Class { negated, members }) => {
                s.get(j).is_some_and(|c| {
                    let inside = members.iter().any(|member| match member {
                        Member::One(one) => c == one,
                        Member::Range(low, high) => (low..=high).contains(&c),
                    });
                    inside != *negated
                }) && self.from(k + 1, j + 1, s, memo)
            }
            Some(Token::Star) => {
                let mut end = j;
                loop {
                    if self.from(k + 1, end, s, memo) {
                        break true;
                    }
                    if end >= s.len() || s[end] == '/' {
                        break false;
                    }
                    end += 1;
                }
            }
            Some(Token::Dirs) => {
                self.from(k + 1, j, s, memo)
                    || (j..s.len())
                        .take_while(|at| !js::is_line_terminator(s[*at]))
                        .any(|at| s[at] == '/' && self.from(k + 1, at + 1, s, memo))
            }
            Some(Token::Rest) => (j + 1..=s.len()).any(|end| {
                !s[j..end].iter().any(|c| js::is_line_terminator(*c))
                    && self.from(k + 1, end, s, memo)
            }),
        };
        memo[slot] = Some(answer);
        answer
    }
}

struct Rule {
    negated: bool,
    dir_only: bool,
    pattern: Pattern,
}

/// The rules of one .gitignore, in order.
fn rules_of(text: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    for raw in text.split("\r\n").flat_map(|part| part.split(['\n', '\r'])) {
        let chars: Vec<char> = raw.chars().collect();
        // Trailing blanks do not count, unless the last is escaped.
        let mut end = chars.len();
        while end > 0 && (chars[end - 1] == ' ' || chars[end - 1] == '\t') {
            end -= 1;
        }
        if end < chars.len() && end > 0 && chars[end - 1] == '\\' {
            end += 1;
        }
        let line: String = chars[..end].iter().collect();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (negated, line) = match line.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, line.as_str()),
        };
        let (dir_only, line) = match line.strip_suffix('/') {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        if line.is_empty() {
            continue;
        }
        if let Some(pattern) = compile(line) {
            rules.push(Rule {
                negated,
                dir_only,
                pattern,
            });
        }
    }
    rules
}

/// A .gitignore and the folder it is in: `""` for the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub dir: String,
    pub text: String,
}

struct Compiled {
    dir: String,
    rules: Vec<Rule>,
}

/// What the sources say of this path: the last rule that matches wins,
/// deeper files last.
fn decision(sources: &[Compiled], path: &str, is_dir: bool) -> bool {
    let mut ignored = false;
    for source in sources {
        let inside = if source.dir.is_empty() {
            path
        } else {
            match path.strip_prefix(&format!("{}/", source.dir)) {
                Some(inside) => inside,
                None => continue,
            }
        };
        for rule in &source.rules {
            if (!rule.dir_only || is_dir) && rule.pattern.matches(inside) {
                ignored = !rule.negated;
            }
        }
    }
    ignored
}

/// `sources`, root first; `path` relative to the root, with `/`.
pub fn is_ignored(sources: &[Source], path: &str) -> bool {
    let compiled: Vec<Compiled> = sources
        .iter()
        .map(|source| Compiled {
            dir: source.dir.clone(),
            rules: rules_of(&source.text),
        })
        .collect();
    let parts: Vec<&str> = path.split('/').collect();
    // A file cannot be brought back when one of its folders is ignored.
    for i in 1..parts.len() {
        if decision(&compiled, &parts[..i].join("/"), true) {
            return true;
        }
    }
    decision(&compiled, path, false)
}

/// The .gitignore files that have a say on this path, root first: `(dir,
/// path)`.
pub fn sources_for(path: &str) -> Vec<(String, String)> {
    let parts: Vec<&str> = path.split('/').collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    let mut out = vec![(String::new(), ".gitignore".to_owned())];
    for i in 1..=dirs.len() {
        let dir = dirs[..i].join("/");
        out.push((dir.clone(), format!("{dir}/.gitignore")));
    }
    out
}

/// The line that ignores this very file, for the root's .gitignore.
pub fn pattern_for(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len() + 4);
    for c in path.chars() {
        if matches!(c, '\\' | '*' | '?' | '[') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    let spaces = escaped.len() - escaped.trim_end_matches(' ').len();
    if spaces > 0 {
        escaped.truncate(escaped.len() - spaces);
        escaped.push_str(&"\\ ".repeat(spaces));
    }
    if escaped.starts_with(['#', '!']) {
        format!("\\{escaped}")
    } else {
        escaped
    }
}

/// The text with that line added, in the file's line endings.
pub fn with_pattern(text: &str, pattern: &str) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let closed = if text.is_empty() || text.ends_with(['\r', '\n']) {
        text.to_owned()
    } else {
        format!("{text}{eol}")
    };
    format!("{closed}{pattern}{eol}")
}
