//! Lines compared, for the diff before saving and the conflict view, and the
//! line endings an editor takes away, given back. diff.js, ported.

use std::collections::HashMap;

/// Beyond this the changed block is shown as replaced whole.
const LCS_CELLS: usize = 4_000_000;
const CONTEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Same,
    Del,
    Add,
}

/// A line of the comparison, with its number on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub kind: Kind,
    pub text: String,
    pub a: Option<usize>,
    pub b: Option<usize>,
}

/// A line, or unchanged lines folded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hunk {
    Line(Op),
    Skip(usize),
}

impl Hunk {
    pub fn is_change(&self) -> bool {
        matches!(self, Hunk::Line(op) if op.kind != Kind::Same)
    }
}

/// What an editor does to a text: it only ever holds `\n`.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn middle<'a>(a: &[&'a str], b: &[&'a str], offset: usize, out: &mut Vec<Op>) {
    let (n, m) = (a.len(), b.len());
    let del = |i: usize| Op {
        kind: Kind::Del,
        text: a[i].to_owned(),
        a: Some(offset + i + 1),
        b: None,
    };
    let add = |j: usize| Op {
        kind: Kind::Add,
        text: b[j].to_owned(),
        a: None,
        b: Some(offset + j + 1),
    };
    if n == 0 || m == 0 || n.saturating_mul(m) > LCS_CELLS {
        out.extend((0..n).map(del));
        out.extend((0..m).map(add));
        return;
    }
    // Lines as numbers, so that each cell compares two integers.
    let mut ids: HashMap<&'a str, u32> = HashMap::new();
    let mut id = |line: &'a str| {
        let next = ids.len() as u32;
        *ids.entry(line).or_insert(next)
    };
    let a_ids: Vec<u32> = a.iter().map(|line| id(line)).collect();
    let b_ids: Vec<u32> = b.iter().map(|line| id(line)).collect();
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if a_ids[i] == b_ids[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a_ids[i] == b_ids[j] {
            out.push(Op {
                kind: Kind::Same,
                text: a[i].to_owned(),
                a: Some(offset + i + 1),
                b: Some(offset + j + 1),
            });
            i += 1;
            j += 1;
        } else if j >= m || (i < n && table[(i + 1) * width + j] >= table[i * width + j + 1]) {
            out.push(del(i));
            i += 1;
        } else {
            out.push(add(j));
            j += 1;
        }
    }
}

/// Every line of `before` and `after` (both `\n` only), in order: same, del
/// or add, with its numbers.
pub fn diff_lines(before: &str, after: &str) -> Vec<Op> {
    let a: Vec<&str> = before.split('\n').collect();
    let b: Vec<&str> = after.split('\n').collect();
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < a.len() - head
        && tail < b.len() - head
        && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
    {
        tail += 1;
    }
    let mut out = Vec::with_capacity(a.len().max(b.len()));
    for (i, line) in a.iter().enumerate().take(head) {
        out.push(Op {
            kind: Kind::Same,
            text: (*line).to_owned(),
            a: Some(i + 1),
            b: Some(i + 1),
        });
    }
    middle(
        &a[head..a.len() - tail],
        &b[head..b.len() - tail],
        head,
        &mut out,
    );
    for i in (1..=tail).rev() {
        out.push(Op {
            kind: Kind::Same,
            text: a[a.len() - i].to_owned(),
            a: Some(a.len() - i + 1),
            b: Some(b.len() - i + 1),
        });
    }
    out
}

/// The same, unchanged stretches folded into skips.
pub fn hunks(ops: Vec<Op>) -> Vec<Hunk> {
    let mut near = vec![false; ops.len()];
    for (index, op) in ops.iter().enumerate() {
        if op.kind == Kind::Same {
            continue;
        }
        let end = (index + CONTEXT).min(ops.len() - 1);
        for flag in &mut near[index.saturating_sub(CONTEXT)..=end] {
            *flag = true;
        }
    }
    let mut out = Vec::new();
    let mut skipped = 0;
    for (op, near) in ops.into_iter().zip(near) {
        if near {
            if skipped > 0 {
                out.push(Hunk::Skip(skipped));
            }
            skipped = 0;
            out.push(Hunk::Line(op));
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        out.push(Hunk::Skip(skipped));
    }
    out
}

pub fn has_changes(ops: &[Op]) -> bool {
    ops.iter().any(|op| op.kind != Kind::Same)
}

/// `edited` (`\n` only) with the line endings of `original` back: each line
/// that did not change keeps its own, and a new one takes the file's most
/// common.
pub fn restore_eols(original: &str, edited: &str) -> String {
    let bytes = original.as_bytes();
    let mut eols: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                eols.push("\r\n");
                i += 1;
            }
            b'\r' => eols.push("\r"),
            b'\n' => eols.push("\n"),
            _ => {}
        }
        i += 1;
    }
    let count = |eol: &str| eols.iter().filter(|other| **other == eol).count();
    let counts = [
        ("\n", count("\n")),
        ("\r\n", count("\r\n")),
        ("\r", count("\r")),
    ];
    if counts[0].1 == eols.len() {
        return edited.to_owned();
    }
    if counts[1].1 == eols.len() {
        return edited.replace('\n', "\r\n");
    }
    let mut common = counts[0];
    for each in &counts[1..] {
        if each.1 > common.1 {
            common = *each;
        }
    }
    let lines: Vec<&str> = edited.split('\n').collect();
    let mut kept: Vec<Option<&str>> = vec![None; lines.len()];
    for op in diff_lines(&normalize(original), edited) {
        if let (Kind::Same, Some(a), Some(b)) = (op.kind, op.a, op.b) {
            kept[b - 1] = eols.get(a - 1).copied();
        }
    }
    let mut out = String::with_capacity(edited.len() + lines.len());
    for (index, line) in lines.iter().enumerate() {
        out.push_str(line);
        if index + 1 < lines.len() {
            out.push_str(kept[index].unwrap_or(common.0));
        }
    }
    out
}
