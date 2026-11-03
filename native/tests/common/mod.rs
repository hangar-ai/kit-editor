//! Shared by the tests: test/helpers.js, ported. A fake Hangar over files held
//! in memory (no test touches a real file of anyone's) answers every call the
//! tab makes, in order, and keeps them all; the tab is driven as a person
//! would, through its page.
#![allow(dead_code)]

use serde_json::{Value, json};

use hangar_kit_sdk::api::{self, KitError};
use hangar_kit_sdk::testing::{self, Harness, StartExt};
use hangar_kit_sdk::ui::abi::{Call, Operation};
use hangar_kit_sdk::ui::{Element, EventValue, HandlerId, ListItem};

use kit_editor_tab::Editor;
use kit_editor_tab::editor::KeyRow;

pub const CALLER: &str = "Asker";
pub const IGNORED: (&str, &str) = (".gitignore", ".env\n");

/// Files in the order they were first written, as a JavaScript `Map` keeps them.
#[derive(Debug, Clone, Default)]
pub struct Store(pub Vec<(String, String)>);

impl Store {
    pub fn get(&self, path: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(other, _)| other == path)
            .map(|(_, text)| text.as_str())
    }

    pub fn set(&mut self, path: &str, text: &str) {
        match self.0.iter_mut().find(|(other, _)| other == path) {
            Some(slot) => slot.1 = text.to_owned(),
            None => self.0.push((path.to_owned(), text.to_owned())),
        }
    }

    pub fn delete(&mut self, path: &str) {
        self.0.retain(|(other, _)| other != path);
    }

    pub fn has(&self, path: &str) -> bool {
        self.get(path).is_some()
    }
}

pub fn refusal(code: &str, message: &str) -> KitError {
    KitError::new(code, if message.is_empty() { code } else { message })
}

/// A Hangar that answers as docs/kits.md says: `files.read` says not_found
/// for what is absent and drops a BOM, `files.write` with `expected` answers
/// conflict when the file holds anything else or is gone, and `deny` lists
/// the methods answered `denied`.
#[derive(Debug, Clone, Default)]
pub struct Fake {
    pub store: Store,
    pub deny: Vec<String>,
    pub send_error: Option<String>,
    pub write_error: Option<String>,
    pub truncated: bool,
    /// How `ui.confirm` is answered.
    pub confirm: bool,
    /// Reads of these paths are refused: (path, code, message).
    pub read_errors: Vec<(String, String, String)>,
    /// Every call the tab made, answered, in order.
    pub calls: Vec<Call>,
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// The manifest's permissions: what the page held, and all the tab may use.
pub const GRANTED: [&str; 3] = ["files.read", "files.write", "panes.send"];

impl Fake {
    fn answer(&mut self, call: &Call) -> Result<Value, KitError> {
        let method = call.method.as_str();
        // Hangar's own gate: a method there is, under the manifest's permissions.
        let granted: Vec<String> = GRANTED
            .iter()
            .map(|permission| permission.to_string())
            .collect();
        assert!(
            api::needs(method).is_some(),
            "{method} is no method of Hangar's"
        );
        assert!(
            api::may_call(method, &call.params, &granted),
            "{method} needs what the manifest does not ask for"
        );
        if self.deny.iter().any(|denied| denied == method) {
            return Err(refusal(
                "denied",
                &format!("this tab was not granted {method}"),
            ));
        }
        let param = |name: &str| {
            call.params
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        match method {
            "tab.setTitle" | "tab.setBadge" => Ok(json!({})),
            "files.read" => {
                let path = param("path");
                if let Some((_, code, message)) =
                    self.read_errors.iter().find(|(other, _, _)| *other == path)
                {
                    return Err(refusal(code, message));
                }
                match self.store.get(&path) {
                    Some(text) => Ok(json!(strip_bom(text))),
                    None => Err(refusal(
                        "not_found",
                        &format!("{path} was not found in the workspace folder"),
                    )),
                }
            }
            "files.list" => Ok(json!({
                "files": self.store.0.iter().map(|(path, text)| json!({ "path": path, "size": text.len(), "modified": 0 })).collect::<Vec<_>>(),
                "truncated": self.truncated,
            })),
            "files.write" => {
                if let Some(code) = &self.write_error {
                    return Err(refusal(code, ""));
                }
                let path = param("path");
                if let Some(expected) = call.params.get("expected") {
                    let now = self.store.get(&path).map(strip_bom);
                    if now != expected.as_str() {
                        return Err(refusal("conflict", "the file changed"));
                    }
                }
                self.store.set(&path, &param("text"));
                Ok(json!({}))
            }
            "panes.send" => match &self.send_error {
                Some(code) => Err(refusal(code, "")),
                None => Ok(json!({ "queued": true, "pane": param("pane") })),
            },
            "ui.confirm" => Ok(json!(self.confirm)),
            _ => Err(refusal("unknown", &format!("unknown {method}"))),
        }
    }

    pub fn calls_to(&self, method: &str) -> Vec<&Call> {
        self.calls
            .iter()
            .filter(|call| call.method == method)
            .collect()
    }

    pub fn writes(&self) -> Vec<&Call> {
        self.calls_to("files.write")
    }

    pub fn writes_to(&self, path: &str) -> Vec<&Call> {
        self.writes()
            .into_iter()
            .filter(|call| call.params["path"] == path)
            .collect()
    }

    pub fn sent(&self) -> Vec<(String, String)> {
        self.calls_to("panes.send")
            .into_iter()
            .map(|call| {
                (
                    call.params["pane"].as_str().unwrap_or("").to_owned(),
                    call.params["text"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect()
    }

    pub fn last_badge(&self) -> Value {
        self.calls_to("tab.setBadge")
            .last()
            .map(|call| call.params.clone())
            .unwrap_or(Value::Null)
    }
}

/// What a test starts the tab with.
#[derive(Default)]
pub struct Setup {
    pub fake: Fake,
    params: Vec<(String, String)>,
    caller: Option<String>,
    locale: Option<String>,
}

impl Setup {
    pub fn new() -> Self {
        Setup::default()
    }

    pub fn param(mut self, name: &str, value: &str) -> Self {
        self.params.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn path(self, path: &str) -> Self {
        self.param("path", path)
    }

    pub fn caller(mut self, pane: &str) -> Self {
        self.caller = Some(pane.to_owned());
        self
    }

    pub fn file(mut self, path: &str, text: &str) -> Self {
        self.fake.store.set(path, text);
        self
    }

    pub fn files(mut self, files: &[(&str, &str)]) -> Self {
        for (path, text) in files {
            self.fake.store.set(path, text);
        }
        self
    }

    pub fn ignored(self) -> Self {
        self.file(IGNORED.0, IGNORED.1)
    }

    pub fn deny(mut self, method: &str) -> Self {
        self.fake.deny.push(method.to_owned());
        self
    }

    pub fn send_error(mut self, code: &str) -> Self {
        self.fake.send_error = Some(code.to_owned());
        self
    }

    pub fn truncated(mut self) -> Self {
        self.fake.truncated = true;
        self
    }

    pub fn confirm(mut self, answer: bool) -> Self {
        self.fake.confirm = answer;
        self
    }

    pub fn read_error(mut self, path: &str, code: &str, message: &str) -> Self {
        self.fake
            .read_errors
            .push((path.to_owned(), code.to_owned(), message.to_owned()));
        self
    }

    pub fn locale(mut self, locale: &str) -> Self {
        self.locale = Some(locale.to_owned());
        self
    }

    /// The tab started on these params, every call answered.
    pub fn open(self) -> Tab {
        let mut start = testing::start().grant(["files.read", "files.write", "panes.send"]);
        for (name, value) in &self.params {
            start = start.param(name.as_str(), value.as_str());
        }
        if let Some(pane) = &self.caller {
            start = start.caller(pane.as_str());
        }
        if let Some(locale) = &self.locale {
            start = start.locale(locale.as_str());
        }
        let mut tab = Tab {
            harness: Harness::<Editor>::new(start),
            fake: self.fake,
            operations: Vec::new(),
        };
        tab.settle();
        tab
    }
}

pub struct Tab {
    pub harness: Harness<Editor>,
    pub fake: Fake,
    /// What the tab asked done to its page, since it started.
    pub operations: Vec<Operation>,
}

const REVEAL: [&str; 4] = ["Show", "Hide", "Afficher", "Masquer"];

impl Tab {
    pub fn kit(&self) -> &Editor {
        self.harness.kit()
    }

    /// Every call answered, as the page's `settle` waited for them.
    pub fn settle(&mut self) {
        for _ in 0..10_000 {
            let Some(call) = self.harness.calls().first().cloned() else {
                break;
            };
            let answer = self.fake.answer(&call);
            self.fake.calls.push(call.clone());
            self.harness.reply(call.id, answer);
        }
        assert!(
            self.harness.calls().is_empty(),
            "the tab never stops calling"
        );
        self.operations.extend(self.harness.take_operations());
    }

    pub fn store(&self) -> &Store {
        &self.fake.store
    }

    pub fn row(&self, key: &str) -> &KeyRow {
        self.kit()
            .form
            .as_ref()
            .and_then(|form| form.rows.iter().find(|row| row.key == key))
            .unwrap_or_else(|| panic!("no row {key}"))
    }

    pub fn keys(&self) -> Vec<String> {
        self.kit()
            .form
            .as_ref()
            .map(|form| form.rows.iter().map(|row| row.key.clone()).collect())
            .unwrap_or_default()
    }

    pub fn shows(&self, needle: &str) -> bool {
        self.harness.shows(needle)
    }

    pub fn text(&self) -> String {
        self.harness.texts().join("\n")
    }

    pub fn press(&mut self, label: &str) {
        self.harness.press(label);
        self.settle();
    }

    pub fn type_into(&mut self, name: &str, text: &str) {
        self.harness.type_into(name, text);
        self.settle();
    }

    pub fn toggle(&mut self, label: &str, on: bool) {
        self.harness.toggle(label, on);
        self.settle();
    }

    pub fn shortcut(&mut self, keys: &str) {
        self.harness.shortcut(keys);
        self.settle();
    }

    pub fn event(&mut self, name: &str, data: Value) {
        self.harness.event(name, data);
        self.settle();
    }

    pub fn ui(&mut self, handler: HandlerId, value: Option<EventValue>) {
        self.harness.ui(handler, value);
        self.settle();
    }

    /// Every element of the page, depth first.
    pub fn elements(&self) -> Vec<Element> {
        let mut all = Vec::new();
        self.harness
            .view()
            .root
            .for_each(&mut |element| all.push(element.clone()));
        all
    }

    /// The row of a list titled `title`.
    pub fn item(&self, title: &str) -> Option<ListItem> {
        self.elements()
            .into_iter()
            .find_map(|element| match element {
                Element::List(list) => list.items.into_iter().find(|item| item.title == title),
                _ => None,
            })
    }

    /// Every element under a row: its end, actions and body.
    pub fn inside(item: &ListItem) -> Vec<Element> {
        let mut all = Vec::new();
        let mut parts: Vec<&Element> = Vec::new();
        parts.extend(item.end.as_deref());
        parts.extend(item.actions.iter());
        parts.extend(item.body.iter());
        for part in parts {
            part.for_each(&mut |element| all.push(element.clone()));
        }
        all
    }

    /// The field of a key's value: a text input or a text area, `value:KEY`.
    pub fn value_field(&self, key: &str) -> Option<Element> {
        let id = format!("value:{key}");
        self.elements().into_iter().find(|element| match element {
            Element::TextInput(input) => input.id == id,
            Element::TextArea(area) => area.id == id,
            _ => false,
        })
    }

    fn reveal_handler(&self, key: &str) -> Option<HandlerId> {
        let item = self.item(key)?;
        Tab::inside(&item)
            .into_iter()
            .find_map(|element| match element {
                Element::Button(button)
                    if button
                        .label
                        .as_deref()
                        .is_some_and(|label| REVEAL.contains(&label)) =>
                {
                    button.on_press
                }
                _ => None,
            })
    }

    /// Shows or hides a key's value, by its row's button.
    pub fn toggle_reveal(&mut self, key: &str) {
        let handler = self
            .reveal_handler(key)
            .unwrap_or_else(|| panic!("no Show button for {key}"));
        self.ui(handler, None);
    }

    /// Types a value into a key's field, revealing it first when it is a
    /// hidden one of several lines, which takes no typing.
    pub fn edit_row(&mut self, key: &str, value: &str) {
        let handler = |tab: &Tab| match tab.value_field(key) {
            Some(Element::TextInput(input)) => input.on_input,
            Some(Element::TextArea(area)) => area.on_input,
            _ => None,
        };
        let handler = match handler(self) {
            Some(handler) => handler,
            None => {
                self.toggle_reveal(key);
                handler(self).unwrap_or_else(|| panic!("no field for {key}"))
            }
        };
        self.ui(handler, Some(EventValue::Text(value.to_owned())));
    }

    /// Presses the Add button of an offered key.
    pub fn add_offered(&mut self, key: &str) {
        let item = self
            .item(key)
            .unwrap_or_else(|| panic!("{key} is not offered"));
        let handler = item
            .actions
            .iter()
            .find_map(|element| match element {
                Element::Button(button) => button.on_press,
                _ => None,
            })
            .unwrap_or_else(|| panic!("{key} has no Add"));
        self.ui(handler, None);
    }

    /// Chooses a side of a key in the dotenv conflict: `mine` or `theirs`.
    pub fn choose_side(&mut self, key: &str, side: &str) {
        let item = self
            .item(key)
            .unwrap_or_else(|| panic!("no conflict row {key}"));
        let handler = Tab::inside(&item)
            .into_iter()
            .find_map(|element| match element {
                Element::Radio(radio) => radio.on_select,
                _ => None,
            })
            .unwrap_or_else(|| panic!("no choice for {key}"));
        self.ui(handler, Some(EventValue::Text(side.to_owned())));
    }

    /// The radio buttons of a key in the dotenv conflict: (labels, chosen).
    pub fn sides(&self, key: &str) -> (Vec<String>, Option<String>) {
        let item = self
            .item(key)
            .unwrap_or_else(|| panic!("no conflict row {key}"));
        Tab::inside(&item)
            .into_iter()
            .find_map(|element| match element {
                Element::Radio(radio) => Some((
                    radio
                        .options
                        .iter()
                        .map(|option| option.label.clone())
                        .collect(),
                    radio.selected,
                )),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The button labelled `label`, if the page shows one.
    pub fn button(&self, label: &str) -> Option<hangar_kit_sdk::ui::Button> {
        self.elements()
            .into_iter()
            .find_map(|element| match element {
                Element::Button(button) if button.label.as_deref() == Some(label) => Some(button),
                _ => None,
            })
    }

    /// Whether the button labelled `label` can be pressed.
    pub fn enabled(&self, label: &str) -> bool {
        self.button(label)
            .unwrap_or_else(|| panic!("no button {label:?}"))
            .on_press
            .is_some()
    }

    /// The labels of the primary buttons on the page.
    pub fn primaries(&self) -> Vec<String> {
        self.elements()
            .into_iter()
            .filter_map(|element| match element {
                Element::Button(button)
                    if button.kind == hangar_kit_sdk::ui::ButtonKind::Primary =>
                {
                    button.label
                }
                _ => None,
            })
            .collect()
    }

    /// The titles of the page's sections.
    pub fn sections(&self) -> Vec<String> {
        self.elements()
            .into_iter()
            .filter_map(|element| match element {
                Element::Section(section) => Some(section.title),
                _ => None,
            })
            .collect()
    }

    /// The header's state line, as words.
    pub fn header_state(&self) -> String {
        self.elements()
            .into_iter()
            .find_map(|element| match element {
                Element::Header(header) => Some(header),
                _ => None,
            })
            .map(|header| {
                let mut words = Vec::new();
                for part in &header.state {
                    part.for_each(&mut |element| match element {
                        Element::State(state) => words.push(state.label.clone()),
                        Element::Text(text) => words.push(text.text.clone()),
                        _ => {}
                    });
                }
                words.join(" ")
            })
            .unwrap_or_default()
    }

    /// The header's title.
    pub fn title(&self) -> String {
        self.elements()
            .into_iter()
            .find_map(|element| match element {
                Element::Header(header) => Some(header.title),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The page as JSON: every string it carries, to look for a value in.
    pub fn view_json(&self) -> Value {
        serde_json::to_value(self.harness.view()).expect("a view is JSON")
    }

    /// Every string of the page with the key that holds it, depth first.
    pub fn strings(&self) -> Vec<(String, String)> {
        fn walk(value: &Value, key: &str, out: &mut Vec<(String, String)>) {
            match value {
                Value::String(text) => out.push((key.to_owned(), text.clone())),
                Value::Array(items) => items.iter().for_each(|item| walk(item, key, out)),
                Value::Object(map) => map.iter().for_each(|(name, item)| walk(item, name, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(&self.view_json(), "", &mut out);
        out
    }

    /// The cursor moves the tab asked for: (line, column).
    pub fn cursor_moves(&self) -> Vec<(u32, u32)> {
        self.operations
            .iter()
            .filter_map(|operation| match operation {
                Operation::CursorTo { id, line, column } if id == "editor" => {
                    Some((*line, *column))
                }
                _ => None,
            })
            .collect()
    }
}
