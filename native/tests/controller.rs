//! The tab's behaviour against a fake Hangar: what it reads, when it writes,
//! what it refuses to write, and what it says when Hangar answers no.
//! test/controller.test.js, ported: each case drives the tab through its page.

mod common;

use serde_json::{Value, json};

use common::{CALLER, Setup};
use kit_editor_tab::diff::{Hunk, Kind};
use kit_editor_tab::editor::{ConflictKind, IgnoreStatus, Screen, Side, Stage, TellStatus};
use kit_editor_tab::logic;

fn changes(hunks: &[Hunk]) -> Vec<(Kind, String)> {
    hunks
        .iter()
        .filter_map(|hunk| match hunk {
            Hunk::Line(op) if op.kind != Kind::Same => Some((op.kind, op.text.clone())),
            _ => None,
        })
        .collect()
}

/* ── the dotenv form ──────────────────────────────────────────────── */

#[test]
fn a_dotenv_file_opens_as_a_form_one_row_per_key_hidden_its_comments_and_order_kept_on_save() {
    let text = "# Database\nDB_URL=old\n\n# Payments\nexport STRIPE_KEY='x' # test mode\nLOG=debug\nDB_URL=later\n";
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", text)
        .ignored()
        .open();
    assert_eq!(tab.kit().screen, Screen::Dotenv);
    assert_eq!(tab.keys(), ["DB_URL", "STRIPE_KEY", "LOG"]);
    assert!(
        tab.kit()
            .form
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .all(|row| !row.revealed),
        "hidden until revealed"
    );
    assert_eq!(tab.row("DB_URL").value, "later");
    assert_eq!(tab.row("DB_URL").count, 2);
    assert_eq!(tab.row("STRIPE_KEY").hint, "Payments");

    tab.edit_row("STRIPE_KEY", "sk_new");
    tab.press("Save");
    assert_eq!(
        tab.store().get(".env"),
        Some(
            "# Database\nDB_URL=old\n\n# Payments\nexport STRIPE_KEY='sk_new' # test mode\nLOG=debug\nDB_URL=later\n"
        )
    );
    assert_eq!(
        tab.fake.writes()[0].params["expected"],
        text,
        "with what it read"
    );
    assert!(tab.kit().saved);
    assert_eq!(tab.fake.last_badge()["state"], "success");
}

#[test]
fn a_value_is_revealed_row_by_row_and_hidden_again_after_saving() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\nB=2\n")
        .ignored()
        .open();
    tab.toggle_reveal("A");
    assert!(tab.row("A").revealed);
    assert!(!tab.row("B").revealed);
    tab.edit_row("A", "9");
    tab.press("Save");
    assert!(!tab.row("A").revealed);
}

#[test]
fn the_keys_an_agent_asks_for_come_first_marked_missing_with_why_at_the_top() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "NEW_ONE, PRESENT,EMPTY")
        .param("why", "For the payment tests.")
        .caller(CALLER)
        .file(".env", "OTHER=1\nPRESENT=x\nEMPTY=\n")
        .ignored()
        .open();
    assert_eq!(tab.kit().why.as_deref(), Some("For the payment tests."));
    assert_eq!(tab.keys(), ["NEW_ONE", "PRESENT", "EMPTY", "OTHER"]);
    let asked: Vec<bool> = tab
        .kit()
        .form
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|row| row.asked)
        .collect();
    assert_eq!(asked, [true, true, true, false]);
    assert_eq!(tab.fake.last_badge()["text"], "2 missing");
    tab.edit_row("NEW_ONE", "v");
    assert_eq!(tab.fake.last_badge()["text"], "1 missing");
    tab.edit_row("EMPTY", "v");
    assert_eq!(tab.fake.last_badge()["state"], Value::Null);
}

#[test]
fn a_key_asked_and_left_empty_is_not_written_one_filled_in_goes_at_the_end() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "FILLED,LEFT")
        .caller(CALLER)
        .file(".env", "OTHER=1\n")
        .ignored()
        .open();
    tab.edit_row("FILLED", "v");
    tab.press("Save");
    assert_eq!(tab.store().get(".env"), Some("OTHER=1\nFILLED=v\n"));
    assert_eq!(
        tab.fake.sent(),
        [(
            "Asker".to_owned(),
            "The person saved `.env`. Set: FILLED. Still empty: LEFT. The values stay in the file: do not read them back, use them through the program that loads it.".to_owned()
        )]
    );
    assert!(tab.row("LEFT").asked, "still asked for");
}

#[test]
fn a_file_that_does_not_exist_yet_opens_empty_and_saving_creates_it() {
    let mut tab = Setup::new()
        .path("config/.env")
        .param("keys", "A")
        .file(".gitignore", "*.env\n.env\n")
        .open();
    assert_eq!(tab.kit().screen, Screen::Dotenv);
    assert!(!tab.kit().exists);
    tab.edit_row("A", "1");
    tab.press("Save");
    assert_eq!(tab.store().get("config/.env"), Some("A=1\n"));
    assert_eq!(
        tab.fake.writes()[0].params.get("expected"),
        None,
        "there was nothing to expect"
    );
    assert!(tab.kit().exists);
}

#[test]
fn keys_of_env_example_missing_here_are_offered_not_added() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "FROM_EXAMPLE")
        .file(".env", "HERE=1\n")
        .file(
            ".env.example",
            "HERE=\n# Where to find it\nFROM_EXAMPLE=placeholder\n# Second\nEXTRA=placeholder\nANOTHER=\n",
        )
        .ignored()
        .open();
    let offered = |tab: &common::Tab| -> Vec<String> {
        tab.kit()
            .form
            .as_ref()
            .unwrap()
            .offered
            .iter()
            .map(|offer| offer.key.clone())
            .collect()
    };
    assert_eq!(offered(&tab), ["EXTRA", "ANOTHER"]);
    assert_eq!(tab.kit().form.as_ref().unwrap().offered[0].hint, "Second");
    assert_eq!(
        tab.kit().form.as_ref().unwrap().example_name.as_deref(),
        Some(".env.example")
    );
    assert_eq!(tab.row("FROM_EXAMPLE").hint, "Where to find it");
    assert_eq!(
        tab.row("FROM_EXAMPLE").value,
        "",
        "the example's value is not taken for a real one"
    );
    assert!(!tab.kit().is_dirty(), "nothing is added until asked");

    tab.add_offered("EXTRA");
    assert_eq!(tab.row("EXTRA").value, "");
    assert_eq!(offered(&tab), ["ANOTHER"]);
    tab.edit_row("EXTRA", "mine");
    tab.press("Save");
    assert_eq!(tab.store().get(".env"), Some("HERE=1\nEXTRA=mine\n"));
    assert_eq!(
        offered(&tab),
        ["ANOTHER"],
        "what is still missing stays offered"
    );
    assert_eq!(
        tab.row("FROM_EXAMPLE").hint,
        "Where to find it",
        "and the template still explains what is asked"
    );

    tab.add_offered("ANOTHER");
    tab.press("Save");
    assert_eq!(
        tab.store().get(".env"),
        Some("HERE=1\nEXTRA=mine\nANOTHER=\n"),
        "an explicit add is written, even empty"
    );
    assert!(offered(&tab).is_empty());
}

#[test]
fn add_all_takes_every_offered_key() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "")
        .file(".env.example", "A=\nB=\n")
        .ignored()
        .open();
    tab.press("Add all");
    assert_eq!(tab.keys(), ["A", "B"]);
    tab.press("Save");
    assert_eq!(tab.store().get(".env"), Some("A=\nB=\n"));
}

#[test]
fn a_template_offers_nothing_and_needs_no_warning() {
    let tab = Setup::new()
        .path(".env.example")
        .file(".env.example", "A=\n")
        .open();
    assert!(tab.kit().form.as_ref().unwrap().offered.is_empty());
    assert_eq!(
        tab.kit().ignore.as_ref().unwrap().status,
        IgnoreStatus::Ignored
    );
    let gitignores = tab
        .fake
        .calls_to("files.read")
        .into_iter()
        .filter(|call| {
            call.params["path"]
                .as_str()
                .unwrap()
                .ends_with(".gitignore")
        })
        .count();
    assert_eq!(gitignores, 0);
}

#[test]
fn a_key_the_person_adds_is_checked_and_a_bad_one_says_why() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .ignored()
        .open();
    tab.type_into("KEY_NAME", "not a key");
    tab.press("Add");
    assert_eq!(
        tab.kit().form.as_ref().unwrap().new_key_problem,
        Some("newKeyBad")
    );
    assert!(
        tab.shows("A key is letters")
            || tab
                .strings()
                .iter()
                .any(|(key, text)| key == "error" && text.starts_with("A key is letters"))
    );
    tab.type_into("KEY_NAME", "A");
    tab.press("Add");
    assert_eq!(
        tab.kit().form.as_ref().unwrap().new_key_problem,
        Some("newKeyTaken")
    );
    tab.type_into("KEY_NAME", " B_KEY ");
    tab.press("Add");
    let form = tab.kit().form.as_ref().unwrap();
    assert_eq!(form.new_key_problem, None);
    assert_eq!(form.new_key, "");
    assert!(tab.row("B_KEY").added);
}

#[test]
fn enter_in_the_new_key_field_adds_it() {
    let mut tab = Setup::new().path(".env").file(".env", "").ignored().open();
    tab.type_into("KEY_NAME", "FRESH");
    let submit = tab
        .elements()
        .into_iter()
        .find_map(|element| match element {
            hangar_kit_sdk::ui::Element::TextInput(input) if input.id == "new-key" => {
                input.on_submit
            }
            _ => None,
        })
        .expect("Enter adds");
    tab.ui(submit, None);
    assert!(tab.row("FRESH").typed);
}

/* ── git ──────────────────────────────────────────────────────────── */

#[test]
fn not_ignored_it_says_so_and_gitignore_changes_only_on_a_click() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .file(".gitignore", "node_modules/\n")
        .open();
    assert_eq!(
        tab.kit().ignore.as_ref().unwrap().status,
        IgnoreStatus::NotIgnored
    );
    assert!(tab.fake.writes().is_empty(), "opening writes nothing");
    tab.edit_row("A", "2");
    tab.press("Save");
    assert!(
        tab.fake.writes_to(".gitignore").is_empty(),
        "saving the file does not touch it either"
    );

    tab.press("Add .env to .gitignore");
    assert_eq!(tab.store().get(".gitignore"), Some("node_modules/\n.env\n"));
    assert_eq!(
        tab.fake.writes_to(".gitignore")[0].params["expected"],
        "node_modules/\n"
    );
    assert_eq!(
        tab.kit().ignore.as_ref().unwrap().status,
        IgnoreStatus::Ignored
    );
}

#[test]
fn no_gitignore_at_all_said_plainly_and_the_click_creates_one() {
    let mut tab = Setup::new()
        .path("sub/.env.local")
        .file("sub/.env.local", "A=1\n")
        .open();
    assert_eq!(
        tab.kit().ignore.as_ref().unwrap().status,
        IgnoreStatus::NoFile
    );
    tab.press("Add sub/.env.local to .gitignore");
    assert_eq!(tab.store().get(".gitignore"), Some("sub/.env.local\n"));
    assert_eq!(
        tab.fake.writes_to(".gitignore")[0].params.get("expected"),
        None
    );
    assert_eq!(
        tab.kit().ignore.as_ref().unwrap().status,
        IgnoreStatus::Ignored
    );
}

#[test]
fn a_gitignore_that_moved_under_the_click_is_not_overwritten_and_a_denied_one_says_so() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .file(".gitignore", "old\n")
        .open();
    tab.fake.write_error = Some("rate_limited".into());
    tab.press("Add .env to .gitignore");
    let ignore = tab.kit().ignore.clone().unwrap();
    assert_eq!(ignore.problem.as_deref(), Some("rate_limited"));
    assert_eq!(ignore.status, IgnoreStatus::NotIgnored);
    assert!(!ignore.busy);
    assert_eq!(tab.store().get(".gitignore"), Some("old\n"));
    assert!(tab.shows(".gitignore was not changed: at most 10 saves a minute."));
    tab.fake.write_error = None;
    tab.press("Add .env to .gitignore");
    assert_eq!(tab.store().get(".gitignore"), Some("old\n.env\n"));
}

#[test]
fn a_gitignore_that_cannot_be_read_leaves_the_question_open_and_the_form_works() {
    let tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .read_error(".gitignore", "denied", "no")
        .open();
    assert_eq!(
        tab.kit().ignore.as_ref().unwrap().status,
        IgnoreStatus::Unknown
    );
    assert_eq!(tab.kit().screen, Screen::Dotenv);
    assert!(tab.shows("Could not read .gitignore"));
}

/* ── any other text file ──────────────────────────────────────────── */

#[test]
fn a_text_file_opens_in_the_editor_at_the_line_asked_for() {
    let lines: Vec<String> = (1..=9).map(|i| format!("line {i}")).collect();
    let lines = lines.join("\n");
    let tab = Setup::new()
        .path("notes.txt")
        .param("line", "7")
        .file("notes.txt", &lines)
        .open();
    let text = tab.kit().text.as_ref().unwrap();
    assert_eq!(tab.kit().screen, Screen::Text);
    assert_eq!(tab.cursor_moves(), [(7, 1)], "the cursor goes there once");
    assert_eq!(text.target, Some(7));
    assert_eq!(text.check.kind, None);
    let far = Setup::new()
        .path("notes.txt")
        .param("line", "70")
        .file("notes.txt", &lines)
        .open();
    assert_eq!(far.cursor_moves(), [(9, 1)], "as far as the file goes");
    assert_eq!(far.kit().text.as_ref().unwrap().far, Some((70, 9)));
    assert!(far.shows("Line 70 was asked for; the file has 9."));
}

#[test]
fn a_diff_comes_before_saving_and_saving_sends_what_was_reviewed() {
    let mut tab = Setup::new()
        .path("a.txt")
        .file("a.txt", "one\ntwo\nthree\n")
        .open();
    tab.type_into("editor", "one\nTWO\nthree\n");
    assert!(tab.fake.writes().is_empty(), "typing writes nothing");
    tab.press("Review changes");
    let text = tab.kit().text.as_ref().unwrap();
    assert_eq!(text.stage, Stage::Review);
    assert_eq!(
        changes(text.hunks.as_ref().unwrap()),
        [(Kind::Del, "two".to_owned()), (Kind::Add, "TWO".to_owned())]
    );
    assert!(tab.fake.writes().is_empty(), "reviewing writes nothing");
    tab.press("Save");
    assert_eq!(tab.store().get("a.txt"), Some("one\nTWO\nthree\n"));
    assert_eq!(tab.fake.writes()[0].params["expected"], "one\ntwo\nthree\n");
    assert_eq!(tab.kit().text.as_ref().unwrap().stage, Stage::Edit);
    assert!(tab.kit().saved);
}

#[test]
fn line_endings_survive_the_editor() {
    let mut crlf = Setup::new()
        .path("a.txt")
        .file("a.txt", "one\r\ntwo\r\n")
        .open();
    assert_eq!(
        crlf.kit().text.as_ref().unwrap().value,
        "one\ntwo\n",
        "as an editor would hold it"
    );
    crlf.type_into("editor", "one\ntwo\nthree\n");
    crlf.press("Review changes");
    let texts: Vec<String> = changes(crlf.kit().text.as_ref().unwrap().hunks.as_ref().unwrap())
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert_eq!(texts, ["three"], "the line endings are no change");
    crlf.press("Save");
    assert_eq!(crlf.store().get("a.txt"), Some("one\r\ntwo\r\nthree\r\n"));

    let mut mixed = Setup::new()
        .path("a.txt")
        .file("a.txt", "a\r\nb\nc\r\nd\r\n")
        .open();
    mixed.type_into("editor", "a\nB\nc\nd\n");
    mixed.press("Review changes");
    mixed.press("Save");
    assert_eq!(
        mixed.store().get("a.txt"),
        Some("a\r\nB\r\nc\r\nd\r\n"),
        "only the changed line takes the common one"
    );
}

#[test]
fn a_broken_file_is_saved_only_after_saying_so() {
    let mut tab = Setup::new()
        .path("a.json")
        .file("a.json", "{\"a\": 1}\n")
        .open();
    assert!(tab.kit().text.as_ref().unwrap().check.ok);
    tab.type_into("editor", "{\"a\": 1,}\n");
    assert!(!tab.kit().text.as_ref().unwrap().check.ok);
    tab.press("Review changes");
    assert!(!tab.enabled("Save"), "not without saying so");
    // Ctrl+S reviews in the editor; in the review it is nothing, as in the page.
    assert!(tab.harness.subscriptions().shortcuts.is_empty());
    assert!(tab.fake.writes().is_empty());
    assert_eq!(tab.store().get("a.json"), Some("{\"a\": 1}\n"));

    tab.toggle("I know this file is broken: save it anyway.", true);
    assert!(tab.enabled("Save"));
    tab.press("← Back to editing");
    tab.type_into("editor", "{\"a\": 1,,}\n");
    assert!(
        !tab.kit().text.as_ref().unwrap().ack,
        "changing the text takes back what was said"
    );
    tab.press("Review changes");
    assert!(!tab.kit().text.as_ref().unwrap().ack);
    assert!(!tab.enabled("Save"));
    assert!(tab.fake.writes().is_empty());

    tab.toggle("I know this file is broken: save it anyway.", true);
    tab.press("Save");
    assert_eq!(tab.store().get("a.json"), Some("{\"a\": 1,,}\n"));
}

#[test]
fn a_save_from_an_older_view_still_needs_the_person_to_say_so() {
    // The SDK keeps eight views' handlers: a click drawn before the text broke
    // must not write it.
    let mut tab = Setup::new()
        .path("a.json")
        .file("a.json", "{\"a\": 1}\n")
        .open();
    tab.type_into("editor", "{\"a\": 2}\n");
    tab.press("Review changes");
    let seq = tab.harness.view().seq;
    let save = tab.button("Save").unwrap().on_press.unwrap();
    tab.press("← Back to editing");
    tab.type_into("editor", "{\"a\": 2,}\n");
    tab.harness.ui_from(seq, save, None);
    tab.settle();
    assert!(tab.fake.writes().is_empty());
}

#[test]
fn a_file_already_broken_when_opened_follows_the_same_rule() {
    let mut tab = Setup::new().path("a.toml").file("a.toml", "a = \n").open();
    assert!(!tab.kit().text.as_ref().unwrap().check.ok);
    tab.type_into("editor", "a = \nb = 2\n");
    tab.press("Review changes");
    assert!(!tab.enabled("Save"));
    assert!(tab.fake.writes().is_empty());
}

#[test]
fn a_new_text_file_the_whole_of_it_is_the_diff_and_it_is_created() {
    let mut tab = Setup::new().path("conf/new.yaml").open();
    assert_eq!(tab.kit().screen, Screen::Text);
    assert!(!tab.kit().exists);
    tab.type_into("editor", "a: 1\n");
    tab.press("Review changes");
    tab.press("Save");
    assert_eq!(tab.store().get("conf/new.yaml"), Some("a: 1\n"));
    assert_eq!(tab.fake.writes()[0].params.get("expected"), None);
}

#[test]
fn ctrl_s_saves_the_form_and_reviews_the_text() {
    let mut form = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .ignored()
        .open();
    form.edit_row("A", "2");
    form.shortcut("mod+s");
    assert_eq!(form.store().get(".env"), Some("A=2\n"));

    let mut text = Setup::new().path("a.txt").file("a.txt", "x\n").open();
    text.type_into("editor", "y\n");
    text.shortcut("mod+s");
    assert_eq!(text.kit().text.as_ref().unwrap().stage, Stage::Review);
    assert!(text.fake.writes().is_empty(), "reviewing is not saving");
    assert!(
        text.harness.subscriptions().shortcuts.is_empty(),
        "nothing to do with it in the review"
    );
}

/* ── conflicts ────────────────────────────────────────────────────── */

fn edited_then_moved(caller: bool) -> common::Tab {
    let mut setup = Setup::new().path("a.txt").file("a.txt", "base\n");
    if caller {
        setup = setup.caller(CALLER);
    }
    let mut tab = setup.open();
    tab.type_into("editor", "mine\n");
    tab.press("Review changes");
    tab.fake.store.set("a.txt", "theirs\n");
    tab.press("Save");
    tab
}

#[test]
fn text_conflict_nothing_is_written_until_the_person_chooses_and_both_sides_show() {
    let mut tab = edited_then_moved(true);
    assert_eq!(
        tab.store().get("a.txt"),
        Some("theirs\n"),
        "never overwritten silently"
    );
    let conflict = tab.kit().conflict.clone().unwrap();
    assert_eq!(conflict.theirs.as_deref(), Some("theirs\n"));
    let ConflictKind::Text { hunks } = &conflict.kind else {
        panic!("a text conflict");
    };
    assert_eq!(
        changes(hunks),
        [
            (Kind::Del, "theirs".to_owned()),
            (Kind::Add, "mine".to_owned())
        ]
    );
    assert!(
        tab.fake.sent().is_empty(),
        "no answer for what was not written"
    );

    tab.press("Replace it with mine");
    assert_eq!(tab.store().get("a.txt"), Some("mine\n"));
    assert_eq!(
        tab.fake.writes().last().unwrap().params["expected"],
        "theirs\n",
        "measured against what the person just saw"
    );
    assert!(tab.kit().conflict.is_none());
    assert_eq!(tab.fake.sent().len(), 1);
}

#[test]
fn text_conflict_taking_theirs_drops_mine_and_writes_nothing() {
    let mut tab = edited_then_moved(false);
    let before = tab.fake.writes().len();
    tab.press("Keep what is on disk, lose mine");
    assert_eq!(tab.kit().text.as_ref().unwrap().value, "theirs\n");
    assert!(!tab.kit().is_dirty());
    assert_eq!(tab.fake.writes().len(), before);
    assert_eq!(tab.store().get("a.txt"), Some("theirs\n"));
}

#[test]
fn text_conflict_back_to_editing_keeps_mine_against_what_the_disk_holds_now() {
    let mut tab = edited_then_moved(false);
    tab.press("Back to editing, with mine");
    let text = tab.kit().text.as_ref().unwrap();
    assert_eq!(text.value, "mine\n");
    assert_eq!(text.original, "theirs\n");
    assert_eq!(text.stage, Stage::Edit);
    tab.press("Review changes");
    tab.press("Save");
    assert_eq!(tab.store().get("a.txt"), Some("mine\n"));
}

#[test]
fn the_file_moved_twice_a_second_conflict_is_a_conflict_again() {
    let mut tab = edited_then_moved(false);
    tab.fake.store.set("a.txt", "theirs again\n");
    tab.press("Replace it with mine");
    assert_eq!(tab.store().get("a.txt"), Some("theirs again\n"));
    assert_eq!(
        tab.kit().conflict.as_ref().unwrap().theirs.as_deref(),
        Some("theirs again\n")
    );
}

#[test]
fn a_file_deleted_meanwhile_is_a_conflict_and_writing_it_back_is_a_choice() {
    let mut tab = Setup::new().path("a.txt").file("a.txt", "base\n").open();
    tab.type_into("editor", "mine\n");
    tab.press("Review changes");
    tab.fake.store.delete("a.txt");
    tab.press("Save");
    assert!(!tab.store().has("a.txt"));
    assert!(tab.kit().conflict.as_ref().unwrap().gone());
    assert!(tab.shows("The file was deleted while you were editing it. Nothing was written."));
    tab.press("Write mine");
    assert_eq!(tab.store().get("a.txt"), Some("mine\n"));
}

#[test]
fn a_new_file_that_appeared_meanwhile_is_a_conflict_too() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "A")
        .ignored()
        .open();
    tab.edit_row("A", "mine");
    tab.fake.store.set(".env", "A=theirs\n");
    tab.press("Save");
    assert_eq!(
        tab.store().get(".env"),
        Some("A=theirs\n"),
        "not overwritten though nothing was expected"
    );
    assert!(matches!(
        tab.kit().conflict.as_ref().unwrap().kind,
        ConflictKind::Dotenv { .. }
    ));
}

#[test]
fn dotenv_conflict_key_by_key_hidden_and_my_keys_go_on_top_of_what_the_disk_holds() {
    let mut tab = Setup::new()
        .path(".env")
        .caller(CALLER)
        .file(".env", "# base\nA=1\nB=2\nC=3\n")
        .ignored()
        .open();
    tab.edit_row("A", "mine-a");
    tab.edit_row("B", "mine-b");
    tab.fake
        .store
        .set(".env", "# moved\nA=theirs-a\nB=2\nC=3\nD=new\n");
    tab.press("Save");

    assert_eq!(
        tab.store().get(".env"),
        Some("# moved\nA=theirs-a\nB=2\nC=3\nD=new\n")
    );
    let conflict = tab.kit().conflict.clone().unwrap();
    let ConflictKind::Dotenv {
        rows,
        changed_on_disk,
        ..
    } = &conflict.kind
    else {
        panic!("a dotenv conflict");
    };
    let shown: Vec<(&str, &str, Option<&str>, Side, bool)> = rows
        .iter()
        .map(|row| {
            (
                row.key.as_str(),
                row.mine.as_str(),
                row.theirs.as_deref(),
                row.choice,
                row.revealed,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("A", "mine-a", Some("theirs-a"), Side::Mine, false),
            ("B", "mine-b", Some("2"), Side::Mine, false),
        ]
    );
    assert_eq!(*changed_on_disk, 1, "D");
    assert!(tab.fake.sent().is_empty());

    tab.toggle_reveal("A");
    let ConflictKind::Dotenv { rows, .. } = &tab.kit().conflict.as_ref().unwrap().kind else {
        panic!("still a conflict");
    };
    assert!(rows[0].revealed);
    tab.choose_side("A", "theirs");
    tab.press("Save with these choices");
    assert_eq!(
        tab.store().get(".env"),
        Some("# moved\nA=theirs-a\nB=mine-b\nC=3\nD=new\n"),
        "their comment and their key stay"
    );
    assert_eq!(
        tab.fake.writes().last().unwrap().params["expected"],
        "# moved\nA=theirs-a\nB=2\nC=3\nD=new\n"
    );
    assert!(tab.fake.sent()[0].1.contains("Set: B."));
    assert!(tab.kit().conflict.is_none());
    assert_eq!(tab.row("D").value, "new", "the form now reads the new file");
}

#[test]
fn dotenv_conflict_when_nothing_collides_it_still_asks() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\nB=2\n")
        .ignored()
        .open();
    tab.edit_row("A", "mine");
    tab.fake.store.set(".env", "A=1\nB=theirs\n");
    tab.press("Save");
    let ConflictKind::Dotenv { rows, .. } = &tab.kit().conflict.as_ref().unwrap().kind else {
        panic!("a dotenv conflict");
    };
    assert_eq!(rows.len(), 1, "A: mine is not what the disk holds");
    assert_eq!(tab.store().get(".env"), Some("A=1\nB=theirs\n"));
    tab.press("Keep what is on disk, lose mine");
    assert_eq!(tab.row("B").value, "theirs");
    assert_eq!(tab.row("A").value, "1");
    assert!(!tab.kit().is_dirty());
}

/* ── who asked ────────────────────────────────────────────────────── */

#[test]
fn the_agent_is_answered_on_save_and_only_when_one_asked() {
    let mut text = Setup::new()
        .path("a.txt")
        .caller(CALLER)
        .file("a.txt", "x\n")
        .open();
    text.type_into("editor", "y\n");
    text.press("Review changes");
    text.press("Save");
    assert_eq!(
        text.fake.sent(),
        [(
            "Asker".to_owned(),
            "The person saved `a.txt`: the file changed.".to_owned()
        )]
    );
    assert_eq!(text.kit().told.as_ref().unwrap().status, TellStatus::Sent);

    let mut nobody = Setup::new().path("a.txt").file("a.txt", "x\n").open();
    nobody.type_into("editor", "y\n");
    nobody.press("Review changes");
    nobody.press("Save");
    assert!(nobody.fake.sent().is_empty());
    assert!(nobody.kit().told.is_none());
}

#[test]
fn nothing_to_change_the_waiting_agent_can_still_be_told_with_names_only() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "SET_KEY,EMPTY_KEY")
        .caller(CALLER)
        .file(".env", "SET_KEY=abc\nEMPTY_KEY=\n")
        .ignored()
        .open();
    tab.press("Tell Asker: nothing to change");
    assert_eq!(
        tab.fake.sent()[0].1,
        "The person checked `.env` and changed nothing. Already set: SET_KEY. Still empty: EMPTY_KEY. The values stay in the file: do not read them back, use them through the program that loads it."
    );
    assert_eq!(tab.kit().told.as_ref().unwrap().status, TellStatus::Checked);
    assert!(tab.fake.writes().is_empty());

    let mut text = Setup::new()
        .path("a.txt")
        .caller(CALLER)
        .file("a.txt", "x\n")
        .open();
    text.press("Tell Asker: nothing to change");
    assert_eq!(
        text.fake.sent()[0].1,
        "The person checked `a.txt` and changed nothing."
    );
    text.type_into("editor", "y\n");
    assert!(
        text.button("Tell Asker: nothing to change").is_none(),
        "not while something is unsaved"
    );
    assert_eq!(text.fake.sent().len(), 1);
}

#[test]
fn an_agent_that_cannot_be_told_saved_all_the_same_it_says_so_and_it_can_try_again() {
    let mut tab = Setup::new()
        .path(".env")
        .caller(CALLER)
        .file(".env", "A=1\n")
        .ignored()
        .send_error("not_found")
        .open();
    tab.edit_row("A", "2");
    tab.press("Save");
    assert_eq!(tab.store().get(".env"), Some("A=2\n"));
    assert!(tab.kit().saved);
    let told = tab.kit().told.clone().unwrap();
    assert_eq!(
        (told.status, told.pane.as_str(), told.code.as_deref()),
        (TellStatus::Failed, "Asker", Some("not_found"))
    );
    tab.fake.send_error = None;
    tab.press("Tell Asker again");
    assert_eq!(tab.kit().told.as_ref().unwrap().status, TellStatus::Sent);
}

#[test]
fn asked_again_the_latest_to_ask_is_answered_and_what_was_typed_stays() {
    let mut tab = Setup::new()
        .path(".env")
        .param("keys", "A")
        .caller(CALLER)
        .file(".env", "A=\nB=2\n")
        .ignored()
        .open();
    tab.edit_row("A", "typing");
    tab.event(
        "tab.params",
        json!({ "params": { "path": ".env", "keys": "A,NEW", "why": "Now this too." }, "caller": { "pane": "Other" } }),
    );
    assert_eq!(tab.kit().caller.as_deref(), Some("Other"));
    assert_eq!(tab.row("A").value, "typing");
    assert_eq!(tab.kit().why.as_deref(), Some("Now this too."));
    let rows: Vec<(String, bool)> = tab
        .kit()
        .form
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|row| (row.key.clone(), row.asked))
        .collect();
    assert_eq!(
        rows,
        [
            ("A".to_owned(), true),
            ("NEW".to_owned(), true),
            ("B".to_owned(), false)
        ]
    );
    let reads = tab
        .fake
        .calls_to("files.read")
        .into_iter()
        .filter(|call| call.params["path"] == ".env")
        .count();
    assert_eq!(reads, 1, "not read again");
    tab.press("Save");
    assert_eq!(tab.fake.sent()[0].0, "Other");
}

#[test]
fn asked_again_after_a_save_the_new_asker_can_be_told_there_is_nothing_more_to_change() {
    let mut tab = Setup::new()
        .path("a.txt")
        .caller(CALLER)
        .file("a.txt", "x\n")
        .open();
    tab.type_into("editor", "y\n");
    tab.press("Review changes");
    tab.press("Save");
    assert!(tab.kit().saved);
    tab.event(
        "tab.params",
        json!({ "params": { "path": "a.txt" }, "caller": { "pane": "Other" } }),
    );
    assert!(!tab.kit().saved);
    assert!(tab.kit().told.is_none());
    tab.press("Tell Other: nothing to change");
    assert_eq!(
        tab.fake.sent().last().unwrap(),
        &(
            "Other".to_owned(),
            "The person checked `a.txt` and changed nothing.".to_owned()
        )
    );
}

#[test]
fn the_person_opening_it_again_takes_the_caller_away_and_nobody_is_told() {
    let mut tab = Setup::new()
        .path("a.txt")
        .caller(CALLER)
        .file("a.txt", "x\n")
        .open();
    tab.event(
        "tab.params",
        json!({ "params": { "path": "a.txt" }, "caller": null }),
    );
    assert!(tab.kit().caller.is_none());
    tab.type_into("editor", "y\n");
    tab.press("Review changes");
    tab.press("Save");
    assert!(tab.fake.sent().is_empty());
}

#[test]
fn asked_for_another_file_it_opens_that_one() {
    let mut tab = Setup::new()
        .path("a.txt")
        .file("a.txt", "x\n")
        .file("b.txt", "y\n")
        .open();
    tab.event(
        "tab.params",
        json!({ "params": { "path": "b.txt" }, "caller": null }),
    );
    assert_eq!(tab.kit().path.as_deref(), Some("b.txt"));
    assert_eq!(tab.kit().text.as_ref().unwrap().value, "y\n");
}

/* ── what Hangar refuses ──────────────────────────────────────────── */

#[test]
fn reading_denied_a_page_that_says_what_to_do_with_nothing_written() {
    let tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .deny("files.read")
        .open();
    assert_eq!(tab.kit().screen, Screen::Error);
    assert_eq!(tab.kit().error.as_ref().unwrap().kind, "denied");
    assert!(tab.fake.writes().is_empty());
}

#[test]
fn listing_denied_the_picker_says_so_too() {
    let tab = Setup::new().deny("files.list").open();
    assert_eq!(tab.kit().screen, Screen::Error);
    assert_eq!(tab.kit().error.as_ref().unwrap().kind, "denied");
}

#[test]
fn writing_denied_or_over_the_limit_the_edits_stay_and_it_says_so() {
    for code in ["denied", "rate_limited", "invalid", "failed"] {
        let mut tab = Setup::new()
            .path(".env")
            .file(".env", "A=1\n")
            .ignored()
            .open();
        tab.fake.write_error = Some(code.into());
        tab.edit_row("A", "typed");
        tab.press("Save");
        let expected = if code == "failed" { "other" } else { code };
        assert_eq!(tab.kit().problem.as_ref().unwrap().code, expected);
        assert!(
            tab.shows(&logic::strings("en").t(&format!("write.{expected}"))),
            "{code}"
        );
        assert_eq!(tab.row("A").value, "typed", "{code}");
        assert!(tab.kit().is_dirty());
        assert!(!tab.kit().saved);
        assert!(!tab.kit().busy);
        tab.fake.write_error = None;
        tab.press("Save");
        assert_eq!(tab.store().get(".env"), Some("A=typed\n"));
        assert!(tab.kit().problem.is_none());
    }
}

#[test]
fn panes_send_denied_saved_and_it_says_the_agent_was_not_told() {
    let mut tab = Setup::new()
        .path("a.txt")
        .caller(CALLER)
        .file("a.txt", "x\n")
        .deny("panes.send")
        .open();
    tab.type_into("editor", "y\n");
    tab.press("Review changes");
    tab.press("Save");
    assert_eq!(tab.store().get("a.txt"), Some("y\n"));
    assert_eq!(
        tab.kit().told.as_ref().unwrap().code.as_deref(),
        Some("denied")
    );
}

#[test]
fn a_title_or_a_badge_refused_changes_nothing() {
    let tab = Setup::new()
        .path(".env")
        .param("keys", "A")
        .file(".env", "")
        .ignored()
        .deny("tab.setTitle")
        .deny("tab.setBadge")
        .open();
    assert_eq!(tab.kit().screen, Screen::Dotenv);
}

#[test]
fn a_path_that_can_never_open_said_as_such_and_hangar_is_not_asked() {
    for path in [
        "../outside",
        "/etc/passwd",
        "C:/x",
        ".git/config",
        "a/../b",
        "sub\\.GIT\\hooks",
        "a. ",
    ] {
        let tab = Setup::new().path(path).open();
        assert_eq!(tab.kit().screen, Screen::Error, "{path}");
        assert_eq!(tab.kit().error.as_ref().unwrap().kind, "bad-path", "{path}");
        assert!(tab.fake.calls_to("files.read").is_empty(), "{path}");
    }
}

#[test]
fn a_file_hangar_cannot_read_shows_why() {
    let tab = Setup::new()
        .path("big.txt")
        .read_error("big.txt", "failed", "big.txt is larger than 1 MB")
        .open();
    let error = tab.kit().error.as_ref().unwrap();
    assert_eq!(error.kind, "failed");
    assert_eq!(error.detail, "big.txt is larger than 1 MB");
}

#[test]
fn try_again_reads_the_file_again() {
    let mut tab = Setup::new()
        .path("a.txt")
        .read_error("a.txt", "unavailable", "gone")
        .open();
    assert_eq!(tab.kit().error.as_ref().unwrap().kind, "unavailable");
    tab.fake.read_errors.clear();
    tab.fake.store.set("a.txt", "back\n");
    tab.press("Try again");
    assert_eq!(tab.kit().text.as_ref().unwrap().value, "back\n");
}

/* ── the picker ───────────────────────────────────────────────────── */

#[test]
fn no_params_a_picker_env_files_first_then_config_what_is_not_text_left_out() {
    let mut tab = Setup::new()
        .files(&[
            ("README.md", ""),
            ("src/app.ts", ""),
            ("logo.png", ""),
            ("package.json", "{}"),
            ("pnpm-lock.yaml", ""),
            ("config/app.yaml", ""),
            (".env.local", "A=1"),
            ("sub/.env", ""),
            (".npmrc", ""),
            ("tsconfig.json", "{}"),
        ])
        .open();
    assert_eq!(tab.kit().screen, Screen::Picker);
    assert_eq!(tab.fake.calls_to("files.list").len(), 1);
    let groups = &tab.kit().picker.as_ref().unwrap().groups;
    let env: Vec<(&str, bool)> = groups
        .env
        .iter()
        .map(|file| (file.path.as_str(), file.is_new))
        .collect();
    assert_eq!(
        env,
        [(".env", true), (".env.local", false), ("sub/.env", false)]
    );
    let config: Vec<&str> = groups
        .config
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        config,
        ["package.json", "config/app.yaml", ".npmrc", "tsconfig.json"]
    );
    let other: Vec<&str> = groups.other.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(other, ["README.md", "src/app.ts", "pnpm-lock.yaml"]);

    tab.press(".env.local");
    assert_eq!(tab.kit().screen, Screen::Dotenv);
    assert!(tab.kit().from_picker);
    assert_eq!(tab.kit().path.as_deref(), Some(".env.local"));
    assert_eq!(
        tab.fake.calls_to("tab.setTitle").last().unwrap().params["title"],
        ".env.local"
    );

    tab.press("← Files");
    assert_eq!(tab.kit().screen, Screen::Picker);
    assert!(tab.kit().path.is_none());
}

#[test]
fn the_pickers_filter_and_a_listing_that_was_cut() {
    let mut paths = vec!["a.yaml".to_owned(), "b/.env".to_owned()];
    paths.extend((0..450).map(|i| format!("src/f{i}.ts")));
    let groups = logic::rank_files(paths.iter().map(String::as_str));
    let counts: Vec<usize> = logic::filter_groups(&groups, "YAML")
        .iter()
        .map(|group| group.files.len())
        .collect();
    assert_eq!(counts, [0, 1, 0], "any case");
    let all = logic::filter_groups(&groups, "");
    assert_eq!(all[2].files.len(), 200);
    assert_eq!(all[2].more, 250);
    let tab = Setup::new().file("a.txt", "").truncated().open();
    assert!(tab.kit().picker.as_ref().unwrap().truncated);
    assert!(tab.shows("This folder holds more files than Hangar lists"));
}

/* ── params, read like form input ─────────────────────────────────── */

#[test]
fn params() {
    assert_eq!(
        logic::parse_keys(Some("A, B ,A,not a key,1BAD,C.D")),
        ["A", "B", "C.D"]
    );
    assert!(logic::parse_keys(None).is_empty());
    assert_eq!(logic::parse_line(Some("12")), Some(12));
    for bad in ["0", "-1", "1e3", "abc", ""] {
        assert_eq!(logic::parse_line(Some(bad)), None, "{bad}");
    }
    assert_eq!(logic::parse_line(None), None);
    assert_eq!(
        logic::check_path("a\\b/./c.txt").as_deref(),
        Some("a/b/c.txt")
    );
    assert_eq!(logic::check_path(".env").as_deref(), Some(".env"));
    assert_eq!(
        logic::check_path(".github/x.yml").as_deref(),
        Some(".github/x.yml")
    );
    let params = [("path", ".env"), ("why", "  why  "), ("keys", "A")]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    let read = logic::read_params(&params, Some("P"));
    assert_eq!(
        read,
        logic::Params {
            path: Some(".env".into()),
            bad_path: false,
            keys: vec!["A".into()],
            why: Some("why".into()),
            line: None,
            caller: Some("P".into()),
        }
    );
    assert_eq!(
        logic::read_params(&Default::default(), Some("")).caller,
        None
    );
    assert_eq!(
        logic::tab_title(&format!("a/{}", "x".repeat(60)))
            .chars()
            .count(),
        40
    );
}

#[test]
fn the_language_follows_hangars() {
    let mut tab = Setup::new()
        .path(".env")
        .file(".env", "A=1\n")
        .ignored()
        .open();
    assert!(tab.shows("In this file"));
    tab.event("theme", json!({ "scheme": "dark", "locale": "fr" }));
    assert!(tab.shows("Dans ce fichier"));
}
