//! Comments, order and every untouched byte survive a save; a value is a
//! value, even over several lines, and is never read as a key.
//! the HTML tab's Node test of the same name, ported before the page was deleted.

use kit_editor_tab::dotenv;

fn value(doc: &dotenv::Doc, key: &str) -> Option<String> {
    dotenv::rows(doc)
        .into_iter()
        .find(|row| row.key == key)
        .map(|row| row.value)
}

fn keys(doc: &dotenv::Doc) -> Vec<String> {
    dotenv::rows(doc).into_iter().map(|row| row.key).collect()
}

fn edit(text: &str, edits: &[(&str, &str)]) -> String {
    let edits: Vec<(String, String)> = edits
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    dotenv::serialize(&dotenv::with_values(&dotenv::parse(text), &edits))
}

const SHAPES: &[(&str, &str)] = &[
    ("empty", ""),
    ("one line, no end of line", "A=1"),
    ("plain", "# head\n\nA=1\nB=2\n"),
    ("crlf", "# head\r\n\r\nA=1\r\nB=2\r\n"),
    ("mixed endings", "A=1\r\nB=2\nC=3\r\n"),
    ("lone CR", "A=1\rB=2\r"),
    ("exported", "export A=1\n  export   B = 2\n"),
    ("spaced", "A = 1\nB= 2\nC =3\n\tD=4\n"),
    ("quotes", "A=\"1\"\nB='2'\nC=`3`\nD=\"\"\nE=''\n"),
    (
        "inline comments",
        "A=1 # one\nB=\"2\" # two\nC= # three\nD=a#b\n",
    ),
    (
        "several lines",
        "A=\"one\ntwo\nB=inside\"\nC='x\ny=z'\nD=1\n",
    ),
    ("CRLF inside a value", "A='one\r\ntwo'\r\nB=2\r\n"),
    ("unclosed quotes", "A=\"open\nB=2\nC='open\nD=4\n"),
    (
        "escaped quotes",
        "A=\"say \\\"hi\\\"\"\nB='it\\'s'\nC=\"back\\\\\"\n",
    ),
    (
        "not understood",
        "just-a-token\nA=1\n1BAD=2\nKEY: value\n=lonely\n",
    ),
    ("duplicates", "A=1\n# again\nA=2\nB=3\nA=4\n"),
    ("blank lines at the end", "A=1\n\n\n"),
    ("trailing blanks", "A=1   \nB=2\t\n"),
    ("unicode", "NOM=Éléonore\nÉTÉ=chaud\nA=✓\n"),
    ("equals in a value", "A=b=c==\nURL=https://x.y/?a=1&b=2\n"),
];

fn shape(name: &str) -> &'static str {
    SHAPES.iter().find(|(other, _)| *other == name).unwrap().1
}

#[test]
fn read_and_written_back_unchanged() {
    for (name, text) in SHAPES {
        assert_eq!(dotenv::serialize(&dotenv::parse(text)), *text, "{name}");
        assert_eq!(edit(text, &[]), *text, "{name}");
    }
}

#[test]
fn values_are_read_as_parsers_read_them() {
    let inline = shape("inline comments")
        .replace("A=", "F=")
        .replace("B=", "G=")
        .replace("C=", "H=")
        .replace("D=", "I=");
    let doc = dotenv::parse(&format!("{}{inline}", shape("quotes")));
    assert_eq!(value(&doc, "A").as_deref(), Some("1"));
    assert_eq!(value(&doc, "B").as_deref(), Some("2"));
    assert_eq!(value(&doc, "C").as_deref(), Some("3"));
    assert_eq!(value(&doc, "D").as_deref(), Some(""));
    assert_eq!(value(&doc, "E").as_deref(), Some(""));
    assert_eq!(
        value(&doc, "F").as_deref(),
        Some("1"),
        "a comment after a blank is no part of the value"
    );
    assert_eq!(value(&doc, "G").as_deref(), Some("2"));
    assert_eq!(
        value(&doc, "H").as_deref(),
        Some(""),
        "nothing but a comment"
    );
    assert_eq!(
        value(&doc, "I").as_deref(),
        Some("a#b"),
        "a # inside a word is part of it"
    );
    assert_eq!(
        value(&dotenv::parse(shape("exported")), "B").as_deref(),
        Some("2")
    );
    assert_eq!(
        value(&dotenv::parse(shape("spaced")), "D").as_deref(),
        Some("4")
    );
    assert_eq!(
        value(&dotenv::parse(shape("equals in a value")), "A").as_deref(),
        Some("b=c==")
    );
    let escaped = dotenv::parse(shape("escaped quotes"));
    assert_eq!(value(&escaped, "A").as_deref(), Some("say \"hi\""));
    assert_eq!(value(&escaped, "B").as_deref(), Some("it's"));
    assert_eq!(value(&escaped, "C").as_deref(), Some("back\\"));
}

#[test]
fn what_is_inside_a_multi_line_value_is_no_key_and_an_unclosed_quote_swallows_nothing() {
    let several = dotenv::parse(shape("several lines"));
    assert_eq!(
        keys(&several),
        ["A", "C", "D"],
        "B=inside and y=z belong to values"
    );
    assert_eq!(value(&several, "A").as_deref(), Some("one\ntwo\nB=inside"));
    assert_eq!(value(&several, "C").as_deref(), Some("x\ny=z"));
    let unclosed = dotenv::parse(shape("unclosed quotes"));
    assert_eq!(keys(&unclosed), ["A", "B", "C", "D"]);
    assert_eq!(value(&unclosed, "A").as_deref(), Some("\"open"));
}

#[test]
fn a_private_key_over_several_lines_none_of_its_lines_is_a_key() {
    let pem = "-----BEGIN KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQ=\nabcDEF123=\n-----END KEY-----";
    let doc = dotenv::parse(&format!("A=1\nPRIVATE_KEY=\"{pem}\"\nB=2\n"));
    assert_eq!(keys(&doc), ["A", "PRIVATE_KEY", "B"]);
    assert_eq!(value(&doc, "PRIVATE_KEY").as_deref(), Some(pem));
}

#[test]
fn a_line_that_is_no_pair_is_no_key_and_is_counted_to_say_so() {
    let doc = dotenv::parse(shape("not understood"));
    assert_eq!(keys(&doc), ["A"]);
    assert_eq!(dotenv::other_lines(&doc), 4);
    assert_eq!(dotenv::other_lines(&dotenv::parse("# comment\n\nA=1\n")), 0);
}

#[test]
fn setting_a_value_changes_that_line_and_no_other() {
    assert_eq!(
        edit(
            "# head\n\nA=1\n\n# about b\nB=2 # inline\n# tail\n",
            &[("B", "new")]
        ),
        "# head\n\nA=1\n\n# about b\nB=new # inline\n# tail\n"
    );
    assert_eq!(edit("export A=1\n", &[("A", "2")]), "export A=2\n");
    assert_eq!(edit("  A = 1\n", &[("A", "2")]), "  A = 2\n");
    assert_eq!(
        edit("A=\"1\"\n", &[("A", "2")]),
        "A=\"2\"\n",
        "its quotes are kept when nothing needs them changed"
    );
    assert_eq!(edit("A='1'\r\nB=2\r\n", &[("A", "x")]), "A='x'\r\nB=2\r\n");
    assert_eq!(edit("A= # empty\n", &[("A", "x")]), "A= x # empty\n");
    assert_eq!(
        edit("A=\"one\ntwo\" # c\nB=1\n", &[("A", "x")]),
        "A=\"x\" # c\nB=1\n"
    );
    assert_eq!(edit("A=1\n", &[("A", "1")]), "A=1\n");
}

#[test]
fn a_new_key_goes_at_the_end_in_the_files_line_endings_and_nothing_moves() {
    assert_eq!(edit("A=1\n", &[("Z", "9")]), "A=1\nZ=9\n");
    assert_eq!(
        edit("A=1", &[("Z", "9")]),
        "A=1\nZ=9\n",
        "the last line gets its end of line first"
    );
    assert_eq!(
        edit("A=1\r\nB=2\r\n", &[("Z", "9")]),
        "A=1\r\nB=2\r\nZ=9\r\n"
    );
    assert_eq!(edit("", &[("Z", "9")]), "Z=9\n");
    assert_eq!(
        edit("# only a comment\n", &[("Z", "9"), ("Y", "8")]),
        "# only a comment\nZ=9\nY=8\n"
    );
}

#[test]
fn a_key_defined_twice_shows_once_and_the_last_line_is_the_one_read_and_edited() {
    let doc = dotenv::parse(shape("duplicates"));
    let a = dotenv::rows(&doc)
        .into_iter()
        .find(|row| row.key == "A")
        .unwrap();
    assert_eq!(a.value, "4");
    assert_eq!(a.count, 3);
    assert_eq!(
        edit(shape("duplicates"), &[("A", "x")]),
        "A=1\n# again\nA=2\nB=3\nA=x\n"
    );
}

#[test]
fn any_value_is_read_back_the_same_once_written() {
    let values = [
        "plain",
        "with space",
        " leading",
        "trailing ",
        "#hash",
        "a #b",
        "it's",
        "say \"hi\"",
        "both ' and \"",
        "back\\slash",
        "ends\\",
        "$HOME",
        "${HOME}",
        "`tick`",
        "two\nlines",
        "two\r\nlines",
        "it's\ntwo",
        "a=b",
        "é✓",
        "=",
        "'",
        "\"",
        "\\",
        "\\n",
        "'\\n'",
        "tab\there",
    ];
    for original in ["A=1\n", "A=\"1\"\n", "A='1'\n", "export A = 1 # c\n"] {
        for wanted in values {
            let text = edit(original, &[("A", wanted), ("B", wanted)]);
            let doc = dotenv::parse(&text);
            assert_eq!(
                value(&doc, "A").as_deref(),
                Some(wanted),
                "{wanted:?} over {original:?}: {text:?}"
            );
            assert_eq!(
                value(&doc, "B").as_deref(),
                Some(wanted),
                "{wanted:?} as a new key: {text:?}"
            );
            assert_eq!(keys(&doc), ["A", "B"], "{text:?}");
        }
    }
}

#[test]
fn the_comment_above_a_key_is_its_hint_but_a_value_put_aside_in_a_comment_is_not() {
    let rows = dotenv::rows(&dotenv::parse(
        "# Get one at the dashboard\n# (free)\nA=1\n\n# unrelated\n\nB=2\n# OLD_TOKEN=abc123\n#export C=1\nC=3\n",
    ));
    assert_eq!(rows[0].hint, "Get one at the dashboard (free)");
    assert_eq!(rows[1].hint, "", "a blank line cuts the link");
    assert_eq!(rows[2].hint, "");
}

#[test]
fn which_keys_changed_names_only() {
    let changed = dotenv::changed_keys(
        &dotenv::parse("A=1\nB=2\nC=\nD=4\n"),
        &dotenv::parse("A=1\nB=x\nC=y\nD=\nE=z\nF=\n"),
    );
    assert_eq!(changed.set, ["B", "C", "E"]);
    assert_eq!(changed.emptied, ["D"]);
}

#[test]
fn what_counts_as_a_dotenv_file_and_what_as_a_template() {
    for path in [
        ".env",
        ".env.local",
        "config/.env.production",
        "app.env",
        "a/b/prod.ENV",
        ".env.example",
    ] {
        assert!(dotenv::is_dotenv_path(path), "{path}");
    }
    for path in [
        ".envrc",
        "env",
        "env.json",
        "dotenv",
        ".environment",
        "config.yaml",
        ".env/file",
    ] {
        assert!(!dotenv::is_dotenv_path(path), "{path}");
    }
    assert!(dotenv::is_example_path("sub/.env.example"));
    assert!(dotenv::is_example_path(".env.sample"));
    assert!(!dotenv::is_example_path(".env"));
    assert!(!dotenv::is_example_path(".env.local"));
    assert_eq!(
        dotenv::example_candidates("sub/.env.local")[..3],
        [
            "sub/.env.local.example",
            "sub/.env.local.sample",
            "sub/.env.example"
        ]
    );
    assert!(!dotenv::example_candidates(".env.example").contains(&".env.example".to_owned()));
}

#[test]
fn no_text_makes_the_parser_panic() {
    // A panic's message is logged, and Rust's for a cut inside a character
    // quotes the text: none may happen whatever a file holds.
    let pieces = [
        "A", "=", "\"", "'", "`", "\\", "#", " ", "\t", "\n", "\r", "é", "😀", "export ", "x",
    ];
    let mut seed: u32 = 7;
    for _ in 0..3000 {
        let mut text = String::new();
        for _ in 0..(seed % 24) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            text.push_str(pieces[(seed as usize) % pieces.len()]);
        }
        let doc = dotenv::parse(&text);
        assert_eq!(dotenv::serialize(&doc), text);
        let rows = dotenv::rows(&doc);
        let edits: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.key.clone(), format!("{}é\n'\"", row.value)))
            .collect();
        let written = dotenv::serialize(&dotenv::with_values(&doc, &edits));
        let back = dotenv::parse(&written);
        for (key, wanted) in &edits {
            assert_eq!(
                value(&back, key).as_ref(),
                Some(wanted),
                "{text:?} → {written:?}"
            );
        }
    }
}
