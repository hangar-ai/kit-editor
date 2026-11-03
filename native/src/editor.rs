//! The tab's behaviour: controller.js, as a [`Kit`]. Each `await` of the page
//! is a call here, answered by a message of its own, and each answer is
//! measured against `generation`, as the page's were: a later file makes the
//! earlier answers stale. Values go to `files.write` and nowhere else; the
//! one text an agent gets is made by `answer`, from names and counts.

use std::collections::{HashMap, HashSet, VecDeque};

use hangar_kit_sdk::api::{
    BadgeState, Done, FileListing, KitError, Queued, TabParamsAnswer, Theme,
};
use hangar_kit_sdk::prelude::*;
use hangar_kit_sdk::ui::Range;

use crate::answer;
use crate::diff::{self, Hunk};
use crate::dotenv::{self, Doc, Edits};
use crate::gitignore;
use crate::js;
use crate::logic::{self, Groups, Params};
use crate::validate::{self, Check};

/// Larger texts are checked when reviewing, not at each keystroke.
const VALIDATE_WHILE_TYPING_MAX: usize = 100_000;
/// The diff's lines given to Hangar at once: it asks for others as they scroll in.
pub const DIFF_WINDOW: u32 = 400;

/// What the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Loading,
    Picker,
    Dotenv,
    Text,
    Error,
}

/// A key of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRow {
    pub key: String,
    pub asked: bool,
    /// In the file when it was read.
    pub present: bool,
    pub original: Option<String>,
    pub value: String,
    pub hint: String,
    pub count: usize,
    pub revealed: bool,
    /// Added by a click: written even empty.
    pub added: bool,
    /// Typed by the person: counted, never named, to the agent.
    pub typed: bool,
}

impl KeyRow {
    fn fresh(key: String, asked: bool, hint: String, added: bool, typed: bool) -> Self {
        KeyRow {
            key,
            asked,
            present: false,
            original: None,
            value: String::new(),
            hint,
            count: 0,
            revealed: false,
            added,
            typed,
        }
    }

    pub fn missing(&self) -> bool {
        self.asked && self.value.is_empty()
    }

    pub fn changed(&self) -> bool {
        match &self.original {
            Some(original) if self.present => self.value != *original,
            _ => !self.value.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub key: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub rows: Vec<KeyRow>,
    /// Keys of the template, missing here.
    pub offered: Vec<Offer>,
    pub example_name: Option<String>,
    pub other_lines: usize,
    pub new_key: String,
    /// Its string's key: `newKeyBad`, `newKeyTaken`.
    pub new_key_problem: Option<&'static str>,
}

impl Form {
    fn row_mut(&mut self, key: &str) -> Option<&mut KeyRow> {
        self.rows.iter_mut().find(|row| row.key == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreStatus {
    Ignored,
    NotIgnored,
    NoFile,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignore {
    pub status: IgnoreStatus,
    pub busy: bool,
    /// What the write of .gitignore was answered: a code.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Edit,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextState {
    pub value: String,
    /// What the disk holds, as the editor holds it (`\n` only).
    pub original: String,
    pub stage: Stage,
    pub check: Check,
    /// The person said they know it is broken.
    pub ack: bool,
    pub hunks: Option<Vec<Hunk>>,
    /// The line asked for, marked.
    pub target: Option<u32>,
    /// The line asked for and how many the file has, when it is past its end.
    pub far: Option<(u32, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Mine,
    Theirs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRow {
    pub key: String,
    pub mine: String,
    /// None: absent on disk.
    pub theirs: Option<String>,
    pub choice: Side,
    pub revealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    Text {
        hunks: Vec<Hunk>,
    },
    Dotenv {
        rows: Vec<ConflictRow>,
        edits: Edits,
        changed_on_disk: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// What the disk holds now; none when the file is gone.
    pub theirs: Option<String>,
    pub kind: ConflictKind,
}

impl Conflict {
    pub fn gone(&self) -> bool {
        self.theirs.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TellStatus {
    Sent,
    Checked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    pub status: TellStatus,
    pub pane: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// `denied`, `rate_limited`, `invalid` or `other`.
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageError {
    /// `denied`, `invalid`, `bad-path`, `failed`, `unavailable`, `timeout`, `error`.
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub groups: Groups,
    pub filter: String,
    pub truncated: bool,
}

/// The .gitignore files that have a say, read one after the other.
#[derive(Debug, Clone)]
struct Scan {
    remaining: VecDeque<(String, String)>,
    reading: Option<String>,
    found: Vec<gitignore::Source>,
}

impl Scan {
    /// The scan, or the answer at once: a template is meant to be committed.
    fn of(path: &str) -> Result<Scan, IgnoreStatus> {
        if dotenv::is_example_path(path) {
            return Err(IgnoreStatus::Ignored);
        }
        Ok(Scan {
            remaining: gitignore::sources_for(path).into(),
            reading: None,
            found: Vec::new(),
        })
    }

    /// The next file to read, or none when all are read.
    fn next(&mut self) -> Option<String> {
        let (dir, path) = self.remaining.pop_front()?;
        self.reading = Some(dir);
        Some(path)
    }

    /// The answer to the read of the last file asked for: none to go on.
    fn take(&mut self, answer: Result<String, KitError>) -> Option<IgnoreStatus> {
        let dir = self.reading.take().unwrap_or_default();
        match answer {
            Ok(text) => self.found.push(gitignore::Source { dir, text }),
            Err(error) if error.is(KitError::NOT_FOUND) => {}
            Err(_) => return Some(IgnoreStatus::Unknown),
        }
        None
    }

    fn status(&self, path: &str) -> IgnoreStatus {
        if self.found.is_empty() {
            IgnoreStatus::NoFile
        } else if gitignore::is_ignored(&self.found, path) {
            IgnoreStatus::Ignored
        } else {
            IgnoreStatus::NotIgnored
        }
    }
}

/// A dotenv file being opened: its template looked for, then git asked.
#[derive(Debug, Clone)]
struct Opening {
    token: u64,
    path: String,
    text: Option<String>,
    candidates: VecDeque<String>,
    reading: Option<String>,
    example: Option<(String, Doc)>,
    scan: Option<Scan>,
}

/// A line being added to .gitignore, then git asked again.
#[derive(Debug, Clone)]
struct Adding {
    token: u64,
    path: String,
    scan: Option<Scan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveKind {
    Form,
    Text,
}

/// A save under way: `files.write`, with what was read as `expected`, or
/// after a look that the new file is still absent.
#[derive(Debug, Clone)]
struct Saving {
    token: u64,
    text: String,
    kind: SaveKind,
    expected: bool,
}

enum Outcome {
    Written,
    Conflict(Option<String>),
    Refused(KitError),
}

/// What comes once the agent's answer is sent, or could not be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterSend {
    /// A save ends.
    Saved(u64),
    /// A message sent on its own ends.
    Done(u64),
}

#[derive(Debug, Clone)]
pub enum Message {
    // Hangar.
    Params(TabParamsAnswer),
    Theme(Theme),
    Opened {
        token: u64,
        line: Option<u32>,
        answer: Result<String, KitError>,
    },
    ExampleRead {
        token: u64,
        answer: Result<String, KitError>,
    },
    IgnoreRead {
        token: u64,
        answer: Result<String, KitError>,
    },
    Listed {
        token: u64,
        answer: Result<FileListing, KitError>,
    },
    SaveChecked {
        token: u64,
        answer: Result<String, KitError>,
    },
    SaveWritten {
        token: u64,
        answer: Result<Done, KitError>,
    },
    SaveTheirs {
        token: u64,
        answer: Result<String, KitError>,
    },
    Sent {
        token: u64,
        pane: String,
        status: TellStatus,
        after: AfterSend,
        answer: Result<Queued, KitError>,
    },
    GitignoreRead {
        token: u64,
        answer: Result<String, KitError>,
    },
    GitignoreWritten {
        token: u64,
        answer: Result<Done, KitError>,
    },
    AddScanRead {
        token: u64,
        answer: Result<String, KitError>,
    },
    Leave(bool),
    // The person.
    Pick(String),
    Filter(String),
    Back,
    Retry,
    EditRow(String, String),
    ToggleReveal(String),
    AddOffered(String),
    AddAllOffered,
    NewKey(String),
    AddNewKey,
    AddToGitignore,
    SaveForm,
    EditText(String),
    Review,
    BackToEdit,
    Ack(bool),
    SaveText,
    DiffRange(Range),
    ConflictChoose(String, Side),
    ConflictReveal(String),
    ConflictKeepMine,
    ConflictTakeTheirs,
    ConflictBack,
    TellNoChange,
    RetryTell,
    /// Ctrl+S: in the form it saves, in the editor it reviews.
    SaveKey,
}

pub struct Editor {
    pub(crate) t: Strings,
    pub screen: Screen,
    pub path: Option<String>,
    /// The pane that asked.
    pub caller: Option<String>,
    pub why: Option<String>,
    pub asked: Vec<String>,
    pub from_picker: bool,
    pub exists: bool,
    pub busy: bool,
    pub problem: Option<Problem>,
    pub saved: bool,
    pub told: Option<Told>,
    pub error: Option<PageError>,
    pub picker: Option<Picker>,
    pub form: Option<Form>,
    pub ignore: Option<Ignore>,
    pub text: Option<TextState>,
    pub conflict: Option<Conflict>,
    /// The diff's lines given: from, to.
    pub diff_range: (u32, u32),

    generation: u64,
    last_title: Option<String>,
    last_badge: Option<String>,
    /// The dotenv file as last read or written.
    doc: Doc,
    /// The exact text the file held when last read; none when absent.
    last_read: Option<String>,
    example_rows: Vec<dotenv::Row>,
    example_name: Option<String>,
    example_values: Vec<String>,
    /// An answer that could not be sent, to try again as it is.
    unsent: Option<(String, TellStatus)>,
    opening: Option<Opening>,
    adding: Option<Adding>,
    saving: Option<Saving>,
}

/// Names, and the values only to keep them unsaid.
struct DotenvFacts {
    known: HashSet<String>,
    values: Vec<String>,
    still_empty: Vec<String>,
    already_set: Vec<String>,
}

impl Editor {
    pub fn t(&self) -> &Strings {
        &self.t
    }

    fn set_title(&mut self, title: String, cx: &mut Context<Message>) {
        if self.last_title.as_ref() == Some(&title) {
            return;
        }
        self.last_title = Some(title.clone());
        // A refused title leaves the tab's own.
        cx.call_quietly(api::TabSetTitle { title });
    }

    pub fn missing_count(&self) -> usize {
        match (&self.form, self.screen) {
            (Some(form), Screen::Dotenv) => form.rows.iter().filter(|row| row.missing()).count(),
            _ => 0,
        }
    }

    fn set_badge(&mut self, cx: &mut Context<Message>) {
        let missing = self.missing_count();
        let badge = if missing > 0 {
            Some((
                BadgeState::Neutral,
                self.t.tn("badgeMissing", missing as i64, &[]),
            ))
        } else if self.saved {
            Some((BadgeState::Success, self.t.t("badgeSaved")))
        } else {
            None
        };
        let badge = badge.map(|(state, text)| (state, js::first_chars(&text, 12)));
        let key = match &badge {
            Some((state, text)) => format!("{state:?}|{text}"),
            None => "none".to_owned(),
        };
        if self.last_badge.as_ref() == Some(&key) {
            return;
        }
        self.last_badge = Some(key);
        let (state, text) = match badge {
            Some((state, text)) => (Some(state), Some(text)),
            None => (None, None),
        };
        cx.call_quietly(api::TabSetBadge { state, text });
    }

    fn show_error(
        &mut self,
        error: Option<&KitError>,
        kind: Option<&str>,
        cx: &mut Context<Message>,
    ) {
        const KNOWN: [&str; 5] = ["denied", "invalid", "failed", "unavailable", "timeout"];
        let code = error.map(|error| error.code.as_str()).unwrap_or("");
        let kind = kind.unwrap_or(if KNOWN.contains(&code) { code } else { "error" });
        self.screen = Screen::Error;
        self.error = Some(PageError {
            kind: kind.to_owned(),
            detail: error.map(|error| error.message.clone()).unwrap_or_default(),
        });
        self.busy = false;
        self.set_badge(cx);
    }

    /// Everything a file shows, cleared, and every answer still on its way
    /// made stale.
    fn reset(&mut self) -> u64 {
        self.generation += 1;
        self.screen = Screen::Loading;
        self.busy = false;
        self.problem = None;
        self.saved = false;
        self.told = None;
        self.error = None;
        self.conflict = None;
        self.form = None;
        self.text = None;
        self.ignore = None;
        self.unsent = None;
        self.opening = None;
        self.adding = None;
        self.saving = None;
        self.generation
    }

    /* ── the agent's answer ───────────────────────────────────────── */

    /// The one place that speaks to an agent. `message` comes from
    /// `answer`, which is handed no value.
    fn send(
        &mut self,
        message: String,
        status: TellStatus,
        after: AfterSend,
        cx: &mut Context<Message>,
    ) {
        let Some(pane) = self.caller.clone() else {
            self.told = None;
            self.unsent = None;
            self.after_send(after, cx);
            return;
        };
        self.unsent = Some((message.clone(), status));
        let token = self.generation;
        cx.call(
            api::PanesSend {
                pane: pane.clone(),
                text: message,
            },
            move |answer| Message::Sent {
                token,
                pane,
                status,
                after,
                answer,
            },
        );
    }

    fn after_send(&mut self, after: AfterSend, cx: &mut Context<Message>) {
        match after {
            AfterSend::Saved(token) if token == self.generation => {
                self.busy = false;
                self.set_badge(cx);
            }
            AfterSend::Done(token) if token == self.generation => self.busy = false,
            _ => {}
        }
    }

    fn retry_tell(&mut self, cx: &mut Context<Message>) {
        if self.busy {
            return;
        }
        let Some((message, status)) = self.unsent.clone() else {
            return;
        };
        self.busy = true;
        self.send(message, status, AfterSend::Done(self.generation), cx);
    }

    /* ── dotenv: the form ─────────────────────────────────────────── */

    fn build_form(&self, doc: &Doc) -> Form {
        let present = dotenv::rows(doc);
        let by_key: HashMap<&str, &dotenv::Row> =
            present.iter().map(|row| (row.key.as_str(), row)).collect();
        let hints: HashMap<&str, &str> = self
            .example_rows
            .iter()
            .map(|row| (row.key.as_str(), row.hint.as_str()))
            .collect();
        let make_row = |key: &str, asked: bool| {
            let found = by_key.get(key);
            let hint = found
                .map(|row| row.hint.as_str())
                .filter(|hint| !hint.is_empty())
                .or_else(|| hints.get(key).copied())
                .unwrap_or("");
            KeyRow {
                key: key.to_owned(),
                asked,
                present: found.is_some(),
                original: found.map(|row| row.value.clone()),
                value: found.map(|row| row.value.clone()).unwrap_or_default(),
                hint: hint.to_owned(),
                count: found.map_or(0, |row| row.count),
                revealed: false,
                added: false,
                typed: false,
            }
        };
        let mut rows: Vec<KeyRow> = self.asked.iter().map(|key| make_row(key, true)).collect();
        for row in &present {
            if !self.asked.contains(&row.key) {
                rows.push(make_row(&row.key, false));
            }
        }
        let shown: HashSet<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        let offered = self
            .example_rows
            .iter()
            .filter(|row| !shown.contains(row.key.as_str()))
            .map(|row| Offer {
                key: row.key.clone(),
                hint: row.hint.clone(),
            })
            .collect();
        Form {
            rows,
            offered,
            example_name: self.example_name.clone(),
            other_lines: dotenv::other_lines(doc),
            new_key: String::new(),
            new_key_problem: None,
        }
    }

    /// What the form changes: key → value.
    pub fn form_edits(&self) -> Edits {
        let Some(form) = &self.form else {
            return Vec::new();
        };
        form.rows
            .iter()
            .filter(|row| {
                if row.present {
                    row.original.as_ref() != Some(&row.value)
                } else {
                    row.added || !row.value.is_empty()
                }
            })
            .map(|row| (row.key.clone(), row.value.clone()))
            .collect()
    }

    fn start_opening(&mut self, path: String, text: Option<String>, cx: &mut Context<Message>) {
        let candidates = if dotenv::is_example_path(&path) {
            VecDeque::new()
        } else {
            dotenv::example_candidates(&path).into()
        };
        self.opening = Some(Opening {
            token: self.generation,
            path,
            text,
            candidates,
            reading: None,
            example: None,
            scan: None,
        });
        self.step_opening(cx);
    }

    /// The next read of a dotenv file being opened, or the form once all are in.
    fn step_opening(&mut self, cx: &mut Context<Message>) {
        let Some(opening) = &mut self.opening else {
            return;
        };
        let token = opening.token;
        if opening.scan.is_none() {
            if let Some(candidate) = opening.candidates.pop_front() {
                opening.reading = Some(js::base_name(&candidate).to_owned());
                cx.call(api::FilesRead { path: candidate }, move |answer| {
                    Message::ExampleRead { token, answer }
                });
                return;
            }
            match Scan::of(&opening.path) {
                Ok(scan) => opening.scan = Some(scan),
                Err(status) => return self.finish_opening(status, cx),
            }
        }
        let scan = opening.scan.as_mut().expect("scanning");
        match scan.next() {
            Some(path) => cx.call(api::FilesRead { path }, move |answer| Message::IgnoreRead {
                token,
                answer,
            }),
            None => {
                let status = scan.status(&opening.path);
                self.finish_opening(status, cx);
            }
        }
    }

    fn finish_opening(&mut self, ignore: IgnoreStatus, cx: &mut Context<Message>) {
        let Some(opening) = self.opening.take() else {
            return;
        };
        if opening.token != self.generation {
            return;
        }
        self.doc = dotenv::parse(opening.text.as_deref().unwrap_or(""));
        match &opening.example {
            Some((name, doc)) => {
                self.example_rows = dotenv::rows(doc);
                self.example_name = Some(name.clone());
                self.example_values = dotenv::all_values(doc);
            }
            None => {
                self.example_rows = Vec::new();
                self.example_name = None;
                self.example_values = Vec::new();
            }
        }
        self.screen = Screen::Dotenv;
        self.form = Some(self.build_form(&self.doc));
        self.ignore = Some(Ignore {
            status: ignore,
            busy: false,
            problem: None,
        });
        self.text = None;
        self.busy = false;
        self.set_badge(cx);
    }

    fn edit_row(&mut self, key: &str, value: String, cx: &mut Context<Message>) {
        let Some(row) = self.form.as_mut().and_then(|form| form.row_mut(key)) else {
            return;
        };
        row.value = diff::normalize(&value);
        self.saved = false;
        self.set_badge(cx);
    }

    fn toggle_reveal(&mut self, key: &str) {
        if let Some(row) = self.form.as_mut().and_then(|form| form.row_mut(key)) {
            row.revealed = !row.revealed;
        }
    }

    fn add_offered(&mut self, key: &str) {
        let Some(form) = &mut self.form else {
            return;
        };
        let Some(at) = form.offered.iter().position(|offer| offer.key == key) else {
            return;
        };
        let offer = form.offered.remove(at);
        form.rows
            .push(KeyRow::fresh(offer.key, false, offer.hint, true, false));
    }

    fn add_all_offered(&mut self) {
        let keys: Vec<String> = match &self.form {
            Some(form) => form.offered.iter().map(|offer| offer.key.clone()).collect(),
            None => return,
        };
        for key in keys {
            self.add_offered(&key);
        }
    }

    /// A key the person types. It may be a secret pasted in the wrong field,
    /// so it is never named to the agent.
    fn add_new_key(&mut self) {
        let Some(form) = &mut self.form else {
            return;
        };
        let name = js::trim(&form.new_key).to_owned();
        if !dotenv::is_key(&name) {
            form.new_key_problem = Some("newKeyBad");
        } else if form.rows.iter().any(|row| row.key == name) {
            form.new_key_problem = Some("newKeyTaken");
        } else {
            form.offered.retain(|offer| offer.key != name);
            form.rows
                .push(KeyRow::fresh(name, false, String::new(), true, true));
            form.new_key = String::new();
            form.new_key_problem = None;
        }
    }

    /// Names and counts for the agent, read from what the file holds now.
    fn dotenv_facts(&self, before: &Doc, after: &Doc) -> DotenvFacts {
        let value: HashMap<String, String> = dotenv::rows(after)
            .into_iter()
            .map(|row| (row.key, row.value))
            .collect();
        let typed: HashSet<&str> = self
            .form
            .iter()
            .flat_map(|form| &form.rows)
            .filter(|row| row.typed)
            .map(|row| row.key.as_str())
            .collect();
        let mut known: HashSet<String> = self.asked.iter().cloned().collect();
        known.extend(self.example_rows.iter().map(|row| row.key.clone()));
        known.extend(dotenv::rows(before).into_iter().map(|row| row.key));
        known.retain(|name| !typed.contains(name.as_str()));
        let mut values = dotenv::all_values(before);
        values.extend(dotenv::all_values(after));
        values.extend(self.example_values.iter().cloned());
        let filled = |key: &String| value.get(key).is_some_and(|value| !value.is_empty());
        DotenvFacts {
            known,
            values,
            still_empty: self
                .asked
                .iter()
                .filter(|key| !filled(key))
                .cloned()
                .collect(),
            already_set: self
                .asked
                .iter()
                .filter(|key| filled(key))
                .cloned()
                .collect(),
        }
    }

    fn after_form_save(&mut self, written: &str, token: u64, cx: &mut Context<Message>) {
        let before = self.doc.clone();
        let after = dotenv::parse(written);
        let changed = dotenv::changed_keys(&before, &after);
        let facts = self.dotenv_facts(&before, &after);
        let path = self.path.clone().unwrap_or_default();
        let message = answer::dotenv_saved(
            &answer::Facts {
                path: &path,
                known: &facts.known,
                values: &facts.values,
            },
            &changed.set,
            &changed.emptied,
            &facts.still_empty,
        );
        self.form = Some(self.build_form(&after));
        self.doc = after;
        self.exists = true;
        self.saved = true;
        self.send(message, TellStatus::Sent, AfterSend::Saved(token), cx);
    }

    fn save_form(&mut self, cx: &mut Context<Message>) {
        if self.screen != Screen::Dotenv || self.busy || self.conflict.is_some() {
            return;
        }
        let edits = self.form_edits();
        if edits.is_empty() {
            return;
        }
        let text = dotenv::serialize(&dotenv::with_values(&self.doc, &edits));
        self.persist(text, SaveKind::Form, cx);
    }

    fn add_to_gitignore(&mut self, cx: &mut Context<Message>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Some(ignore) = &mut self.ignore else {
            return;
        };
        if ignore.busy {
            return;
        }
        ignore.busy = true;
        ignore.problem = None;
        let token = self.generation;
        self.adding = Some(Adding {
            token,
            path,
            scan: None,
        });
        cx.call(
            api::FilesRead {
                path: ".gitignore".into(),
            },
            move |answer| Message::GitignoreRead { token, answer },
        );
    }

    fn adding_failed(&mut self, code: &str) {
        self.adding = None;
        if let Some(ignore) = &mut self.ignore {
            ignore.busy = false;
            ignore.problem = Some(if code.is_empty() { "other" } else { code }.to_owned());
        }
    }

    fn step_adding(&mut self, cx: &mut Context<Message>) {
        let Some(adding) = &mut self.adding else {
            return;
        };
        let token = adding.token;
        let status = match &mut adding.scan {
            None => match Scan::of(&adding.path) {
                Ok(scan) => {
                    adding.scan = Some(scan);
                    return self.step_adding(cx);
                }
                Err(status) => status,
            },
            Some(scan) => match scan.next() {
                Some(path) => {
                    cx.call(api::FilesRead { path }, move |answer| {
                        Message::AddScanRead { token, answer }
                    });
                    return;
                }
                None => scan.status(&adding.path),
            },
        };
        self.adding = None;
        self.ignore = Some(Ignore {
            status,
            busy: false,
            problem: None,
        });
    }

    /* ── any other text file: the editor ──────────────────────────── */

    fn open_text(
        &mut self,
        path: &str,
        text: Option<&str>,
        line: Option<u32>,
        cx: &mut Context<Message>,
    ) {
        let original = diff::normalize(text.unwrap_or(""));
        let total = original.split('\n').count();
        let jump = line.map(|line| (line as usize).min(total) as u32);
        self.screen = Screen::Text;
        self.form = None;
        self.ignore = None;
        self.busy = false;
        self.text = Some(TextState {
            value: original.clone(),
            check: validate::validate(path, &original),
            original,
            stage: Stage::Edit,
            ack: false,
            hunks: None,
            target: jump,
            far: line
                .filter(|line| *line as usize > total)
                .map(|line| (line, total)),
        });
        self.set_badge(cx);
        // The cursor goes there, and the editor scrolls to it.
        if let Some(jump) = jump {
            cx.cursor_to("editor", jump, 1);
        }
    }

    fn edit_text(&mut self, value: String, cx: &mut Context<Message>) {
        let path = self.path.clone().unwrap_or_default();
        let Some(text) = &mut self.text else {
            return;
        };
        text.value = diff::normalize(&value);
        if js::length(&text.value) <= VALIDATE_WHILE_TYPING_MAX {
            text.check = validate::validate(&path, &text.value);
            text.ack = false;
        }
        self.saved = false;
        self.set_badge(cx);
    }

    fn review(&mut self) {
        let path = self.path.clone().unwrap_or_default();
        if self.screen != Screen::Text {
            return;
        }
        let Some(text) = &mut self.text else {
            return;
        };
        text.check = validate::validate(&path, &text.value);
        text.ack = false;
        text.hunks = Some(diff::hunks(diff::diff_lines(&text.original, &text.value)));
        text.stage = Stage::Review;
        self.problem = None;
        self.diff_range = (0, DIFF_WINDOW);
    }

    fn after_text_save(&mut self, written: &str, token: u64, cx: &mut Context<Message>) {
        let path = self.path.clone().unwrap_or_default();
        if let Some(text) = &mut self.text {
            text.original = diff::normalize(written);
            text.value = text.original.clone();
            text.stage = Stage::Edit;
            text.hunks = None;
            text.ack = false;
        }
        self.exists = true;
        self.saved = true;
        self.send(
            answer::text_saved(&path),
            TellStatus::Sent,
            AfterSend::Saved(token),
            cx,
        );
    }

    /// A broken file goes through only once the person has said they know.
    fn save_text(&mut self, cx: &mut Context<Message>) {
        if self.screen != Screen::Text || self.busy || self.conflict.is_some() {
            return;
        }
        let path = self.path.clone().unwrap_or_default();
        let exists = self.exists;
        let Some(text) = &mut self.text else {
            return;
        };
        text.check = validate::validate(&path, &text.value);
        if !text.check.ok && !text.ack {
            return;
        }
        if text.value == text.original && exists {
            return;
        }
        let written = diff::restore_eols(self.last_read.as_deref().unwrap_or(""), &text.value);
        self.persist(written, SaveKind::Text, cx);
    }

    /* ── writing, and what happens when the file moved ────────────── */

    /// `expected` cannot say "still absent", so a new file is looked for
    /// again just before.
    fn persist(&mut self, text: String, kind: SaveKind, cx: &mut Context<Message>) {
        let token = self.generation;
        self.busy = true;
        self.problem = None;
        self.saved = false;
        self.told = None;
        let path = self.path.clone().unwrap_or_default();
        self.saving = Some(Saving {
            token,
            text: text.clone(),
            kind,
            expected: self.last_read.is_some(),
        });
        match self.last_read.clone() {
            None => cx.call(api::FilesRead { path }, move |answer| {
                Message::SaveChecked { token, answer }
            }),
            Some(expected) => cx.call(
                api::FilesWrite {
                    path,
                    text,
                    expected: Some(expected),
                },
                move |answer| Message::SaveWritten { token, answer },
            ),
        }
    }

    fn saving(&self, token: u64) -> Option<&Saving> {
        self.saving.as_ref().filter(|saving| saving.token == token)
    }

    fn saved_as(&mut self, outcome: Outcome, cx: &mut Context<Message>) {
        let Some(saving) = self.saving.take() else {
            return;
        };
        if saving.token != self.generation {
            return;
        }
        match outcome {
            Outcome::Refused(error) => {
                self.busy = false;
                let code = if ["denied", "rate_limited", "invalid"].contains(&error.code.as_str()) {
                    error.code
                } else {
                    "other".to_owned()
                };
                self.problem = Some(Problem {
                    code,
                    detail: error.message,
                });
            }
            Outcome::Conflict(theirs) => {
                self.busy = false;
                self.conflict = Some(if self.screen == Screen::Dotenv {
                    self.form_conflict(theirs)
                } else {
                    self.text_conflict(theirs)
                });
                self.diff_range = (0, DIFF_WINDOW);
            }
            Outcome::Written => {
                self.last_read = Some(saving.text.clone());
                match saving.kind {
                    SaveKind::Form => self.after_form_save(&saving.text, saving.token, cx),
                    SaveKind::Text => self.after_text_save(&saving.text, saving.token, cx),
                }
            }
        }
    }

    fn text_conflict(&self, theirs: Option<String>) -> Conflict {
        let mine = self
            .text
            .as_ref()
            .map(|text| text.value.as_str())
            .unwrap_or("");
        let disk = diff::normalize(theirs.as_deref().unwrap_or(""));
        Conflict {
            kind: ConflictKind::Text {
                hunks: diff::hunks(diff::diff_lines(&disk, mine)),
            },
            theirs,
        }
    }

    fn form_conflict(&self, theirs: Option<String>) -> Conflict {
        let edits = self.form_edits();
        let their_doc = dotenv::parse(theirs.as_deref().unwrap_or(""));
        let disk: HashMap<String, String> = dotenv::rows(&their_doc)
            .into_iter()
            .map(|row| (row.key, row.value))
            .collect();
        let rows = edits
            .iter()
            .filter(|(key, mine)| disk.get(key) != Some(mine))
            .map(|(key, mine)| ConflictRow {
                key: key.clone(),
                mine: mine.clone(),
                theirs: disk.get(key).cloned(),
                choice: Side::Mine,
                revealed: false,
            })
            .collect();
        let was: Vec<dotenv::Row> = dotenv::rows(&self.doc);
        let was_map: HashMap<&str, &str> = was
            .iter()
            .map(|row| (row.key.as_str(), row.value.as_str()))
            .collect();
        let edited: HashSet<&str> = edits.iter().map(|(key, _)| key.as_str()).collect();
        let mut keys: Vec<&str> = was.iter().map(|row| row.key.as_str()).collect();
        let their_rows = dotenv::rows(&their_doc);
        keys.extend(their_rows.iter().map(|row| row.key.as_str()));
        let mut seen = HashSet::new();
        let changed_on_disk = keys
            .into_iter()
            .filter(|key| seen.insert(*key))
            .filter(|key| {
                !edited.contains(key)
                    && was_map.get(key).copied() != disk.get(*key).map(String::as_str)
            })
            .count();
        Conflict {
            kind: ConflictKind::Dotenv {
                rows,
                edits,
                changed_on_disk,
            },
            theirs,
        }
    }

    fn conflict_row(&mut self, key: &str) -> Option<&mut ConflictRow> {
        match &mut self.conflict {
            Some(Conflict {
                kind: ConflictKind::Dotenv { rows, .. },
                ..
            }) => rows.iter_mut().find(|row| row.key == key),
            _ => None,
        }
    }

    /// Mine wins, on top of what the disk holds now. Only ever on the
    /// person's click.
    fn conflict_keep_mine(&mut self, cx: &mut Context<Message>) {
        if self.busy {
            return;
        }
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        self.last_read = conflict.theirs.clone();
        let theirs = conflict.theirs.as_deref().unwrap_or("");
        match conflict.kind {
            ConflictKind::Text { .. } => {
                let Some(text) = &mut self.text else {
                    return;
                };
                text.original = diff::normalize(theirs);
                let written = diff::restore_eols(theirs, &text.value);
                self.persist(written, SaveKind::Text, cx);
            }
            ConflictKind::Dotenv { rows, edits, .. } => {
                let dropped: HashSet<&str> = rows
                    .iter()
                    .filter(|row| row.choice == Side::Theirs)
                    .map(|row| row.key.as_str())
                    .collect();
                let edits: Edits = edits
                    .into_iter()
                    .filter(|(key, _)| !dropped.contains(key.as_str()))
                    .collect();
                self.doc = dotenv::parse(theirs);
                let written = dotenv::serialize(&dotenv::with_values(&self.doc, &edits));
                self.persist(written, SaveKind::Form, cx);
            }
        }
    }

    /// What is on disk wins, and what the person typed is dropped: only on
    /// their click too.
    fn conflict_take_theirs(&mut self, cx: &mut Context<Message>) {
        if self.busy {
            return;
        }
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        self.last_read = conflict.theirs.clone();
        self.exists = conflict.theirs.is_some();
        self.saved = false;
        self.told = None;
        self.problem = None;
        let path = self.path.clone().unwrap_or_default();
        self.load_into(path, conflict.theirs, None, cx);
    }

    /// Back to the text with mine, measured against what the disk holds now.
    fn conflict_back(&mut self) {
        if !matches!(
            &self.conflict,
            Some(Conflict {
                kind: ConflictKind::Text { .. },
                ..
            })
        ) {
            return;
        }
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        self.exists = conflict.theirs.is_some();
        if let Some(text) = &mut self.text {
            text.original = diff::normalize(conflict.theirs.as_deref().unwrap_or(""));
            text.stage = Stage::Edit;
        }
        self.last_read = conflict.theirs;
    }

    /// The person looked and has nothing to change: the waiting agent still
    /// hears of it.
    fn tell_no_change(&mut self, cx: &mut Context<Message>) {
        if self.caller.is_none() || self.busy {
            return;
        }
        let path = self.path.clone().unwrap_or_default();
        let message = match self.screen {
            Screen::Dotenv => {
                if !self.form_edits().is_empty() {
                    return;
                }
                let facts = self.dotenv_facts(&self.doc, &self.doc);
                answer::dotenv_checked(
                    &answer::Facts {
                        path: &path,
                        known: &facts.known,
                        values: &facts.values,
                    },
                    &facts.already_set,
                    &facts.still_empty,
                )
            }
            Screen::Text => {
                match &self.text {
                    Some(text) if text.value == text.original => {}
                    _ => return,
                }
                answer::text_checked(&path)
            }
            _ => return,
        };
        self.busy = true;
        self.send(
            message,
            TellStatus::Checked,
            AfterSend::Done(self.generation),
            cx,
        );
    }

    /* ── opening ──────────────────────────────────────────────────── */

    fn load_into(
        &mut self,
        path: String,
        text: Option<String>,
        line: Option<u32>,
        cx: &mut Context<Message>,
    ) {
        if dotenv::is_dotenv_path(&path) {
            // Its template and git are asked first: nothing to edit meanwhile.
            self.screen = Screen::Loading;
            self.start_opening(path, text, cx);
        } else {
            self.open_text(&path, text.as_deref(), line, cx);
        }
    }

    fn open(&mut self, path: String, line: Option<u32>, cx: &mut Context<Message>) {
        let token = self.reset();
        self.path = Some(path.clone());
        self.set_title(logic::tab_title(&path), cx);
        cx.call(api::FilesRead { path }, move |answer| Message::Opened {
            token,
            line,
            answer,
        });
    }

    fn show_picker(&mut self, cx: &mut Context<Message>) {
        let token = self.reset();
        self.path = None;
        self.from_picker = false;
        let title = self.t.t("tab");
        self.set_title(title, cx);
        self.set_badge(cx);
        cx.call(api::FilesList::default(), move |answer| Message::Listed {
            token,
            answer,
        });
    }

    fn pick(&mut self, path: &str, cx: &mut Context<Message>) {
        let Some(checked) = logic::check_path(path) else {
            return;
        };
        self.from_picker = true;
        self.open(checked, None, cx);
    }

    pub fn is_dirty(&self) -> bool {
        match self.screen {
            Screen::Dotenv if self.form.is_some() => !self.form_edits().is_empty(),
            Screen::Text => self
                .text
                .as_ref()
                .is_some_and(|text| text.value != text.original),
            _ => false,
        }
    }

    /// Asked for again: the latest to ask is the one to answer, and what was
    /// typed stays.
    fn handle_params(&mut self, read: Params, cx: &mut Context<Message>) {
        self.caller = read.caller;
        self.unsent = None;
        let editing = matches!(self.screen, Screen::Dotenv | Screen::Text) && self.path.is_some();
        if editing && (read.path.is_none() || read.path == self.path) {
            self.why = read.why;
            if self.screen == Screen::Dotenv && !read.keys.is_empty() {
                let fresh: Vec<String> = read
                    .keys
                    .into_iter()
                    .filter(|key| !self.asked.contains(key))
                    .collect();
                self.asked.extend(fresh.iter().cloned());
                if let Some(form) = &mut self.form {
                    for key in fresh {
                        match form.row_mut(&key) {
                            Some(row) => row.asked = true,
                            None => {
                                form.offered.retain(|offer| offer.key != key);
                                form.rows.push(KeyRow::fresh(
                                    key,
                                    true,
                                    String::new(),
                                    false,
                                    false,
                                ));
                            }
                        }
                    }
                    form.rows.sort_by_key(|row| !row.asked);
                }
            }
            self.told = None;
            self.saved = false;
            self.set_badge(cx);
            return;
        }
        if read.bad_path {
            self.path = None;
            self.show_error(None, Some("bad-path"), cx);
            return;
        }
        self.why = read.why;
        self.asked = read.keys;
        self.from_picker = false;
        match read.path {
            Some(path) => self.open(path, read.line, cx),
            None => self.show_picker(cx),
        }
    }

    fn go_back(&mut self, cx: &mut Context<Message>) {
        if !self.is_dirty() {
            return self.show_picker(cx);
        }
        cx.confirm(
            api::Confirm {
                message: self.t.t("leaveMessage"),
                confirm: Some(self.t.t("leaveConfirm")),
                cancel: Some(self.t.t("leaveCancel")),
                danger: true,
            },
            Message::Leave,
        );
    }

    /// Whether Ctrl+S means something now.
    pub fn save_key(&self) -> bool {
        self.conflict.is_none()
            && match self.screen {
                Screen::Dotenv => true,
                Screen::Text => self
                    .text
                    .as_ref()
                    .is_some_and(|text| text.stage == Stage::Edit),
                _ => false,
            }
    }
}

fn read_or_none(answer: Result<String, KitError>) -> Result<Option<String>, KitError> {
    match answer {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.is(KitError::NOT_FOUND) => Ok(None),
        Err(error) => Err(error),
    }
}

impl Kit for Editor {
    type Message = Message;

    fn init(start: &Start, cx: &mut Context<Message>) -> Self {
        let mut editor = Editor {
            t: logic::strings(&start.theme.locale),
            screen: Screen::Loading,
            path: None,
            caller: None,
            why: None,
            asked: Vec::new(),
            from_picker: false,
            exists: false,
            busy: false,
            problem: None,
            saved: false,
            told: None,
            error: None,
            picker: None,
            form: None,
            ignore: None,
            text: None,
            conflict: None,
            diff_range: (0, DIFF_WINDOW),
            generation: 0,
            last_title: None,
            last_badge: None,
            doc: dotenv::parse(""),
            last_read: None,
            example_rows: Vec::new(),
            example_name: None,
            example_values: Vec::new(),
            unsent: None,
            opening: None,
            adding: None,
            saving: None,
        };
        // The page asked `tab.params`; a native tab is started with them.
        let caller = start.tab.caller.as_ref().map(|caller| caller.pane.as_str());
        editor.handle_params(logic::read_params(&start.tab.params, caller), cx);
        editor
    }

    fn update(&mut self, message: Message, cx: &mut Context<Message>) {
        match message {
            Message::Params(answer) => {
                let caller = answer.caller.as_ref().map(|caller| caller.pane.as_str());
                let read = logic::read_params(&answer.params, caller);
                self.handle_params(read, cx);
            }
            Message::Theme(theme) => self.t.set_locale(theme.locale),
            Message::Opened {
                token,
                line,
                answer,
            } => {
                if token != self.generation {
                    return;
                }
                let text = match read_or_none(answer) {
                    Ok(text) => text,
                    Err(error) => return self.show_error(Some(&error), None, cx),
                };
                self.last_read = text.clone();
                self.exists = text.is_some();
                let path = self.path.clone().unwrap_or_default();
                self.load_into(path, text, line, cx);
            }
            Message::ExampleRead { token, answer } => {
                let Some(opening) = self
                    .opening
                    .as_mut()
                    .filter(|opening| opening.token == token)
                else {
                    return;
                };
                let name = opening.reading.take().unwrap_or_default();
                match read_or_none(answer) {
                    Ok(Some(text)) => {
                        opening.example = Some((name, dotenv::parse(&text)));
                        opening.candidates.clear();
                    }
                    Ok(None) => {}
                    // No template to offer: the form works without.
                    Err(_) => opening.candidates.clear(),
                }
                self.step_opening(cx);
            }
            Message::IgnoreRead { token, answer } => {
                let Some(opening) = self
                    .opening
                    .as_mut()
                    .filter(|opening| opening.token == token)
                else {
                    return;
                };
                let Some(scan) = opening.scan.as_mut() else {
                    return;
                };
                match scan.take(answer) {
                    Some(status) => self.finish_opening(status, cx),
                    None => self.step_opening(cx),
                }
            }
            Message::Listed { token, answer } => {
                if token != self.generation {
                    return;
                }
                match answer {
                    Err(error) => self.show_error(Some(&error), None, cx),
                    Ok(listing) => {
                        self.picker = Some(Picker {
                            groups: logic::rank_files(
                                listing.files.iter().map(|file| file.path.as_str()),
                            ),
                            filter: String::new(),
                            truncated: listing.truncated,
                        });
                        self.screen = Screen::Picker;
                    }
                }
            }
            Message::SaveChecked { token, answer } => {
                let Some(saving) = self.saving(token).cloned() else {
                    return;
                };
                match read_or_none(answer) {
                    Ok(Some(now)) => self.saved_as(Outcome::Conflict(Some(now)), cx),
                    Ok(None) => {
                        let path = self.path.clone().unwrap_or_default();
                        cx.call(
                            api::FilesWrite {
                                path,
                                text: saving.text,
                                expected: None,
                            },
                            move |answer| Message::SaveWritten { token, answer },
                        );
                    }
                    Err(error) => self.saved_as(Outcome::Refused(error), cx),
                }
            }
            Message::SaveWritten { token, answer } => {
                let Some(saving) = self.saving(token).cloned() else {
                    return;
                };
                match answer {
                    Ok(_) => self.saved_as(Outcome::Written, cx),
                    Err(error) if error.is(KitError::CONFLICT) && saving.expected => {
                        let path = self.path.clone().unwrap_or_default();
                        cx.call(api::FilesRead { path }, move |answer| Message::SaveTheirs {
                            token,
                            answer,
                        });
                    }
                    Err(error) => self.saved_as(Outcome::Refused(error), cx),
                }
            }
            Message::SaveTheirs { token, answer } => {
                if self.saving(token).is_none() {
                    return;
                }
                match read_or_none(answer) {
                    Ok(theirs) => self.saved_as(Outcome::Conflict(theirs), cx),
                    Err(error) => self.saved_as(Outcome::Refused(error), cx),
                }
            }
            Message::Sent {
                token,
                pane,
                status,
                after,
                answer,
            } => {
                if token == self.generation {
                    match answer {
                        Ok(_) => {
                            self.told = Some(Told {
                                status,
                                pane,
                                code: None,
                            });
                            self.unsent = None;
                        }
                        Err(error) => {
                            let code = if error.code.is_empty() {
                                "other".to_owned()
                            } else {
                                error.code
                            };
                            self.told = Some(Told {
                                status: TellStatus::Failed,
                                pane,
                                code: Some(code),
                            });
                        }
                    }
                }
                self.after_send(after, cx);
            }
            Message::GitignoreRead { token, answer } => {
                let Some(adding) = self.adding.as_ref().filter(|adding| adding.token == token)
                else {
                    return;
                };
                let current = match read_or_none(answer) {
                    Ok(current) => current,
                    Err(error) => return self.adding_failed(&error.code),
                };
                let text = gitignore::with_pattern(
                    current.as_deref().unwrap_or(""),
                    &gitignore::pattern_for(&adding.path),
                );
                cx.call(
                    api::FilesWrite {
                        path: ".gitignore".into(),
                        text,
                        expected: current,
                    },
                    move |answer| Message::GitignoreWritten { token, answer },
                );
            }
            Message::GitignoreWritten { token, answer } => {
                if !self
                    .adding
                    .as_ref()
                    .is_some_and(|adding| adding.token == token)
                {
                    return;
                }
                match answer {
                    Ok(_) => self.step_adding(cx),
                    Err(error) => self.adding_failed(&error.code),
                }
            }
            Message::AddScanRead { token, answer } => {
                let Some(adding) = self.adding.as_mut().filter(|adding| adding.token == token)
                else {
                    return;
                };
                let Some(scan) = adding.scan.as_mut() else {
                    return;
                };
                match scan.take(answer) {
                    Some(status) => {
                        self.adding = None;
                        self.ignore = Some(Ignore {
                            status,
                            busy: false,
                            problem: None,
                        });
                    }
                    None => self.step_adding(cx),
                }
            }
            Message::Leave(true) => self.show_picker(cx),
            Message::Leave(false) => {}
            Message::Pick(path) => self.pick(&path, cx),
            Message::Filter(filter) => {
                if let Some(picker) = &mut self.picker {
                    picker.filter = filter;
                }
            }
            Message::Back => self.go_back(cx),
            Message::Retry => match self.path.clone() {
                Some(path) => self.open(path, None, cx),
                None => self.show_picker(cx),
            },
            Message::EditRow(key, value) => self.edit_row(&key, value, cx),
            Message::ToggleReveal(key) => self.toggle_reveal(&key),
            Message::AddOffered(key) => self.add_offered(&key),
            Message::AddAllOffered => self.add_all_offered(),
            Message::NewKey(name) => {
                if let Some(form) = &mut self.form {
                    form.new_key = name;
                    form.new_key_problem = None;
                }
            }
            Message::AddNewKey => self.add_new_key(),
            Message::AddToGitignore => self.add_to_gitignore(cx),
            Message::SaveForm => self.save_form(cx),
            Message::EditText(value) => self.edit_text(value, cx),
            Message::Review => self.review(),
            Message::BackToEdit => {
                if let Some(text) = &mut self.text {
                    text.stage = Stage::Edit;
                }
            }
            Message::Ack(on) => {
                if let Some(text) = &mut self.text {
                    text.ack = on;
                }
            }
            Message::SaveText => self.save_text(cx),
            Message::DiffRange(range) => {
                self.diff_range = (range.start, range.end.max(range.start))
            }
            Message::ConflictChoose(key, side) => {
                if let Some(row) = self.conflict_row(&key) {
                    row.choice = side;
                }
            }
            Message::ConflictReveal(key) => {
                if let Some(row) = self.conflict_row(&key) {
                    row.revealed = !row.revealed;
                }
            }
            Message::ConflictKeepMine => self.conflict_keep_mine(cx),
            Message::ConflictTakeTheirs => self.conflict_take_theirs(cx),
            Message::ConflictBack => self.conflict_back(),
            Message::TellNoChange => self.tell_no_change(cx),
            Message::RetryTell => self.retry_tell(cx),
            Message::SaveKey => {
                if !self.save_key() {
                    return;
                }
                match self.screen {
                    Screen::Dotenv => self.save_form(cx),
                    _ => self.review(),
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        crate::view::view(self)
    }

    fn subscriptions(&self) -> Subscriptions<Message> {
        let subscriptions = Subscriptions::none()
            .tab_params(Message::Params)
            .theme(Message::Theme);
        if self.save_key() {
            subscriptions.shortcut("mod+s", Message::SaveKey)
        } else {
            subscriptions
        }
    }
}
