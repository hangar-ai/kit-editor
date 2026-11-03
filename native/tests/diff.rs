//! test/diff.test.js, ported.

use std::time::Instant;

use kit_editor_tab::diff::{self, Hunk, Kind};

fn changes(before: &str, after: &str) -> Vec<(Kind, String)> {
    diff::diff_lines(before, after)
        .into_iter()
        .filter(|op| op.kind != Kind::Same)
        .map(|op| (op.kind, op.text))
        .collect()
}

fn ch(kind: Kind, text: &str) -> (Kind, String) {
    (kind, text.to_owned())
}

#[test]
fn what_changed_line_by_line_with_both_numberings() {
    assert!(changes("a\nb\nc", "a\nb\nc").is_empty());
    assert_eq!(
        changes("a\nb\nc", "a\nX\nc"),
        [ch(Kind::Del, "b"), ch(Kind::Add, "X")]
    );
    assert_eq!(changes("a\nc", "a\nb\nc"), [ch(Kind::Add, "b")]);
    assert_eq!(changes("a\nb\nc", "a\nc"), [ch(Kind::Del, "b")]);
    assert_eq!(changes("", "a\n"), [ch(Kind::Add, "a")]);
    let ops: Vec<(Kind, Option<usize>, Option<usize>)> = diff::diff_lines("a\nb\nc", "a\nX\nY\nc")
        .into_iter()
        .map(|op| (op.kind, op.a, op.b))
        .collect();
    assert_eq!(
        ops,
        [
            (Kind::Same, Some(1), Some(1)),
            (Kind::Del, Some(2), None),
            (Kind::Add, None, Some(2)),
            (Kind::Add, None, Some(3)),
            (Kind::Same, Some(3), Some(4)),
        ]
    );
}

#[test]
fn lines_moved_in_the_middle_of_a_block_are_found() {
    assert_eq!(
        changes("1\n2\n3\n4\n5\n6", "1\n3\n4\nnew\n5\n6"),
        [ch(Kind::Del, "2"), ch(Kind::Add, "new")]
    );
}

#[test]
fn a_change_too_large_to_compare_is_shown_replaced_not_lost() {
    let big = |prefix: &str| {
        (0..3000)
            .map(|i| format!("{prefix}{i}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let ops = diff::diff_lines(&big("a"), &big("b"));
    assert_eq!(ops.iter().filter(|op| op.kind == Kind::Del).count(), 3000);
    assert_eq!(ops.iter().filter(|op| op.kind == Kind::Add).count(), 3000);
}

#[test]
fn a_small_change_in_a_large_file_is_cheap() {
    let lines: Vec<String> = (0..50000).map(|i| format!("line {i}")).collect();
    let mut changed = lines.clone();
    changed[25000] = "changed".into();
    let started = Instant::now();
    let ops = diff::diff_lines(&lines.join("\n"), &changed.join("\n"));
    assert!(started.elapsed().as_millis() < 1000);
    let texts: Vec<String> = ops
        .into_iter()
        .filter(|op| op.kind != Kind::Same)
        .map(|op| op.text)
        .collect();
    assert_eq!(texts, ["line 25000", "changed"]);
}

#[test]
fn unchanged_stretches_fold_around_the_changes() {
    let lines = (0..20)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let changed = lines.replace("\n10\n", "\nten\n");
    let hunks = diff::hunks(diff::diff_lines(&lines, &changed));
    let shown: Vec<String> = hunks
        .iter()
        .map(|hunk| match hunk {
            Hunk::Skip(count) => format!("skip {count}"),
            Hunk::Line(op) => op.text.clone(),
        })
        .collect();
    assert_eq!(
        shown,
        [
            "skip 7", "7", "8", "9", "10", "ten", "11", "12", "13", "skip 6"
        ]
    );
    assert!(!diff::has_changes(&diff::diff_lines("a", "a")));
    assert!(diff::has_changes(&diff::diff_lines("a", "b")));
    assert_eq!(
        diff::hunks(diff::diff_lines("a\nb", "a\nb")),
        [Hunk::Skip(2)]
    );
}

#[test]
fn what_an_editor_takes_away_comes_back() {
    assert_eq!(diff::normalize("a\r\nb\rc\n"), "a\nb\nc\n");
    assert_eq!(diff::restore_eols("a\nb\n", "a\nc\n"), "a\nc\n");
    assert_eq!(
        diff::restore_eols("a\r\nb\r\n", "a\nc\nd\n"),
        "a\r\nc\r\nd\r\n"
    );
    assert_eq!(
        diff::restore_eols("no end of line", "no end\nof line"),
        "no end\nof line"
    );
    assert_eq!(diff::restore_eols("", "new\n"), "new\n");
    assert_eq!(
        diff::restore_eols("a\r\nb\nc\r\n", "a\nb\nc\n"),
        "a\r\nb\nc\r\n",
        "mixed, untouched"
    );
    assert_eq!(
        diff::restore_eols("a\r\nb\nc\r\n", "a\nb\nnew\nc\n"),
        "a\r\nb\nnew\r\nc\r\n",
        "a new line takes the common one"
    );
    assert_eq!(diff::restore_eols("a\rb\r", "a\nb\nc\n"), "a\rb\rc\r");
    assert_eq!(
        diff::restore_eols("a\r\nb", "a\nb\nc"),
        "a\r\nb\r\nc",
        "the old last line had none of its own"
    );
}
