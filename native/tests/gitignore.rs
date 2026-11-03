//! Git's rules, and git itself as the judge: the table below is run through
//! `git check-ignore` in a temporary repository when git is on the machine.
//! test/gitignore.test.js, ported.

use std::path::PathBuf;
use std::process::Command;

use kit_editor_tab::gitignore::{self, Source};

fn root(text: &str) -> Vec<Source> {
    vec![Source {
        dir: String::new(),
        text: text.to_owned(),
    }]
}

// (.gitignore, path, ignored?)
const TABLE: &[(&str, &str, bool)] = &[
    ("", ".env", false),
    (".env\n", ".env", true),
    (".env\n", "sub/.env", true),
    ("/.env\n", ".env", true),
    ("/.env\n", "sub/.env", false),
    ("sub/.env\n", "sub/.env", true),
    ("sub/.env\n", "other/sub/.env", false),
    (".env*\n", ".env.local", true),
    (".env*\n", "sub/.env", true),
    ("*.env\n", "app.env", true),
    ("*.env\n", ".env", true),
    ("*.env\n", "sub/app.env", true),
    (".env\n!.env\n", ".env", false),
    (".env*\n!.env.example\n", ".env.example", false),
    (".env*\n!.env.example\n", ".env.local", true),
    ("!.env\n.env\n", ".env", true),
    ("# .env\n", ".env", false),
    ("\\#x\n", "#x", true),
    ("\\!x\n", "!x", true),
    ("config/\n", "config/.env", true),
    ("config/\n", "a/config/.env", true),
    ("config/\n", "config", false),
    ("/config/\n", "a/config/.env", false),
    ("config\n", "config", true),
    ("config\n", "config/.env", true),
    ("config/*\n!config/.env\n", "config/.env", false),
    ("config/\n!config/.env\n", "config/.env", true),
    ("**/.env\n", "a/b/.env", true),
    ("**/.env\n", ".env", true),
    ("a/**/.env\n", "a/.env", true),
    ("a/**/.env\n", "a/b/c/.env", true),
    ("a/**\n", "a/b/.env", true),
    ("a/**\n", "a", false),
    ("*\n", "a/b", true),
    ("*\n!a/\n!a/b\n", "a/b", false),
    (".en?\n", ".env", true),
    (".en?\n", ".env/x", true),
    (".e[nm]v\n", ".env", true),
    (".e[!n]v\n", ".env", false),
    (".e[a-z]v\n", ".env", true),
    ("  .env\n", ".env", false),
    (".env  \n", ".env", true),
    (".env\r\n", ".env", true),
    ("a b\n", "a b", true),
    ("foo*bar\n", "foo/bar", false),
    ("a/*/b\n", "a/x/b", true),
    ("a/*/b\n", "a/x/y/b", false),
    ("[\n", "[", false),
];

#[test]
fn gits_rules() {
    for (text, file, expected) in TABLE {
        assert_eq!(
            gitignore::is_ignored(&root(text), file),
            *expected,
            "{text:?} on {file}"
        );
    }
}

#[test]
fn deeper_gitignore_files_have_the_last_word() {
    let sources = vec![
        Source {
            dir: String::new(),
            text: ".env\n".into(),
        },
        Source {
            dir: "sub".into(),
            text: "!.env\n".into(),
        },
    ];
    assert!(!gitignore::is_ignored(&sources, "sub/.env"));
    assert!(gitignore::is_ignored(&sources, ".env"));
    assert!(gitignore::is_ignored(&sources, "other/.env"));
    let sub = |text: &str| {
        vec![Source {
            dir: "sub".into(),
            text: text.into(),
        }]
    };
    assert!(gitignore::is_ignored(&sub("/x\n"), "sub/x"));
    assert!(!gitignore::is_ignored(&sub("/x\n"), "x"));
    assert!(!gitignore::is_ignored(&sub("/x\n"), "sub/y/x"));
}

#[test]
fn the_gitignore_files_that_have_a_say() {
    assert_eq!(
        gitignore::sources_for(".env"),
        [(String::new(), ".gitignore".to_owned())]
    );
    let paths: Vec<String> = gitignore::sources_for("a/b/.env")
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    assert_eq!(paths, [".gitignore", "a/.gitignore", "a/b/.gitignore"]);
}

#[test]
fn the_line_a_click_adds_ignores_that_very_file_whatever_its_name() {
    for file in [
        ".env",
        "sub/.env",
        "a b/.env",
        "we[ird]/.env",
        "st*r/.env",
        "#x/.env",
        "!x/.env",
        "back\\slash.env",
    ] {
        let pattern = gitignore::pattern_for(file);
        assert!(
            gitignore::is_ignored(&root(&format!("{pattern}\n")), file),
            "{file} by {pattern}"
        );
    }
    assert_eq!(gitignore::pattern_for(".env"), ".env");
    assert_eq!(gitignore::with_pattern("", ".env"), ".env\n");
    assert_eq!(gitignore::with_pattern("a\n", ".env"), "a\n.env\n");
    assert_eq!(gitignore::with_pattern("a", ".env"), "a\n.env\n");
    assert_eq!(
        gitignore::with_pattern("a\r\nb\r\n", ".env"),
        "a\r\nb\r\n.env\r\n"
    );
}

#[test]
fn ranges_out_of_order_and_odd_classes_match_nothing_as_the_page_did() {
    assert!(
        !gitignore::is_ignored(&root(".e[z-a]v\n"), ".env"),
        "the page's RegExp threw: no rule"
    );
    assert!(
        gitignore::is_ignored(&root(".e[]n]v\n"), ".env"),
        "a ] first is a character"
    );
    assert!(
        gitignore::is_ignored(&root(".e[a-]v\n"), ".e-v"),
        "a - last is a character"
    );
}

fn git(dir: &PathBuf, args: &[&str]) -> Option<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()
}

#[test]
fn git_agrees_with_the_table() {
    let temp = std::env::temp_dir();
    if git(&temp, &["--version"]).is_none_or(|out| !out.status.success()) {
        eprintln!("git is not installed: skipped");
        return;
    }
    for (index, (text, file, expected)) in TABLE.iter().enumerate() {
        // Names Windows cannot create are for the logic above, not for git here.
        if cfg!(windows) && file.contains(['*', '?', '[', ']']) {
            continue;
        }
        let dir = temp.join(format!("editor-kit-{}-{index}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).unwrap();
        std::fs::write(dir.join(".gitignore"), text).unwrap();
        let answer = git(
            &dir,
            &[
                "-c",
                "core.excludesFile=",
                "-c",
                "core.ignorecase=false",
                "check-ignore",
                "--no-index",
                "-q",
                file,
            ],
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            answer.status.success(),
            *expected,
            "git says otherwise of {text:?} on {file}"
        );
    }
}
