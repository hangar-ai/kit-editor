//! What the page draws for each state: values dotted until revealed, text
//! never read as markup, both languages, one primary action.
//! test/view.test.js, ported.

mod common;

use std::path::Path;

use common::{CALLER, Setup, Tab};
use hangar_kit_sdk::ui::{Element, EmptyKind, LineKind, Notice};
use kit_editor_tab::editor::{ConflictKind, Screen, Side, Stage};
use kit_editor_tab::logic::{EN, FR};
use kit_editor_tab::view::MASK;

const SECRET: &str = "s3cr3t-value-XqZ9";

fn holding(tab: &Tab, needle: &str) -> Vec<(String, String)> {
    tab.strings()
        .into_iter()
        .filter(|(_, text)| text.contains(needle))
        .collect()
}

#[test]
fn a_value_is_masked_until_its_row_is_revealed_and_shows_nowhere_else() {
    let mut tab = Setup::new()
        .path(".env")
        .file(
            ".env",
            &format!("# about it\nAPI_KEY={SECRET}\n# OLD={SECRET}\n"),
        )
        .ignored()
        .open();
    let Some(Element::TextInput(input)) = tab.value_field("API_KEY") else {
        panic!("a one-line field");
    };
    assert!(input.secure, "dotted");
    assert_eq!(
        input.value, SECRET,
        "the field holds it, and Hangar dots it"
    );
    assert!(!tab.text().contains(SECRET), "no text of the page holds it");
    assert_eq!(
        holding(&tab, SECRET),
        [("value".to_owned(), SECRET.to_owned())],
        "only that field's value, not a title, a hint or a comment"
    );

    tab.toggle_reveal("API_KEY");
    let Some(Element::TextInput(input)) = tab.value_field("API_KEY") else {
        panic!("a one-line field");
    };
    assert!(!input.secure);
    assert!(tab.row("API_KEY").revealed);
}

#[test]
fn a_value_over_several_lines_stays_hidden_and_is_edited_whole_once_shown() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", &format!("KEY='first\n{SECRET}'\n"))
        .ignored()
        .open();
    let Some(Element::TextInput(hidden)) = tab.value_field("KEY") else {
        panic!("a one-line field stands in");
    };
    assert!(hidden.on_input.is_none(), "nothing to type in");
    assert!(hidden.secure);
    assert_eq!(
        hidden.value, "",
        "a one-line field would drop its line breaks"
    );
    assert_eq!(hidden.placeholder, "Several lines");
    assert!(holding(&tab, SECRET).is_empty());

    tab.toggle_reveal("KEY");
    let Some(Element::TextArea(area)) = tab.value_field("KEY") else {
        panic!("a text area once shown");
    };
    assert_eq!(area.value, format!("first\n{SECRET}"));
    assert!(area.on_input.is_some());
}

#[test]
fn asked_keys_come_first_with_their_state_then_the_files_and_why_at_the_top() {
    let tab = Setup::new()
        .path("sub/.env")
        .param("keys", "MISSING,DONE")
        .param("why", "For the <b>tests</b>.")
        .caller(CALLER)
        .file("sub/.env", "OTHER=1\nDONE=x\n")
        .file("sub/.env.example", "# Offer hint\nOFFERED=y\n")
        .ignored()
        .open();
    let banner = tab
        .elements()
        .into_iter()
        .find_map(|element| match element {
            Element::Banner(banner) if banner.title.is_some() => Some(banner),
            _ => None,
        })
        .expect("the request");
    assert_eq!(banner.title.as_deref(), Some("Asker asks:"));
    assert_eq!(
        banner.text, "For the <b>tests</b>.",
        "shown as written, not as markup"
    );
    assert_eq!(tab.title(), ".env");
    let state = tab.header_state();
    assert!(
        state.contains("1 missing") && state.contains("sub/"),
        "{state}"
    );
    assert_eq!(
        tab.sections(),
        [
            "Asked for",
            "In this file",
            "In .env.example, missing here",
            "Add a key"
        ]
    );
    let titles: Vec<String> = tab
        .elements()
        .into_iter()
        .filter_map(|element| match element {
            Element::List(list) => Some(list.items.into_iter().map(|item| item.title)),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(titles, ["MISSING", "DONE", "OTHER", "OFFERED"]);
    assert!(tab.shows("Offer hint"));
    let states: Vec<String> = tab
        .elements()
        .into_iter()
        .filter_map(|element| match element {
            Element::State(state) => Some(state.label),
            _ => None,
        })
        .collect();
    assert_eq!(states.iter().filter(|label| *label == "missing").count(), 1);
    assert_eq!(states.iter().filter(|label| *label == "set").count(), 1);
}

#[test]
fn the_warning_says_it_plainly_and_the_button_is_what_writes() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .file(".gitignore", "x\n")
        .open();
    assert!(tab.shows(".env is not ignored by git: git would commit it, secrets included."));
    let warnings = |tab: &Tab| {
        tab.elements()
            .into_iter()
            .filter(|element| matches!(element, Element::Banner(banner) if banner.kind == Notice::Warning))
            .count()
    };
    assert_eq!(warnings(&tab), 1);
    assert!(tab.fake.writes().is_empty());
    tab.press("Add .env to .gitignore");
    assert_eq!(tab.store().get(".gitignore"), Some("x\n.env\n"));
    assert_eq!(warnings(&tab), 0);
}

#[test]
fn typing_refreshes_what_depends_on_it_and_leaves_the_field_alone() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "A")
        .file(".env", "A=\n")
        .ignored()
        .open();
    assert!(!tab.enabled("Save"), "nothing to save");
    assert_eq!(
        tab.button("Save").unwrap().tooltip.as_deref(),
        Some("Nothing to save yet.")
    );
    assert!(tab.header_state().contains("1 missing"));
    tab.edit_row("A", "typed");
    assert!(
        matches!(tab.value_field("A"), Some(Element::TextInput(input)) if input.id == "value:A"),
        "the same field: its caret stays"
    );
    assert!(tab.enabled("Save"));
    assert!(!tab.header_state().contains("missing"));
    assert!(
        tab.elements()
            .iter()
            .any(|element| matches!(element, Element::Chip(chip) if chip.label == "changed"))
    );
    tab.press("Save");
    assert!(tab.kit().saved);
    assert!(tab.header_state().contains("Saved"));
}

#[test]
fn one_primary_action_at_a_time_save_or_tell_the_agent_there_is_nothing_to_change() {
    let mut tab = Setup::new()
        .path(".env")
        .caller(CALLER)
        .file(".env", "A=1\n")
        .ignored()
        .open();
    assert_eq!(tab.primaries(), ["Tell Asker: nothing to change"]);
    tab.edit_row("A", "2");
    assert_eq!(tab.primaries(), ["Save"]);
    tab.press("Save");
    assert!(tab.shows("Asker was told, without any value."));
    assert!(!tab.shows("nothing to change"));
}

#[test]
fn with_keys_still_missing_save_stays_the_primary_action() {
    let tab = Setup::new()
        .path(".env")
        .param("keys", "A")
        .caller(CALLER)
        .file(".env", "")
        .ignored()
        .open();
    assert_eq!(tab.primaries(), ["Save"]);
    assert!(
        tab.button("Tell Asker: nothing to change").is_some(),
        "the lesser choice is there too"
    );
}

#[test]
fn an_agent_that_could_not_be_told_is_said_with_a_way_to_try_again() {
    let mut tab = Setup::new()
        .path(".env")
        .caller(CALLER)
        .file(".env", "A=1\n")
        .ignored()
        .send_error("not_found")
        .open();
    tab.edit_row("A", "2");
    tab.press("Save");
    assert!(tab.shows("Saved, but Asker could not be told: that pane is gone. No value was sent."));
    tab.fake.send_error = None;
    tab.press("Tell Asker again");
    assert!(tab.shows("Asker was told"));
}

#[test]
fn the_editor_line_numbers_the_line_asked_for_marked_and_the_check_above_it() {
    let body: Vec<String> = (0..12).map(|i| format!("\"k{i}\": {i},")).collect();
    let mut tab = Setup::new()
        .path("conf.json")
        .param("line", "5")
        .param("why", "Check the timeout.")
        .file("conf.json", &format!("{{\n{}\n}}\n", body.join("\n")))
        .open();
    let area = |tab: &Tab| {
        tab.elements()
            .into_iter()
            .find_map(|element| match element {
                Element::TextArea(area) if area.id == "editor" => Some(area),
                _ => None,
            })
            .expect("the editor")
    };
    let editor = area(&tab);
    assert!(editor.line_numbers);
    assert!(editor.fill);
    let marks: Vec<(u32, Notice)> = editor
        .marks
        .iter()
        .map(|mark| (mark.line, mark.kind))
        .collect();
    assert!(marks.contains(&(5, Notice::Info)), "{marks:?}");
    assert!(
        marks.iter().any(|(_, kind)| *kind == Notice::Error),
        "the broken line too: {marks:?}"
    );
    assert_eq!(
        tab.cursor_moves(),
        [(5, 1)],
        "scrolled to it, the keyboard left where it was"
    );
    assert!(
        tab.header_state().starts_with("Broken JSON, line "),
        "{}",
        tab.header_state()
    );
    assert!(tab.shows("Check the timeout."));
    assert!(tab.shows("JSON cannot be read past this point."));

    tab.type_into("editor", "{\"a\": 1}\n");
    assert!(tab.header_state().contains("Valid JSON"));
    assert_eq!(
        area(&tab)
            .marks
            .iter()
            .map(|mark| mark.line)
            .collect::<Vec<_>>(),
        [5],
        "the error is gone"
    );
    assert!(tab.kit().is_dirty());
}

#[test]
fn the_review_signs_as_well_as_colours_then_a_broken_file_asks() {
    let mut tab = Setup::new()
        .path("a.json")
        .file("a.json", "{\"a\": 1}\n")
        .open();
    tab.type_into("editor", "{\"a\": 2,}\n");
    tab.press("Review changes");
    let lines: Vec<(String, LineKind)> = tab
        .elements()
        .into_iter()
        .find_map(|element| match element {
            Element::Log(log) if log.id == "diff" => Some(
                log.lines
                    .into_iter()
                    .map(|line| (line.text, line.kind))
                    .collect(),
            ),
            _ => None,
        })
        .expect("the diff");
    assert!(
        lines.contains(&("− {\"a\": 1}".to_owned(), LineKind::Removed)),
        "{lines:?}"
    );
    assert!(
        lines.contains(&("+ {\"a\": 2,}".to_owned(), LineKind::Added)),
        "{lines:?}"
    );
    assert!(!tab.enabled("Save"), "broken, and not said so");
    assert!(tab.shows("I know this file is broken: save it anyway."));
    tab.toggle("I know this file is broken: save it anyway.", true);
    assert!(tab.enabled("Save"));
    tab.press("Save");
    assert_eq!(tab.store().get("a.json"), Some("{\"a\": 2,}\n"));
    assert_eq!(tab.kit().text.as_ref().unwrap().stage, Stage::Edit);
}

#[test]
fn a_long_diff_is_given_in_parts() {
    let before: Vec<String> = (0..3000).map(|i| format!("a{i}")).collect();
    let mut tab = Setup::new()
        .path("big.txt")
        .file("big.txt", &before.join("\n"))
        .open();
    let after: Vec<String> = (0..3000).map(|i| format!("b{i}")).collect();
    tab.type_into("editor", &after.join("\n"));
    tab.press("Review changes");
    let log = |tab: &Tab| {
        tab.elements()
            .into_iter()
            .find_map(|element| match element {
                Element::Log(log) if log.id == "diff" => Some(log),
                _ => None,
            })
            .unwrap()
    };
    let first = log(&tab);
    assert_eq!(first.total, Some(6000));
    assert_eq!((first.first, first.lines.len()), (0, 400));
    tab.harness.range("diff", 5000, 5100);
    tab.settle();
    let later = log(&tab);
    assert_eq!(later.first, 5000);
    assert!(
        later.lines[0].text.starts_with("+ b"),
        "{}",
        later.lines[0].text
    );
}

#[test]
fn the_text_conflict_shows_both_sides_and_three_ways_out() {
    let mut tab = Setup::new().path("a.txt").file("a.txt", "base\n").open();
    tab.type_into("editor", "mine\n");
    tab.press("Review changes");
    tab.fake.store.set("a.txt", "theirs\n");
    tab.press("Save");
    assert!(tab.shows("This file changed while you were editing it. Nothing was written."));
    assert!(tab.shows("− theirs"));
    assert!(tab.shows("+ mine"));
    assert!(tab.header_state().contains("Changed on disk"));
    for way in [
        "Replace it with mine",
        "Keep what is on disk, lose mine",
        "Back to editing, with mine",
    ] {
        assert!(tab.enabled(way), "{way}");
    }
    tab.press("Keep what is on disk, lose mine");
    assert_eq!(tab.kit().text.as_ref().unwrap().value, "theirs\n");
    assert!(tab.button("Replace it with mine").is_none());
}

#[test]
fn the_dotenv_conflict_keeps_values_hidden_until_asked_on_both_sides() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .ignored()
        .open();
    tab.edit_row("A", &format!("{SECRET}-mine"));
    tab.fake
        .store
        .set(".env", &format!("A={SECRET}-theirs\nB=2\n"));
    tab.press("Save");
    assert!(holding(&tab, SECRET).is_empty(), "both dotted");
    let (labels, chosen) = tab.sides("A");
    assert_eq!(
        labels,
        [format!("Yours  {MASK}"), format!("On disk  {MASK}")]
    );
    assert_eq!(chosen.as_deref(), Some("mine"));
    assert!(tab.shows("1 other key changed on disk"));

    tab.toggle_reveal("A");
    let (labels, _) = tab.sides("A");
    assert_eq!(
        labels,
        [
            format!("Yours  {SECRET}-mine"),
            format!("On disk  {SECRET}-theirs")
        ]
    );
    tab.choose_side("A", "theirs");
    let ConflictKind::Dotenv { rows, .. } = &tab.kit().conflict.as_ref().unwrap().kind else {
        panic!("a dotenv conflict");
    };
    assert_eq!(rows[0].choice, Side::Theirs);
    tab.press("Save with these choices");
    assert_eq!(
        tab.store().get(".env"),
        Some(format!("A={SECRET}-theirs\nB=2\n").as_str())
    );
}

#[test]
fn the_picker_groups_filters_as_you_type_and_opens_what_is_clicked() {
    let mut tab = Setup::new()
        .files(&[
            (".env.local", "A=1\n"),
            ("app.yaml", "a: 1\n"),
            ("src/x.ts", ""),
            (".gitignore", ".env.local\n"),
        ])
        .open();
    assert_eq!(
        tab.sections(),
        ["Environment files", "Configuration files", "Other files"]
    );
    assert!(tab.shows("new"), ".env is offered though it is not there");
    tab.type_into("Filter by name", "app");
    assert_eq!(tab.sections(), ["Configuration files"]);
    tab.type_into("Filter by name", "nothing like it");
    assert!(tab.shows("No file matches."));
    tab.type_into("Filter by name", "");
    tab.press(".env.local");
    assert_eq!(tab.kit().screen, Screen::Dotenv);
    assert!(tab.button("← Files").is_some());
}

#[test]
fn back_with_edits_asks_in_place_without_one_it_just_goes() {
    let opened = |answer: bool| {
        let mut tab = Setup::new().file("a.txt", "x\n").confirm(answer).open();
        tab.press("a.txt");
        tab
    };
    let mut stay = opened(false);
    stay.type_into("editor", "changed\n");
    stay.press("← Files");
    let asked = stay.fake.calls_to("ui.confirm");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].params["message"], "Your changes are not saved.");
    assert_eq!(asked[0].params["confirm"], "Lose them");
    assert_eq!(asked[0].params["cancel"], "Keep editing");
    assert_eq!(stay.kit().screen, Screen::Text);
    assert_eq!(stay.kit().text.as_ref().unwrap().value, "changed\n");

    let mut leave = opened(true);
    leave.type_into("editor", "changed\n");
    leave.press("← Files");
    assert_eq!(leave.kit().screen, Screen::Picker);

    let mut clean = opened(false);
    clean.press("← Files");
    assert_eq!(clean.kit().screen, Screen::Picker);
    assert!(clean.fake.calls_to("ui.confirm").is_empty());
}

#[test]
fn denied_and_failed_are_designed_states_each_saying_what_to_do() {
    let denied = Setup::new().path(".env").deny("files.read").open();
    assert!(denied.shows("This tab may not read or write here"));
    assert!(denied.shows("Accept files.read and files.write"));
    let kinds: Vec<EmptyKind> = denied
        .elements()
        .into_iter()
        .filter_map(|element| match element {
            Element::Empty(empty) => Some(empty.kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, [EmptyKind::Denied], "drawn with its lock");
    assert!(denied.button("Try again").is_some());

    let mut bad = Setup::new().path("../x").open();
    assert!(bad.shows("This path cannot be opened"));
    assert!(
        bad.button("Try again").is_none(),
        "trying again would change nothing"
    );
    bad.press("Choose a file");
    assert_eq!(bad.kit().screen, Screen::Picker, "the picker is the way on");

    let failed = Setup::new()
        .path("big.txt")
        .read_error("big.txt", "failed", "big.txt is larger than 1 MB")
        .open();
    assert!(failed.shows("This file cannot be opened"));
    assert!(
        failed.shows("big.txt is larger than 1 MB"),
        "what Hangar said"
    );
}

#[test]
fn loading_is_a_state_too() {
    let tab = hangar_kit_sdk::testing::Harness::<kit_editor_tab::Editor>::new(
        hangar_kit_sdk::testing::start(),
    );
    assert!(tab.shows("Loading…"));
}

#[test]
fn in_french_with_accents() {
    let mut tab = Setup::new()
        .locale("fr")
        .path(".env")
        .param("keys", "A,B")
        .caller(CALLER)
        .file(".env", "C=1\n")
        .file(".gitignore", "x\n")
        .open();
    let text = tab.text();
    assert!(text.contains("Dans ce fichier"));
    assert!(text.contains("Demandées"));
    assert!(text.contains("manquante"));
    assert!(text.contains("n'est pas ignoré par git"));
    tab.edit_row("A", "x");
    assert!(tab.shows("Enregistrer"));
    assert!(
        tab.elements()
            .iter()
            .any(|element| matches!(element, Element::Chip(chip) if chip.label == "modifiée"))
    );
    assert_eq!(tab.fake.last_badge()["text"], "1 à remplir");
}

fn sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn every_word_the_page_says_exists_in_both_languages() {
    let mut en: Vec<&str> = EN.iter().map(|(key, _)| *key).collect();
    let mut fr: Vec<&str> = FR.iter().map(|(key, _)| *key).collect();
    en.sort();
    fr.sort();
    assert_eq!(en, fr);
    let accents = |text: &str| {
        text.chars()
            .any(|c| "àâçéèêëîïôûùüœÀÂÇÉÈÊËÎÏÔÛÙÜŒ".contains(c))
    };
    assert!(
        FR.iter().filter(|(_, text)| accents(text)).count() > 60,
        "written with its accents"
    );
    let known = |key: &str| {
        EN.iter()
            .any(|(other, _)| *other == key || *other == format!("{key}_one"))
    };
    let mut used = 0;
    for (file, source) in sources() {
        for call in [".t(\"", ".tv(\"", ".tn(\""] {
            for (at, _) in source.match_indices(call) {
                let rest = &source[at + call.len()..];
                let key: String = rest.chars().take_while(|c| *c != '"').collect();
                assert!(known(&key), "{file}: {key}");
                used += 1;
            }
        }
    }
    assert!(used > 80, "the scan finds the words: {used}");
    for code in [
        "json",
        "yaml-tab",
        "yaml-quote",
        "yaml-bracket",
        "yaml-reserved",
        "toml-trailing",
        "toml-string",
        "toml-escape",
        "toml-key",
        "toml-equals",
        "toml-value",
        "toml-header",
        "toml-duplicate",
    ] {
        assert!(known(&format!("check.{code}")), "{code}");
    }
    let validate = sources()
        .into_iter()
        .find(|(file, _)| file == "validate.rs")
        .unwrap()
        .1;
    for prefix in ["\"yaml-", "\"toml-"] {
        for (at, _) in validate.match_indices(prefix) {
            let code: String = validate[at + 1..]
                .chars()
                .take_while(|c| *c != '"')
                .collect();
            assert!(known(&format!("check.{code}")), "{code}");
        }
    }
    for kind in [
        "denied",
        "invalid",
        "bad-path",
        "failed",
        "unavailable",
        "timeout",
        "error",
    ] {
        assert!(
            known(&format!("err.{kind}.title")) && known(&format!("err.{kind}.hint")),
            "{kind}"
        );
    }
    for code in ["denied", "rate_limited", "invalid", "other", "conflict"] {
        assert!(known(&format!("write.{code}")), "{code}");
    }
}

#[test]
fn no_colour_of_its_own() {
    // Hangar draws every widget in the person's theme: a tab names what
    // something is, never a colour.
    for (file, source) in sources() {
        assert!(!source.contains("Ink::"), "{file} names an ink");
        assert!(!source.contains("Surface::"), "{file} picks a surface");
        let hex = source.match_indices('#').any(|(at, _)| {
            let digits: String = source[at + 1..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .collect();
            digits.len() == 6
                && source[at + 1..]
                    .chars()
                    .nth(6)
                    .is_none_or(|c| !c.is_ascii_alphanumeric())
        });
        assert!(!hex, "{file} holds a colour");
    }
}
