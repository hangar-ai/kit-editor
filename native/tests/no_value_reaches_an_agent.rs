//! The proof the kit stands on: whatever the file holds, whatever the person
//! types, however the save goes, no value reaches `panes.send`, any other call
//! but the file's own, or the tab's log. Every test drives the tab through its
//! page against a fake Hangar, marks every value with a sentinel, and looks at
//! what left the tab. test/no-value-reaches-an-agent.test.js, ported.

mod common;

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use common::{CALLER, Fake, Setup, Tab};
use hangar_kit_sdk::testing;
use hangar_kit_sdk::ui::Element;
use hangar_kit_sdk::ui::abi::Call;
use kit_editor_tab::answer;
use kit_editor_tab::editor::Screen;

/// A deterministic generator: a failure here can be run again as it is.
struct Random(u32);

impl Random {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    fn int(&mut self, max: u32) -> u32 {
        self.next() % max
    }

    fn pick<'a, T>(&mut self, list: &'a [T]) -> &'a T {
        &list[self.next() as usize % list.len()]
    }
}

fn base36(mut n: u32) -> String {
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(digits[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// What every message must look like: fixed sentences, a path, names,
/// counts. Nothing else fits.
static SHAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^The person (?:saved|checked) `[^`\n]+`(?:",
        r": the file changed\.|",
        r" and changed nothing\.(?: (?:Already set|Still empty): [A-Za-z_][\w.-]*(?:, [A-Za-z_][\w.-]*)*\.)*(?: \d+ other keys? set or empty\.)?(?: The values stay in the file: do not read them back, use them through the program that loads it\.)?|",
        r"\.(?: (?:Set|Emptied|Still empty): [A-Za-z_][\w.-]*(?:, [A-Za-z_][\w.-]*)*\.)*(?: \d+ other keys? changed or empty\.)? The values stay in the file: do not read them back, use them through the program that loads it\.",
        r")$",
    ))
    .unwrap()
});

/// What left the tab: every call answered, and what it logged.
fn assert_clean(calls: &[Call], logs: &[String], secrets: &[String], label: &str) -> Vec<String> {
    let sent: Vec<String> = calls
        .iter()
        .filter(|call| call.method == "panes.send")
        .map(|call| call.params["text"].as_str().unwrap_or("").to_owned())
        .collect();
    for text in &sent {
        assert!(
            SHAPE.is_match(text),
            "{label}: not one of the fixed sentences: {text}"
        );
        for secret in secrets {
            assert!(
                !text.contains(secret.as_str()),
                "{label}: a value reached panes.send: {secret} in {text}"
            );
        }
    }
    // Only the file calls may carry a value, and only to the file itself.
    for call in calls {
        if call.method == "files.write" || call.method == "files.read" {
            continue;
        }
        let seen = call.params.to_string();
        for secret in secrets {
            assert!(
                !seen.contains(secret.as_str()),
                "{label}: {secret} left in {}: {seen}",
                call.method
            );
        }
    }
    for line in logs {
        for secret in secrets {
            assert!(
                !line.contains(secret.as_str()),
                "{label}: {secret} reached the log: {line}"
            );
        }
    }
    sent
}

fn clean(tab: &Tab, secrets: &[String], label: &str) -> Vec<String> {
    let logs: Vec<String> = testing::take_logs()
        .into_iter()
        .map(|(_, line)| line)
        .collect();
    assert_clean(&tab.fake.calls, &logs, secrets, label)
}

fn s(n: impl std::fmt::Display) -> String {
    format!("s3cr3t-{n}-XqZ9")
}

/// Every way a value can sit in a dotenv file.
fn hostile(index: usize, v: &str) -> String {
    let u = v.replace('-', "_");
    match index {
        0 => format!("PLAIN={v}"),
        1 => format!("export EXPORTED={v}"),
        2 => format!("SPACED = {v}"),
        3 => format!("DOUBLE=\"{v}\""),
        4 => format!("SINGLE='{v}'"),
        5 => format!("TICK=`{v}`"),
        6 => format!("INLINE={v} # also {v}"),
        7 => format!("# COMMENTED={v}"),
        8 => format!("#{v}"),
        9 => v.to_owned(), // a token alone on its line
        10 => format!("MULTI=\"first {v}\nINNER_KEY={v}\nlast\""),
        11 => format!("MULTISINGLE='{v}\nOTHER={v}'"),
        12 => format!("UNCLOSED=\"{v}"),
        13 => format!("DUP={v}"),
        14 => format!("DUP={v}x"),
        15 => format!("COLON: {v}"),
        16 => format!("EQUALS={v}={v}=="),
        17 => format!("A{u}={u}"), // the value is also in the key's name
        18 => format!("{u}={u}"),  // a key that is its own value
        _ => format!("NOTE=see {u}"),
    }
}
const HOSTILE: usize = 20;

fn rows(tab: &Tab) -> Vec<(String, String)> {
    tab.kit()
        .form
        .as_ref()
        .map(|form| {
            form.rows
                .iter()
                .map(|row| (row.key.clone(), row.value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Adds a key the person typed, by Enter in its field.
fn add_new_key(tab: &mut Tab, name: &str) {
    tab.type_into("KEY_NAME", name);
    let submit = tab
        .elements()
        .into_iter()
        .find_map(|element| match element {
            Element::TextInput(input) if input.id == "new-key" => input.on_submit,
            _ => None,
        })
        .unwrap();
    tab.ui(submit, None);
}

fn offered(tab: &Tab) -> Vec<String> {
    tab.kit()
        .form
        .as_ref()
        .map(|form| form.offered.iter().map(|offer| offer.key.clone()).collect())
        .unwrap_or_default()
}

fn add_all_offered(tab: &mut Tab) {
    let keys = offered(tab);
    match keys.len() {
        0 => {}
        1 => tab.add_offered(&keys[0]),
        _ => tab.press("Add all"),
    }
}

/// Saves when there is something to save, or tells the agent there is not.
fn save_or_tell(tab: &mut Tab) {
    if tab.kit().is_dirty() {
        tab.press("Save");
    } else if tab
        .button("Tell Asker: nothing to change")
        .is_some_and(|button| button.on_press.is_some())
    {
        tab.press("Tell Asker: nothing to change");
    }
}

#[test]
fn every_hostile_line_one_by_one_the_answer_holds_no_value() {
    for index in 0..HOSTILE {
        let secret = s(index);
        let text = format!("# head\n{}\nTARGET=old\n", hostile(index, &secret));
        let mut tab = Setup::new()
            .path(".env")
            .param("keys", "TARGET,NEW_KEY")
            .caller(CALLER)
            .file(".env", &text)
            .ignored()
            .open();
        tab.edit_row("TARGET", &format!("{secret}-typed"));
        tab.edit_row("NEW_KEY", &format!("{secret}-new"));
        for (key, value) in rows(&tab) {
            tab.edit_row(&key, &format!("{value}-{secret}"));
        }
        tab.press("Save");
        let sent = clean(
            &tab,
            &[secret.clone(), secret.replace('-', "_")],
            &format!("line {index}"),
        );
        assert_eq!(sent.len(), 1, "line {index}: it answered once");
    }
}

#[test]
fn random_files_random_edits_400_of_them_the_answer_holds_no_value() {
    let mut random = Random(20261031);
    let eols = ["\n", "\r\n"];
    let keys = [
        "API_KEY", "DB_URL", "TOKEN", "PORT", "A.B", "x-y", "_z", "DEBUG",
    ];
    let mut conflicts = 0;
    let mut answers = 0;
    for run in 0..400 {
        let mut secrets: Vec<String> = Vec::new();
        let secret = |random: &mut Random, secrets: &mut Vec<String>| {
            let value = format!(
                "v{run}-{}-{}Q",
                secrets.len(),
                base36(random.int(1_000_000_000))
            );
            secrets.push(value.clone());
            value
        };
        let eol = *random.pick(&eols);
        let mut lines: Vec<String> = Vec::new();
        for _ in 0..=random.int(12) {
            let index = random.int(HOSTILE as u32) as usize;
            let value = secret(&mut random, &mut secrets);
            lines.push(hostile(index, &value));
        }
        for key in keys {
            if random.int(2) == 1 {
                let at = random.int(lines.len() as u32 + 1) as usize;
                let value = secret(&mut random, &mut secrets);
                lines.insert(at, format!("{key}={value}"));
            }
        }
        let mut text = lines.join(eol);
        if random.int(2) == 1 {
            text.push_str(eol);
        }
        let mut setup = Setup::new()
            .path(".env")
            .param("why", "for the test")
            .caller(CALLER)
            .file(".env", &text);
        if random.int(2) == 1 {
            let (a, b) = (
                secret(&mut random, &mut secrets),
                secret(&mut random, &mut secrets),
            );
            setup = setup.file(
                ".env.example",
                &format!("# say what\nFROM_EXAMPLE={a}\nAPI_KEY={b}\n"),
            );
        }
        if random.int(2) == 1 {
            setup = setup.ignored();
        }
        let asked: Vec<&str> = keys
            .iter()
            .copied()
            .filter(|_| random.int(3) == 0)
            .collect();
        if !asked.is_empty() {
            setup = setup.param("keys", &asked.join(","));
        }
        let mut tab = setup.open();
        assert_eq!(tab.kit().screen, Screen::Dotenv, "run {run}");

        for (key, _) in rows(&tab) {
            match random.int(4) {
                0 => {
                    let value = secret(&mut random, &mut secrets);
                    tab.edit_row(&key, &value);
                }
                1 => tab.edit_row(&key, ""),
                2 => {
                    let (a, b) = (
                        secret(&mut random, &mut secrets),
                        secret(&mut random, &mut secrets),
                    );
                    tab.edit_row(&key, &format!("{a}\n{b}=x"));
                }
                _ => {}
            }
        }
        if !offered(&tab).is_empty() && random.int(2) == 1 {
            add_all_offered(&mut tab);
        }
        if random.int(4) == 0 {
            // The person pastes a secret where the name of a new key goes.
            let pasted = format!(
                "Pasted_{}",
                secret(&mut random, &mut secrets).replace('-', "_")
            );
            secrets.push(pasted.clone());
            add_new_key(&mut tab, &pasted);
            let value = secret(&mut random, &mut secrets);
            tab.edit_row(&pasted, &value);
        }
        // The file may have moved in the meantime, and the person chooses.
        if random.int(4) == 0 {
            let moved = secret(&mut random, &mut secrets);
            tab.fake
                .store
                .set(".env", &format!("# moved\nMOVED={moved}\n{text}"));
        }
        save_or_tell(&mut tab);
        if tab.kit().conflict.is_some() {
            conflicts += 1;
            if random.int(2) == 1 {
                tab.press("Save with these choices");
            } else {
                tab.press("Keep what is on disk, lose mine");
            }
            save_or_tell(&mut tab);
        }
        answers += clean(&tab, &secrets, &format!("run {run} of {text:?}")).len();
    }
    assert!(conflicts > 20, "the fuzz reaches the conflict: {conflicts}");
    assert!(answers > 300, "and answers the agent: {answers}");
}

#[test]
fn a_text_file_only_that_it_changed_whatever_it_holds() {
    let secret = s("text");
    let files = [
        ("config.json", format!("{{ \"password\": \"{secret}\" }}\n")),
        ("values.yaml", format!("password: {secret}\n")),
        ("app.toml", format!("password = \"{secret}\"\n")),
        ("notes.txt", format!("{secret}\n")),
    ];
    for (file, _) in &files {
        let mut setup = Setup::new().path(file).caller(CALLER);
        for (path, text) in &files {
            setup = setup.file(path, text);
        }
        let mut tab = setup.open();
        let value = tab.kit().text.as_ref().unwrap().value.clone();
        tab.type_into("editor", &format!("{value}# {secret}-typed\n"));
        tab.press("Review changes");
        if tab.shows("I know this file is broken: save it anyway.") {
            tab.toggle("I know this file is broken: save it anyway.", true);
        }
        tab.press("Save");
        let sent = clean(&tab, std::slice::from_ref(&secret), file);
        assert_eq!(
            sent,
            [format!("The person saved `{file}`: the file changed.")]
        );
    }
}

#[test]
fn failures_and_retries_carry_no_value_either() {
    let secret = s("retry");
    for code in ["not_found", "denied", "unavailable", "rate_limited"] {
        let mut tab = Setup::new()
            .path(".env")
            .param("keys", "A")
            .caller(CALLER)
            .file(".env", &format!("A={secret}\n"))
            .ignored()
            .send_error(code)
            .open();
        tab.edit_row("A", &format!("{secret}2"));
        tab.press("Save");
        assert_eq!(
            tab.kit().told.as_ref().unwrap().status,
            kit_editor_tab::editor::TellStatus::Failed
        );
        tab.fake.send_error = None;
        tab.press("Tell Asker again");
        assert_eq!(
            tab.kit().told.as_ref().unwrap().status,
            kit_editor_tab::editor::TellStatus::Sent
        );
        let sent = clean(&tab, std::slice::from_ref(&secret), code);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0], sent[1], "the same message, sent again");
    }
}

#[test]
fn with_nobody_to_answer_nothing_is_sent() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .ignored()
        .open();
    tab.edit_row("A", "2");
    tab.press("Save");
    assert!(tab.fake.sent().is_empty());
    assert_eq!(tab.store().get(".env"), Some("A=2\n"));
}

#[test]
fn the_message_is_made_of_names_what_is_not_a_name_or_is_a_value_stays_unsaid() {
    let known: HashSet<String> = [
        "API_KEY",
        "hunter2hunter2",
        "HAS_hunter2hunter2_IN",
        "SHORT",
        "Bad Name",
        "a=b",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let names =
        |list: &[&str]| -> Vec<String> { list.iter().map(|name| name.to_string()).collect() };
    let values = names(&["hunter2hunter2", "x"]);
    let text = answer::dotenv_saved(
        &answer::Facts {
            path: ".env",
            known: &known,
            values: &values,
        },
        &names(&[
            "API_KEY",
            "hunter2hunter2",
            "HAS_hunter2hunter2_IN",
            "NEVER_SEEN",
            "Bad Name",
            "a=b",
            "line\nbreak",
            "42",
        ]),
        &[],
        &names(&["SHORT"]),
    );
    assert_eq!(
        text,
        "The person saved `.env`. Set: API_KEY. Still empty: SHORT. 7 other keys changed or empty. The values stay in the file: do not read them back, use them through the program that loads it."
    );
    let one = |name: &str| -> HashSet<String> { [name.to_owned()].into_iter().collect() };
    assert!(
        answer::may_name("A1", &one("A1"), &names(&["1"])),
        "a short value does not silence a name by chance"
    );
    assert!(
        !answer::may_name("A1", &one("A1"), &names(&["A1"])),
        "but a name that is a value does"
    );
    let long = "x".repeat(65);
    assert!(
        !answer::may_name(&long, &one(&long), &[]),
        "a name this long reads as a token"
    );
}

#[test]
fn a_key_the_person_typed_is_counted_never_named() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "A")
        .caller(CALLER)
        .file(".env", "A=\n")
        .ignored()
        .open();
    add_new_key(&mut tab, "sk_live_PASTED_BY_MISTAKE");
    tab.edit_row("sk_live_PASTED_BY_MISTAKE", "whatever");
    tab.edit_row("A", "filled");
    tab.press("Save");
    let sent = tab.fake.sent();
    assert!(
        sent[0].1.contains("Set: A. 1 other key changed or empty."),
        "{}",
        sent[0].1
    );
    assert!(!sent[0].1.contains("sk_live"));
}

#[test]
fn a_long_list_of_keys_stays_a_list_of_names_and_within_what_panes_send_carries() {
    let lines: Vec<String> = (0..300).map(|i| format!("KEY_{i}=old{i}")).collect();
    let mut tab = Setup::new()
        .path(".env")
        .caller(CALLER)
        .file(".env", &lines.join("\n"))
        .ignored()
        .open();
    for (key, _) in rows(&tab) {
        tab.edit_row(&key, &format!("new-{key}"));
    }
    tab.press("Save");
    let sent = clean(&tab, &["new-KEY".to_owned(), "old1".to_owned()], "long");
    assert!(sent[0].contains("260 other keys changed or empty."));
    assert!(sent[0].chars().count() < 20_000);
}

fn source(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(name)).unwrap()
}

#[test]
fn in_the_sources_one_panes_send_fed_by_answer_alone_and_no_log() {
    let names = [
        "answer.rs",
        "diff.rs",
        "dotenv.rs",
        "editor.rs",
        "gitignore.rs",
        "js.rs",
        "lib.rs",
        "logic.rs",
        "validate.rs",
        "view.rs",
    ];
    let mut uses = Vec::new();
    for name in names {
        let text = source(name);
        uses.extend(text.matches("PanesSend").map(|_| name));
        uses.extend(text.matches("\"panes.send\"").map(|_| name));
        // Nothing is logged: a log line is kept beside a failure.
        for logging in [
            "console::",
            "sys::log",
            "eprintln!",
            "println!",
            "dbg!",
            "call_raw",
        ] {
            assert!(!text.contains(logging), "{name} uses {logging}");
        }
    }
    assert_eq!(uses, ["editor.rs"], "panes.send is called from one place");
    let editor = source("editor.rs");
    assert!(editor.contains("api::PanesSend {\n                pane: pane.clone(),\n                text: message,\n            }"));
    // Each `send(` is given what answer made, or the message already made for it.
    let senders = Regex::new(r"self\.send\(([^,]+),").unwrap();
    let mut count = 0;
    for found in senders.captures_iter(&editor) {
        let first = found[1].trim();
        assert!(
            first == "message" || first.starts_with("answer::"),
            "send({first}, …)"
        );
        count += 1;
    }
    assert_eq!(count, 4);
    for made in [
        "let message = answer::dotenv_saved(",
        "answer::dotenv_checked(",
        "let Some((message, status)) = self.unsent.clone()",
    ] {
        assert!(editor.contains(made), "{made}");
    }
    // And answer takes no field that could carry a value but `values`, used only to stay silent.
    let speech = source("answer.rs");
    assert!(
        !speech.contains("{values"),
        "a value formatted into a sentence"
    );
    assert!(
        !Regex::new(r"\.value\b").unwrap().is_match(&speech),
        "a value read in answer"
    );
}

#[test]
fn the_fuzz_would_catch_a_leak_a_value_in_the_message_fails_the_checks() {
    // Guards the checks themselves: a broken answer must not slip past `assert_clean`.
    let leak = s("leak");
    let call = |method: &str, params: serde_json::Value| Call {
        id: 1,
        method: method.to_owned(),
        params,
    };
    let planted = |calls: Vec<Call>, logs: Vec<String>, secret: String| {
        std::panic::catch_unwind(move || assert_clean(&calls, &logs, &[secret], "planted")).is_err()
    };
    let stay = "The values stay in the file: do not read them back, use them through the program that loads it.";
    assert!(planted(
        vec![call(
            "panes.send",
            serde_json::json!({ "pane": "Asker", "text": format!("The person saved `.env`. Set: API_KEY={leak}. {stay}") })
        )],
        vec![],
        leak.clone()
    ));
    let named = leak.replace('-', "_");
    assert!(planted(
        vec![call(
            "panes.send",
            serde_json::json!({ "pane": "Asker", "text": format!("The person saved `.env`. Set: {named}. {stay}") })
        )],
        vec![],
        named.clone()
    ));
    assert!(planted(
        vec![call(
            "tab.setBadge",
            serde_json::json!({ "state": "neutral", "text": leak })
        )],
        vec![],
        leak.clone()
    ));
    assert!(
        planted(vec![], vec![format!("files.write: {leak}")], leak.clone()),
        "a log line too"
    );
    let _ = Fake::default();
}
