//! Draws the controller's state with Hangar's widgets: view.js, ported.
//! Nothing a file or a caller said is read as markup, and a value shows only
//! in its own field, dotted until the person reveals it.

use hangar_kit_sdk::Handler;
use hangar_kit_sdk::prelude::*;
use hangar_kit_sdk::ui::{Header, ListItem, LogLine};

use crate::diff::{Hunk, Kind};
use crate::editor::{
    ConflictKind, ConflictRow, DIFF_WINDOW, Editor, Form, IgnoreStatus, KeyRow, Message, Screen,
    Side, Stage, TellStatus,
};
use crate::gitignore;
use crate::js;
use crate::logic;
use crate::validate::Check;

/// What a value hidden in the conflict view shows.
pub const MASK: &str = "••••••••";
const VALUE_LINES_MAX: usize = 8;
/// A line of a text area, and its padding, in logical pixels.
const AREA_LINE: f32 = 18.0;
const AREA_PAD: f32 = 16.0;

type El = Element<Message>;

pub fn view(editor: &Editor) -> El {
    match editor.screen {
        Screen::Loading => page![loading(editor.t().t("loading"))].into(),
        Screen::Picker => picker_view(editor),
        Screen::Error => error_view(editor),
        _ if editor.conflict.is_some() => conflict_view(editor),
        Screen::Dotenv => dotenv_view(editor),
        Screen::Text => match editor.text.as_ref().map(|text| text.stage) {
            Some(Stage::Review) => text_review_view(editor),
            _ => text_edit_view(editor),
        },
    }
}

/* ── the parts every file shows ───────────────────────────────────── */

/// A page that does not scroll as a whole: its header, the part that
/// scrolls, and the foot with the one thing to do.
fn frame(header: impl Into<El>, body: Vec<El>, foot: Option<El>) -> El {
    let mut parts: Vec<El> = vec![
        container(header)
            .padding([0.0, 12.0])
            .width(Length::Fill)
            .into(),
        scroll(column(body).spacing(12.0).padding(12.0)).into(),
    ];
    parts.extend(foot);
    page(parts).fill().into()
}

fn header_of(editor: &Editor, states: Vec<El>) -> Header<Handler<Message>> {
    let t = editor.t();
    let path = editor.path.as_deref().unwrap_or("");
    let mut head = header(js::base_name(path));
    for state in states {
        head = head.state(state);
    }
    let dir = js::dir_name(path);
    if !dir.is_empty() {
        head = head.state(text(dir).mono().small().dim());
    }
    if editor.from_picker {
        head = head.action(button(t.t("back")).ghost().small().on_press(Message::Back));
    }
    head
}

fn request_banner(editor: &Editor) -> Option<El> {
    let why = editor.why.as_ref()?;
    let t = editor.t();
    let asks = match &editor.caller {
        Some(pane) => t.tv("askedBy", &[("pane", pane)]),
        None => t.t("askedAnon"),
    };
    Some(banner(Notice::Info, why.clone()).title(asks).into())
}

fn save_problem(editor: &Editor) -> Option<El> {
    let problem = editor.problem.as_ref()?;
    Some(
        banner(
            Notice::Error,
            editor.t().t(&format!("write.{}", problem.code)),
        )
        .into(),
    )
}

fn tell_problem(editor: &Editor) -> Option<El> {
    let told = editor
        .told
        .as_ref()
        .filter(|told| told.status == TellStatus::Failed)?;
    let t = editor.t();
    let key = format!("tell.{}", told.code.as_deref().unwrap_or("other"));
    let reason = if logic::has(&key) {
        t.t(&key)
    } else {
        t.t("tell.other")
    };
    Some(
        banner(
            Notice::Warning,
            t.tv("notTold", &[("pane", &told.pane), ("reason", &reason)]),
        )
        .action(
            button(t.tv("tellAgain", &[("pane", &told.pane)]))
                .small()
                .on_press_maybe((!editor.busy).then_some(Message::RetryTell)),
        )
        .into(),
    )
}

/// What is true right now, and the one thing to do: a line of its own at the
/// foot of the page.
fn foot(status: String, buttons: Vec<El>) -> El {
    let mut line: Vec<El> = vec![container(text(status)).width(Length::Fill).into()];
    line.extend(buttons);
    bar([row(line).spacing(6.0).align(Align::Center).into()])
        .foot()
        .into()
}

fn saved_status(editor: &Editor) -> String {
    let t = editor.t();
    if editor.busy {
        return t.t("saving");
    }
    let saved = if editor.saved {
        t.t("savedLocal")
    } else {
        String::new()
    };
    match &editor.told {
        None => saved,
        Some(told) => match told.status {
            TellStatus::Sent if editor.screen == Screen::Text => {
                t.tv("toldText", &[("pane", &told.pane)])
            }
            TellStatus::Sent => t.tv("told", &[("pane", &told.pane)]),
            TellStatus::Checked => t.tv("toldNoChange", &[("pane", &told.pane)]),
            TellStatus::Failed => saved,
        },
    }
}

fn can_tell_no_change(editor: &Editor) -> bool {
    editor.caller.is_some()
        && !editor.is_dirty()
        && !editor.saved
        && !editor
            .told
            .as_ref()
            .is_some_and(|told| told.status != TellStatus::Failed)
}

fn tell_button(editor: &Editor, pane: &str) -> ui::Button<Handler<Message>> {
    button(editor.t().tv("tellNoChange", &[("pane", pane)]))
        .small()
        .on_press_maybe((!editor.busy).then_some(Message::TellNoChange))
}

use hangar_kit_sdk::ui;

/* ── the form ─────────────────────────────────────────────────────── */

fn value_input(editor: &Editor, row: &KeyRow) -> El {
    let t = editor.t();
    let id = format!("value:{}", row.key);
    let key = row.key.clone();
    let edit = move |value| Message::EditRow(key.clone(), value);
    let lines = row.value.split('\n').count();
    if lines > 1 && !row.revealed {
        // A one-line field would drop the line breaks: there is nothing to
        // edit until it shows.
        return text_input(t.t("valueLines"), "")
            .secure()
            .mono()
            .id(id)
            .into();
    }
    if lines > 1 {
        let shown = lines.min(VALUE_LINES_MAX) as f32;
        return text_area(id, row.value.clone())
            .mono()
            .placeholder(t.t("valueEmpty"))
            .height(shown * AREA_LINE + AREA_PAD)
            .on_input(edit)
            .into();
    }
    let input = text_input(t.t("valueEmpty"), row.value.clone())
        .mono()
        .id(id)
        .on_input(edit);
    if row.revealed { input } else { input.secure() }.into()
}

fn row_mark(editor: &Editor, row: &KeyRow) -> Option<El> {
    let t = editor.t();
    if row.missing() {
        return Some(state_label(Status::Waiting, t.t("keyMissing")).into());
    }
    if row.changed() {
        return Some(chip(t.t("keyChanged")).into());
    }
    row.asked
        .then(|| state_label(Status::Success, t.t("keySet")).into())
}

fn reveal_button(editor: &Editor, revealed: bool, message: Message) -> El {
    let t = editor.t();
    button(if revealed { t.t("hide") } else { t.t("show") })
        .ghost()
        .small()
        .on_press(message)
        .into()
}

fn key_row(editor: &Editor, row: &KeyRow) -> ListItem<Handler<Message>> {
    let t = editor.t();
    let mut entry = item(row.key.clone()).mono().wrap();
    if let Some(mark) = row_mark(editor, row) {
        entry = entry.end(mark);
    }
    entry = entry.body(
        row![
            value_input(editor, row),
            reveal_button(editor, row.revealed, Message::ToggleReveal(row.key.clone())),
        ]
        .spacing(4.0)
        .align(Align::Start),
    );
    if !row.hint.is_empty() {
        entry = entry.body(text(row.hint.clone()).small().dim());
    }
    if row.count > 1 {
        entry = entry.body(text(t.tn("keyTwice", row.count as i64, &[])).small().dim());
    }
    entry
}

fn ignore_banners(editor: &Editor) -> Vec<El> {
    let Some(ignore) = editor
        .ignore
        .as_ref()
        .filter(|ignore| ignore.status != IgnoreStatus::Ignored)
    else {
        return Vec::new();
    };
    let t = editor.t();
    let path = editor.path.as_deref().unwrap_or("");
    let file = js::base_name(path);
    if ignore.status == IgnoreStatus::Unknown {
        return vec![banner(Notice::Info, t.tv("ignoreUnknown", &[("file", file)])).into()];
    }
    let words = if ignore.status == IgnoreStatus::NoFile {
        t.tv("ignoreNoFile", &[("file", file)])
    } else {
        t.tv("ignoreWarning", &[("file", file)])
    };
    let label = if ignore.busy {
        t.t("ignoreAdding")
    } else {
        t.tv("ignoreAdd", &[("pattern", &gitignore::pattern_for(path))])
    };
    let add = button(label)
        .small()
        .on_press_maybe((!ignore.busy).then_some(Message::AddToGitignore));
    let mut out: Vec<El> = vec![banner(Notice::Warning, words).action(add).into()];
    if let Some(problem) = &ignore.problem {
        let key = format!("write.{problem}");
        let reason = if logic::has(&key) {
            t.t(&key)
        } else {
            t.t("write.other")
        };
        out.push(banner(Notice::Error, t.tv("ignoreFailed", &[("reason", &reason)])).into());
    }
    out
}

fn offered_section(editor: &Editor, form: &Form) -> Option<El> {
    if form.offered.is_empty() {
        return None;
    }
    let t = editor.t();
    let items = form.offered.iter().map(|offer| {
        let mut entry = item(offer.key.clone()).mono().wrap().action(
            button(t.t("add"))
                .small()
                .on_press(Message::AddOffered(offer.key.clone())),
        );
        if !offer.hint.is_empty() {
            entry = entry.sub(offer.hint.clone());
        }
        entry
    });
    let file = form.example_name.as_deref().unwrap_or("");
    let mut part = section(
        t.tv("offeredTitle", &[("file", file)]),
        [list(items).group().into()],
    )
    .count(form.offered.len().to_string());
    if form.offered.len() > 1 {
        part = part.action(
            button(t.t("addAll"))
                .ghost()
                .small()
                .on_press(Message::AddAllOffered),
        );
    }
    Some(part.into())
}

fn new_key_field(editor: &Editor, form: &Form) -> El {
    let t = editor.t();
    let mut input = text_input(t.t("newKeyPlaceholder"), form.new_key.clone())
        .mono()
        .id("new-key")
        .on_input(Message::NewKey)
        .on_submit(Message::AddNewKey);
    if let Some(problem) = form.new_key_problem {
        input = input.error(t.t(problem));
    }
    row![
        input,
        button(t.t("add")).small().on_press(Message::AddNewKey)
    ]
    .spacing(4.0)
    .align(Align::Start)
    .into()
}

fn dotenv_view(editor: &Editor) -> El {
    let t = editor.t();
    let Some(form) = &editor.form else {
        return page![loading(t.t("loading"))].into();
    };
    let missing = editor.missing_count();
    let state: Vec<El> = if missing > 0 {
        vec![state_label(Status::Waiting, t.tn("stateMissing", missing as i64, &[])).into()]
    } else if editor.saved {
        vec![state_label(Status::Success, t.t("stateSaved")).into()]
    } else if !editor.exists {
        vec![state_label(Status::Neutral, t.t("stateNew")).into()]
    } else {
        Vec::new()
    };
    let head = header_of(editor, state);

    let mut body: Vec<El> = Vec::new();
    body.extend(request_banner(editor));
    body.extend(ignore_banners(editor));
    body.extend(save_problem(editor));
    body.extend(tell_problem(editor));
    if !editor.exists && editor.why.is_none() {
        body.push(banner(Notice::Info, t.t("newFile")).into());
    }
    let asked: Vec<&KeyRow> = form.rows.iter().filter(|row| row.asked).collect();
    let rest: Vec<&KeyRow> = form.rows.iter().filter(|row| !row.asked).collect();
    for (title, rows) in [("sectionAsked", &asked), ("sectionFile", &rest)] {
        if rows.is_empty() {
            continue;
        }
        let items = rows.iter().map(|row| key_row(editor, row));
        body.push(
            section(t.t(title), [list(items).group().into()])
                .count(rows.len().to_string())
                .into(),
        );
    }
    if form.rows.is_empty() {
        body.push(empty(t.t("noKeys")).into());
    }
    body.extend(offered_section(editor, form));
    body.push(section(t.t("newKeyLabel"), [new_key_field(editor, form)]).into());
    if form.other_lines > 0 {
        body.push(
            text(t.tn("otherLines", form.other_lines as i64, &[]))
                .small()
                .muted()
                .into(),
        );
    }

    // With keys still to fill in, saying there is nothing to change is the
    // lesser choice.
    let mut buttons: Vec<El> = Vec::new();
    let offer_tell = can_tell_no_change(editor);
    if offer_tell && let Some(pane) = &editor.caller {
        let tell = tell_button(editor, pane);
        buttons.push(if missing > 0 { tell } else { tell.primary() }.into());
    }
    if !offer_tell || missing > 0 {
        let disabled = editor.busy || !editor.is_dirty();
        let mut save = button(t.t("save"))
            .primary()
            .small()
            .on_press_maybe((!disabled).then_some(Message::SaveForm));
        if disabled && !editor.busy {
            save = save.tooltip(t.t("nothingToSave"));
        }
        buttons.push(save.into());
    }
    frame(head, body, Some(foot(saved_status(editor), buttons)))
}

/* ── diffs ────────────────────────────────────────────────────────── */

fn diff_line(editor: &Editor, hunk: &Hunk) -> LogLine {
    match hunk {
        Hunk::Skip(count) => {
            log_line(editor.t().tn("diffSkip", *count as i64, &[])).kind(LineKind::Head)
        }
        Hunk::Line(op) => {
            let (sign, kind, number) = match op.kind {
                Kind::Add => ("+ ", LineKind::Added, op.b),
                Kind::Del => ("− ", LineKind::Removed, op.a),
                Kind::Same => ("  ", LineKind::Normal, op.a),
            };
            let line = log_line(format!("{sign}{}", op.text)).kind(kind);
            match number {
                Some(number) => line.number(number as u32),
                None => line,
            }
        }
    }
}

/// Signs as well as colours. Given in parts: a diff of a large file would
/// not fit in one view.
fn diff_block(editor: &Editor, hunks: &[Hunk]) -> El {
    let total = hunks.len() as u32;
    let (start, end) = editor.diff_range;
    let start = start.min(total);
    let end = end.max(start + DIFF_WINDOW).min(total);
    let lines = hunks[start as usize..end as usize]
        .iter()
        .map(|hunk| diff_line(editor, hunk));
    log_lines(lines)
        .id("diff")
        .window(start, total)
        .on_range(Message::DiffRange)
        .height(480.0)
        .into()
}

/* ── the conflict ─────────────────────────────────────────────────── */

fn conflict_view(editor: &Editor) -> El {
    let t = editor.t();
    let Some(conflict) = &editor.conflict else {
        return page![loading(t.t("loading"))].into();
    };
    let head = header_of(
        editor,
        vec![state_label(Status::Failure, t.t("stateConflict")).into()],
    );
    let intro = if conflict.gone() {
        t.t("conflictGone")
    } else {
        format!("{}. {}", t.t("conflictTitle"), t.t("conflictHint"))
    };
    let mut body: Vec<El> = vec![banner(Notice::Error, intro).into()];
    match &conflict.kind {
        ConflictKind::Text { hunks } => {
            body.push(text(t.t("conflictDiff")).mono().small().dim().into());
            body.push(diff_block(editor, hunks));
            let mine = if conflict.gone() {
                t.t("conflictWriteMine")
            } else {
                t.t("conflictKeepMine")
            };
            let buttons = vec![
                button(mine)
                    .small()
                    .on_press(Message::ConflictKeepMine)
                    .into(),
                button(t.t("conflictTakeTheirs"))
                    .small()
                    .on_press(Message::ConflictTakeTheirs)
                    .into(),
                button(t.t("conflictEdit"))
                    .ghost()
                    .small()
                    .on_press(Message::ConflictBack)
                    .into(),
            ];
            frame(head, body, Some(foot(String::new(), buttons)))
        }
        ConflictKind::Dotenv {
            rows,
            changed_on_disk,
            ..
        } => {
            if rows.is_empty() {
                body.push(banner(Notice::Info, t.t("conflictNone")).into());
            } else {
                let items = rows.iter().map(|row| conflict_row(editor, row));
                body.push(
                    section(t.t("conflictRows"), [list(items).group().into()])
                        .count(rows.len().to_string())
                        .into(),
                );
            }
            if *changed_on_disk > 0 {
                body.push(
                    text(t.tn("conflictTheirsChanged", *changed_on_disk as i64, &[]))
                        .small()
                        .dim()
                        .into(),
                );
            }
            let buttons = vec![
                button(t.t("conflictApply"))
                    .primary()
                    .small()
                    .on_press(Message::ConflictKeepMine)
                    .into(),
                button(t.t("conflictTakeTheirs"))
                    .small()
                    .on_press(Message::ConflictTakeTheirs)
                    .into(),
            ];
            frame(head, body, Some(foot(String::new(), buttons)))
        }
    }
}

/// One side of a key that differs: absent, empty, or dotted until shown.
fn side_label(editor: &Editor, row: &ConflictRow, side: Side) -> String {
    let t = editor.t();
    let (name, value) = match side {
        Side::Mine => (t.t("mine"), Some(&row.mine)),
        Side::Theirs => (t.t("theirs"), row.theirs.as_ref()),
    };
    let shown = match value {
        None => t.t("absent"),
        Some(value) if value.is_empty() => t.t("valueEmpty"),
        Some(value) if row.revealed => value.clone(),
        Some(_) => MASK.to_owned(),
    };
    format!("{name}  {shown}")
}

fn conflict_row(editor: &Editor, row: &ConflictRow) -> ListItem<Handler<Message>> {
    let key = row.key.clone();
    let choice = radio(
        [
            select_option("mine", side_label(editor, row, Side::Mine)),
            select_option("theirs", side_label(editor, row, Side::Theirs)),
        ],
        Some(if row.choice == Side::Mine {
            "mine"
        } else {
            "theirs"
        }),
    )
    .on_select(move |value| {
        let side = if value == "theirs" {
            Side::Theirs
        } else {
            Side::Mine
        };
        Message::ConflictChoose(key.clone(), side)
    });
    let choice = if row.revealed {
        choice
    } else {
        choice.horizontal()
    };
    item(row.key.clone())
        .mono()
        .wrap()
        .end(reveal_button(
            editor,
            row.revealed,
            Message::ConflictReveal(row.key.clone()),
        ))
        .body(choice)
}

/* ── the editor ───────────────────────────────────────────────────── */

fn check_word(editor: &Editor, check: &Check) -> Option<El> {
    let t = editor.t();
    let name = check.kind?.name();
    Some(if check.ok {
        if check.partial {
            state_label(Status::Neutral, t.tv("checkBasic", &[("kind", name)])).into()
        } else {
            state_label(Status::Success, t.tv("checkOk", &[("kind", name)])).into()
        }
    } else {
        let words = match check.line {
            Some(line) => t.tv(
                "checkBrokenAt",
                &[("kind", name), ("line", &line.to_string())],
            ),
            None => t.tv("checkBroken", &[("kind", name)]),
        };
        state_label(Status::Failure, words).into()
    })
}

fn text_header(editor: &Editor) -> Header<Handler<Message>> {
    let t = editor.t();
    let check = editor.text.as_ref().map(|text| &text.check);
    let state: Vec<El> = if editor.saved {
        vec![state_label(Status::Success, t.t("stateSaved")).into()]
    } else if let Some(word) = check.and_then(|check| check_word(editor, check)) {
        vec![word]
    } else if !editor.exists {
        vec![state_label(Status::Neutral, t.t("stateNew")).into()]
    } else {
        Vec::new()
    };
    header_of(editor, state)
}

pub fn check_reason(editor: &Editor, check: &Check) -> String {
    let t = editor.t();
    let Some(kind) = check.kind else {
        return String::new();
    };
    let name = kind.name();
    if check.ok {
        return if check.partial {
            t.tv("checkPartial", &[("kind", name)])
        } else {
            String::new()
        };
    }
    let reason = t.t(&format!("check.{}", check.code.unwrap_or("json")));
    match check.line {
        Some(line) => t.tv(
            "checkBadAt",
            &[
                ("kind", name),
                ("line", &line.to_string()),
                ("reason", &reason),
            ],
        ),
        None => t.tv("checkBad", &[("kind", name), ("reason", &reason)]),
    }
}

fn capitalized(words: &str) -> String {
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn text_edit_view(editor: &Editor) -> El {
    let t = editor.t();
    let Some(text_state) = &editor.text else {
        return page![loading(t.t("loading"))].into();
    };
    let check = &text_state.check;
    let mut notes: Vec<El> = Vec::new();
    notes.extend(request_banner(editor));
    notes.extend(save_problem(editor));
    notes.extend(tell_problem(editor));
    if let Some((line, total)) = text_state.far {
        notes.push(
            banner(
                Notice::Info,
                t.tv(
                    "lineJumpFar",
                    &[("line", &line.to_string()), ("total", &total.to_string())],
                ),
            )
            .into(),
        );
    }
    if !editor.exists {
        notes.push(banner(Notice::Info, t.t("newFile")).into());
    }
    if check.kind.is_some() && !check.ok {
        let bare = t.t(&format!("check.{}", check.code.unwrap_or("json")));
        notes.push(text(capitalized(&bare)).small().tone(Tone::Danger).into());
    } else {
        let words = check_reason(editor, check);
        if !words.is_empty() {
            notes.push(text(words).small().dim().into());
        }
    }

    let mut area = text_area("editor", text_state.value.clone())
        .line_numbers()
        .fill()
        .on_input(Message::EditText);
    let broken_line = check.line.filter(|_| !check.ok).map(|line| line as u32);
    if let Some(line) = broken_line {
        area = area.mark(line_mark(line, Notice::Error).label(check_reason(editor, check)));
    }
    if let Some(target) = text_state
        .target
        .filter(|target| Some(*target) != broken_line)
    {
        area = area.mark(line_mark(target, Notice::Info));
    }

    let buttons: Vec<El> = match &editor.caller {
        Some(pane) if can_tell_no_change(editor) => {
            vec![tell_button(editor, pane).primary().into()]
        }
        _ => {
            let disabled = editor.busy || (!editor.is_dirty() && editor.exists);
            let mut go = button(t.t("review"))
                .primary()
                .small()
                .on_press_maybe((!disabled).then_some(Message::Review));
            if disabled && !editor.busy {
                go = go.tooltip(t.t("nothingToSave"));
            }
            vec![go.into()]
        }
    };

    let mut parts: Vec<El> = vec![
        container(text_header(editor))
            .padding([0.0, 12.0])
            .width(Length::Fill)
            .into(),
    ];
    if !notes.is_empty() {
        parts.push(
            container(column(notes).spacing(6.0))
                .padding([8.0, 12.0])
                .width(Length::Fill)
                .into(),
        );
    }
    parts.push(area.into());
    parts.push(foot(saved_status(editor), buttons));
    page(parts).fill().into()
}

fn text_review_view(editor: &Editor) -> El {
    let t = editor.t();
    let Some(text_state) = &editor.text else {
        return page![loading(t.t("loading"))].into();
    };
    let check = &text_state.check;
    let mut body: Vec<El> = Vec::new();
    body.extend(save_problem(editor));
    let broken = check.kind.is_some() && !check.ok;
    if broken {
        body.push(
            banner(Notice::Error, check_reason(editor, check))
                .action(switch(t.t("ackBroken"), text_state.ack).on_toggle(Message::Ack))
                .into(),
        );
    } else if check.kind.is_some() && check.partial {
        body.push(text(check_reason(editor, check)).small().dim().into());
    }
    let hunks = text_state.hunks.as_deref().unwrap_or(&[]);
    let changed = hunks.iter().any(Hunk::is_change);
    let diff: El = if changed {
        diff_block(editor, hunks)
    } else {
        text(t.t("diffNone")).small().dim().into()
    };
    body.push(section(t.t("diffTitle"), [diff]).into());

    let disabled = editor.busy || (broken && !text_state.ack) || (!changed && editor.exists);
    let buttons = vec![
        button(t.t("backToEdit"))
            .ghost()
            .small()
            .on_press(Message::BackToEdit)
            .into(),
        button(t.t("save"))
            .primary()
            .small()
            .on_press_maybe((!disabled).then_some(Message::SaveText))
            .into(),
    ];
    let status = if editor.busy {
        t.t("saving")
    } else {
        String::new()
    };
    frame(text_header(editor), body, Some(foot(status, buttons)))
}

/* ── the picker ───────────────────────────────────────────────────── */

fn picker_view(editor: &Editor) -> El {
    let t = editor.t();
    let Some(picker) = &editor.picker else {
        return page![loading(t.t("loading"))].into();
    };
    let tools = container(
        search(t.t("pickFilter"), picker.filter.clone())
            .id("filter")
            .on_input(Message::Filter)
            .on_clear(Message::Filter(String::new())),
    )
    .padding([8.0, 12.0])
    .width(Length::Fill);

    let mut body: Vec<El> = Vec::new();
    if picker.truncated {
        body.push(banner(Notice::Warning, t.t("pickTruncated")).into());
    }
    let groups: Vec<logic::Shown> = logic::filter_groups(&picker.groups, &picker.filter)
        .into_iter()
        .filter(|group| !group.files.is_empty())
        .collect();
    if groups.is_empty() {
        body.push(empty(t.t("pickEmpty")).into());
    }
    for group in groups {
        let items = group.files.iter().map(|file| {
            let mut entry = item(js::base_name(&file.path))
                .mono()
                .tooltip(file.path.clone())
                .on_press(Message::Pick(file.path.clone()));
            let dir = js::dir_name(&file.path);
            if !dir.is_empty() {
                entry = entry.sub(dir);
            }
            if file.is_new {
                entry = entry.end(chip(t.t("pickNew")));
            }
            entry
        });
        let mut children: Vec<El> = vec![list(items).into()];
        if group.more > 0 {
            children.push(
                text(t.tn("pickMore", group.more as i64, &[]))
                    .small()
                    .dim()
                    .into(),
            );
        }
        body.push(
            section(t.t(group.group.title()), children)
                .count((group.files.len() + group.more).to_string())
                .into(),
        );
    }
    let head = header(t.t("pickTitle"));
    page![
        container(head).padding([0.0, 12.0]).width(Length::Fill),
        tools,
        scroll(column(body).spacing(12.0).padding(12.0)),
    ]
    .fill()
    .into()
}

/* ── the rest ─────────────────────────────────────────────────────── */

fn error_view(editor: &Editor) -> El {
    let t = editor.t();
    let (kind, detail) = match &editor.error {
        Some(error) => (error.kind.as_str(), error.detail.as_str()),
        None => ("error", ""),
    };
    let title = t.t(&format!("err.{kind}.title"));
    let hint = t.t(&format!("err.{kind}.hint"));
    let action = if kind == "bad-path" {
        button(t.t("pickTitle"))
            .primary()
            .small()
            .on_press(Message::Back)
    } else {
        button(t.t("retry"))
            .primary()
            .small()
            .on_press(Message::Retry)
    };
    let state = if kind == "denied" {
        denied(title).text(hint)
    } else {
        let mut state = failed(title).text(hint);
        if !detail.is_empty() {
            state = state.child(text(detail).mono().small());
        }
        state
    };
    page![state.action(action)].into()
}
