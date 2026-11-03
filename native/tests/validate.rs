//! JSON's answer is the grammar's. YAML is scanned and TOML parsed for what is
//! certainly wrong, so the first thing checked is that valid files, full of
//! traps, pass. test/validate.test.js, ported.

use std::path::Path;

use kit_editor_tab::validate::{self, Check, Checker};

fn valid(kind: Checker, partial: bool) -> Check {
    Check {
        kind: Some(kind),
        ok: true,
        partial,
        code: None,
        line: None,
        column: None,
    }
}

#[test]
fn which_check_applies() {
    let table: &[(&str, Option<Checker>)] = &[
        ("a.json", Some(Checker::Json)),
        ("pkg/package.json", Some(Checker::Json)),
        ("b.JSON", Some(Checker::Json)),
        ("tsconfig.json", Some(Checker::Jsonc)),
        ("tsconfig.app.json", Some(Checker::Jsonc)),
        (".vscode/settings.json", Some(Checker::Jsonc)),
        ("c.jsonc", Some(Checker::Jsonc)),
        ("w.code-workspace", Some(Checker::Jsonc)),
        ("a.yaml", Some(Checker::Yaml)),
        ("a.yml", Some(Checker::Yaml)),
        ("a.toml", Some(Checker::Toml)),
        ("Cargo.toml", Some(Checker::Toml)),
        ("a.txt", None),
        ("a.ini", None),
        ("a.json5", None),
        ("Dockerfile", None),
    ];
    for (path, kind) in table {
        assert_eq!(validate::checker_for(path), *kind, "{path}");
    }
}

#[test]
fn json_what_the_grammar_says_with_a_line() {
    assert_eq!(
        validate::validate("a.json", "{\"a\": [1, 2]}"),
        valid(Checker::Json, false)
    );
    let bad = validate::validate("a.json", "{\n  \"a\": 1,\n  \"b\": \n}\n");
    assert!(!bad.ok);
    assert!(!bad.partial);
    assert_eq!(bad.code, Some("json"));
    assert_eq!(bad.line, Some(4));
    assert!(
        !validate::validate("a.json", "").ok,
        "an empty file is no JSON"
    );
    assert!(!validate::validate("a.json", "{\"a\": 1} // no").ok);
    assert!(!validate::validate("a.json", "{\"a\": 1,}").ok);
}

#[test]
fn json_where_it_goes_wrong_is_found_and_agrees_with_json_parse_on_what_is_wrong() {
    let cases: &[(&str, usize, usize)] = &[
        ("{\"a\": 1,}", 1, 9),
        ("[1, 2\n", 2, 1),
        ("{\n\"a\" 1}", 2, 5),
        ("{\"a\": tru}", 1, 7),
        ("\"\\q\"", 1, 2),
        ("\"new\nline\"", 1, 5),
        ("1 2", 1, 3),
        ("{\"a\":01}", 1, 7),
        ("[1,\n2,\n,3]", 3, 1),
        ("nul", 1, 1),
    ];
    for (text, line, column) in cases {
        assert!(
            serde_json::from_str::<serde_json::Value>(text).is_err(),
            "{text}"
        );
        let result = validate::validate("a.json", text);
        assert!(!result.ok, "{text}");
        assert_eq!(
            (result.line, result.column),
            (Some(*line), Some(*column)),
            "{text}"
        );
    }
    for text in [
        "\"\\u00e9 \\\" \\n\"",
        "[[], {}, [{\"a\": [null, true, false, -1.5e+3, 0]}]]",
        " \n 1 \n",
    ] {
        assert!(validate::validate("a.json", text).ok, "{text}");
    }
    assert!(
        !validate::validate("a.json", &"[".repeat(200_000)).ok,
        "too deep is still not valid"
    );
    let deep = format!("{}{}", "[".repeat(200_000), "]".repeat(200_000));
    assert!(
        validate::validate("a.json", &deep).ok,
        "and as deep as it goes, valid is valid"
    );
}

#[test]
fn json_agrees_with_a_parser_on_many_texts() {
    let pieces = [
        "{",
        "}",
        "[",
        "]",
        ",",
        ":",
        "\"a\"",
        "1",
        "-0.5e3",
        "true",
        "null",
        " ",
        "\n",
        "\"\\u12\"",
        "01",
        "é",
    ];
    let mut seed: u32 = 99;
    for _ in 0..5000 {
        let mut text = String::new();
        for _ in 0..(seed % 12) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            text.push_str(pieces[(seed as usize) % pieces.len()]);
        }
        let parsed = serde_json::from_str::<serde_json::Value>(&text).is_ok();
        assert_eq!(validate::validate("a.json", &text).ok, parsed, "{text:?}");
    }
}

#[test]
fn jsonc_comments_and_trailing_commas_are_fine_and_strings_are_left_alone() {
    let good = "// head\n{\n  /* block\n     over lines */\n  \"url\": \"http://x.y/*z*/\", // tail\n  \"list\": [1, 2,],\n  \"o\": { \"a\": \"// not, a comment\", },\n}\n";
    assert!(validate::validate("tsconfig.json", good).ok);
    assert!(validate::validate(".vscode/settings.json", good).ok);
    assert!(
        !validate::validate("a.json", good).ok,
        "plain JSON has neither"
    );
    let bad = validate::validate("c.jsonc", "{\n  // fine\n  \"a\": 1\n  \"b\": 2\n}\n");
    assert!(!bad.ok);
    assert_eq!(bad.line, Some(4), "the lines are still the file's");
    assert!(validate::validate("c.jsonc", "{\"a\": \",\", }").ok);
    assert!(
        !validate::validate("c.jsonc", "[1,,]").ok,
        "only the last comma goes"
    );
}

const GOOD_YAML: &[(&str, &str)] = &[
    (
        "workflow",
        "name: CI
on:
  push:
    branches: [main, \"release/**\"]
env: { A: 1, B: \"x,y\" }
jobs:
  build:
    runs-on: ${{ matrix.os }}
    if: github.event_name == 'push' && contains(x, 'y')
    steps:
      - uses: actions/checkout@v4
      - name: Say \"hi\"   # a comment with a ' quote and a [ bracket
        run: |
          echo \"unterminated
          it's [fine { here
          \ttab inside a block scalar
      - run: >-
          folded 'text
        env:
          X: 'it''s'
          Y: \"a \\\" b\"
",
    ),
    (
        "plain",
        "key: it's fine
other: a \"quoted\" word
url: http://x.y:80/a#b
time: 12:30:00
list:
- a
- b: c
  d: [e, {f: g}]
- 'single'
- \"double\"
-   - nested
    - more
empty:
tilde: ~
glob: \"*.js\"
star: *anchor
amp: &anchor value
tagged: !!str \"text\"
tagged2: !custom [a, b]
? complex key
: complex value
",
    ),
    (
        "documents",
        "%YAML 1.2
---
a: 1
...
--- # second
b: 2
--- |
  text with 'a quote
--- \"quoted\"
",
    ),
    (
        "multiline",
        "a: \"first line
  second 'line\"
b: 'first
  second \"line'
c: [1,
  2,
\t3]
d: {
  e: f
}
plain: this goes on
  'over several lines
  and \"another
next: 1
",
    ),
    (
        "jsonish",
        "{\"a\": 1, \"b\": [true, null], \"c\": {\"d\": \"e\"}}\n",
    ),
    ("tabs", "a: 1\n\t\n  \t# a comment after a tab\nb: 2 \t\n"),
    ("empty", ""),
    ("crlf", "a: 1\r\nb:\r\n  - c\r\n"),
];

#[test]
fn valid_yaml_passes() {
    for (name, text) in GOOD_YAML {
        assert_eq!(
            validate::validate("a.yaml", text),
            valid(Checker::Yaml, true),
            "{name}"
        );
    }
}

#[test]
fn broken_yaml_what_is_certainly_wrong_with_its_line() {
    let cases: &[(&str, &str, usize)] = &[
        ("a: 1\n\tb: 2\n", "yaml-tab", 2),
        ("a:\n  - b\n \t- c\n", "yaml-tab", 3),
        ("a: \"never closed\nb: 2\n", "yaml-quote", 1),
        ("a: 1\nb: 'never closed\n", "yaml-quote", 2),
        ("a: [1, 2\nb: 3\n", "yaml-bracket", 1),
        ("a: {b: 1]\n", "yaml-bracket", 1),
        ("a: ]\n", "yaml-bracket", 1),
        ("a: @scope/pkg\n", "yaml-reserved", 1),
        ("a: `tick`\n", "yaml-reserved", 1),
    ];
    for (text, code, line) in cases {
        let result = validate::validate("a.yml", text);
        assert!(!result.ok, "{text:?}");
        assert_eq!(result.code, Some(*code), "{text:?}");
        assert_eq!(result.line, Some(*line), "{text:?}");
        assert!(result.partial);
    }
}

const GOOD_TOML: &[(&str, &str)] = &[
    (
        "everything",
        "# comment
title = \"TOML\" # trailing
\"quoted key\" = 1
'lit key' = 2
a.b.c = 3
d . e = 4
bare-key_1 = true
no = false
int = +1_000
hex = 0xDEAD_beef
oct = 0o755
bin = 0b1010
float = 6.626e-34
inf = -inf
nan = nan
date = 1979-05-27
dt = 1979-05-27T07:32:00Z
dt2 = 1979-05-27 07:32:00.5-07:00
time = 07:32:00
str = \"esc \\\" \\\\ \\t \\u00E9 \\U0001F600\"
lit = 'C:\\no\\escape'
multi = \"\"\"
line \\
  joined \"quotes\" \"\"
\"\"\"
multi2 = '''
raw 'x' ''
'''
arr = [1, 2, 3,]
arr2 = [
  \"a\", # comment
  [1, 2],

  { x = 1, y = \"z\" },
]
empty = []
inline = { a = 1, b.c = 2 }
none = {}

[table]
x = 1

[table.sub]
x = 2

[ spaced . \"name\" ]
x = 3

[[items]]
name = \"one\"

[[items]]
name = \"two\"

[other]
x = 4
",
    ),
    ("crlf", "a = 1\r\n# c\r\n[b]\r\nc = \"d\"\r\n"),
    ("empty", ""),
    ("no final newline", "a = 1"),
];

#[test]
fn valid_toml_passes() {
    for (name, text) in GOOD_TOML {
        assert_eq!(
            validate::validate("a.toml", text),
            valid(Checker::Toml, true),
            "{name}"
        );
    }
}

#[test]
fn broken_toml_with_its_line() {
    let cases: &[(&str, &str, usize)] = &[
        ("a = \n", "toml-value", 1),
        ("ok = 1\nb\n", "toml-equals", 2),
        ("a = 1 2\n", "toml-trailing", 1),
        ("a = \"open\n", "toml-string", 1),
        ("ok = 1\nb = \"bad \\q\"\n", "toml-escape", 2),
        ("= 1\n", "toml-key", 1),
        ("[table\nb = 1\n", "toml-header", 1),
        ("[[table]\n", "toml-header", 1),
        ("a = 1\na = 2\n", "toml-duplicate", 2),
        ("[t]\nx = 1\n[t]\ny = 2\n", "toml-duplicate", 3),
        ("[t]\nx = 1\nx = 2\n", "toml-duplicate", 3),
        ("a = [1, 2\n", "toml-value", 1),
        ("a = { b = 1,\n}\n", "toml-key", 1),
        ("a = nope\n", "toml-value", 1),
        ("a = 01\n", "toml-value", 1),
        ("a = 1_\n", "toml-value", 1),
        ("a = truex\n", "toml-value", 1),
        ("a = '''open\n", "toml-string", 1),
    ];
    for (text, code, line) in cases {
        let result = validate::validate("a.toml", text);
        assert!(!result.ok, "{text:?}");
        assert_eq!(result.code, Some(*code), "{text:?}");
        assert_eq!(result.line, Some(*line), "{text:?}");
    }
}

#[test]
fn the_same_key_in_two_tables_or_two_items_is_no_duplicate() {
    assert!(validate::validate("a.toml", "[a]\nx = 1\n[b]\nx = 2\n").ok);
    assert!(validate::validate("a.toml", "[[a]]\nx = 1\n[[a]]\nx = 2\n").ok);
}

#[test]
fn the_repositorys_own_files_pass_when_they_are_around() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for sample in [
        ".github/workflows/ci.yml",
        ".github/repo.json",
        "hangar-kit.json",
        "package.json",
        "native/Cargo.toml",
    ] {
        let file = root.join(sample);
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let result = validate::validate(sample, &text);
        assert!(result.ok, "{sample}: {result:?}");
    }
}

#[test]
fn no_check_for_other_files_nothing_can_be_said_so_nothing_is_claimed() {
    assert_eq!(
        validate::validate("a.txt", "whatever {{{"),
        Check {
            kind: None,
            ok: true,
            partial: false,
            code: None,
            line: None,
            column: None,
        }
    );
}

#[test]
fn no_text_makes_a_check_panic() {
    let pieces = [
        "\"", "'", "\\", "[", "]", "{", "}", "=", ":", "#", " ", "\t", "\n", "\r", "é", "😀", "a",
        "1", "-", ".", "'''", "\"\"\"",
    ];
    let mut seed: u32 = 3;
    for _ in 0..4000 {
        let mut text = String::new();
        for _ in 0..(seed % 20) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            text.push_str(pieces[(seed as usize) % pieces.len()]);
        }
        for path in ["a.json", "a.jsonc", "a.yaml", "a.toml"] {
            let _ = validate::validate(path, &text);
        }
    }
}
