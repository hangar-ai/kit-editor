# Editor

A Hangar kit. When an agent needs a value it cannot or must not set (an API key, a
secret, a machine setting), it opens the file in a tab, you fill it in, and the agent
carries on without ever seeing the secret. The same goes for a config file you should
check or edit.

## What it does

- **A dotenv file opens as a form**: one row per key, values hidden until you reveal
  them. The keys the agent asked for come first, marked missing, under its reason. Keys
  in `.env.example` and missing here are offered. Saving changes only the lines you
  edited: comments, order, quotes and line endings stay as they were.
- **It warns when the file is not ignored by git.** It reads the `.gitignore` files, says
  so plainly, and adds the line only when you click. A template such as `.env.example`
  is meant to be committed and gets no warning.
- **Any other text file opens in a plain editor**: line numbers, the line the agent
  pointed at, a diff before saving. JSON is checked by `JSON.parse`. TOML is parsed for
  its syntax and YAML is scanned for what is certainly wrong (a tab in the indentation, a
  quote or a bracket left open): both say it is a basic check, not a proof. A broken file
  can be saved only after you say you know.
- **It never overwrites silently.** If the file changed while you were editing, nothing is
  written and it shows both sides: a diff for a text file, key by key and hidden for a
  dotenv file, where your keys go on top of what the disk now holds. You choose.
- **The agent is told when you save**, with which keys were set or that the file changed.
  With nothing to change, one click says that too.
- **Opened by you, with no params**, it offers a file picker: env files first, then config
  files.

English and French, in Hangar's language; light and dark, in Hangar's theme; legible at
the side panel's narrowest.

**Native on the native Hangar.** The tab is also written in Rust and built to
WebAssembly (`tabs/editor.wasm`): the native Hangar runs that module and draws the tab
with its own widgets, no browser engine; Hangar 1.x opens the HTML page as before. Both do
the same thing, ask for the same three permissions, and send an agent the same sentences.

## What it can touch

Three permissions, shown to you before you enable the kit. It runs nothing by itself: no
hook, no MCP server, no command.

| Permission    | What for                                                                       |
| ------------- | ------------------------------------------------------------------------------ |
| `files.read`  | Read the file you or the agent named, `.gitignore`, `.env.example`; list files. |
| `files.write` | Save the file; add a line to `.gitignore` when you click.                       |
| `panes.send`  | Tell the agent that asked that you saved.                                       |

`files.read` and `files.write` together let a tab read hidden files, `.env` included, and
this is the kit that is for. Nothing under `.git` can be reached, and nothing leaves the
machine.

**What reaches the agent:** the file's path, key names and counts, in sentences fixed in
[`tabs/editor/answer.js`](tabs/editor/answer.js) (and, for the native tab, its port
[`native/src/answer.rs`](native/src/answer.rs)). Never a value. A name that is also a
value in the file, or that you typed yourself in the « Add a key » field, is counted and
not named. `test/no-value-reaches-an-agent.test.js` drives the page over hostile files and
random edits and looks at everything that leaves the page;
`native/tests/no_value_reaches_an_agent.rs` does the same to the native tab, its log
included. The native tab logs nothing, and shows a value only in its own field, dotted
until you click Show.

What it cannot do: a tab cannot read `.git/info/exclude` or your global excludes, or tell
whether a file is already tracked, so a tracked file listed in `.gitignore` reads as
ignored and a file excluded elsewhere still gets the warning. `files.read` drops a BOM and
the page cannot know there was one, so saving a file that had one saves it without.

What the native tab does otherwise than the page: a value over several lines shows as a
dotted one-line field until you click Show, then in a plain text area (Hangar's text areas
have no dotted mode; the page did the same). In the editor, a line longer than the pane is
cut at its edge rather than scrolled to, and Ctrl+S works anywhere in the tab, not only in
a field. It is opened with its params, so it does not ask Hangar for them again.

## Install

You need Hangar 1.9.0 or later. The native Hangar opens the native tab; an older one, the
page.

1. In Hangar, Settings › Kits, add the GitHub link:
   `https://github.com/hangar-ai/kit-editor`
   (or link a local copy of this folder).
2. Enable it on a workspace, and read what you accept: the three permissions above.
3. Restart the workspace's Claude Code panes, so they load the skill.

## For agents

The skill in [`skills/editor/SKILL.md`](skills/editor/SKILL.md) says it: do not ask for a
secret in chat and do not write a placeholder. Call `kit_tab_open`, then wait for the
answer, and never read the values back.

```json
{ "kit": "editor", "tab": "editor", "params": { "path": ".env", "keys": "STRIPE_KEY,DB_URL", "why": "For the payment tests." } }
```

| Param  | What                                                                       |
| ------ | -------------------------------------------------------------------------- |
| `path` | The file, relative to the workspace folder. Without it, the picker.        |
| `keys` | For a dotenv file, the keys to fill in, separated by commas.               |
| `why`  | One sentence, shown at the top.                                            |
| `line` | For a text file, the line to jump to.                                      |

Hangar keeps one tab per set of params. So ask for all your keys at once: the same file
with other `keys` opens a second tab, and `expected` keeps the two from overwriting each
other.

## How it is built

```
editor/
├── hangar-kit.json            the kit: id, hangar >=1.9.0, the tab, its params and permissions
├── icon.svg
├── .claude-plugin/plugin.json the Claude Code plugin (name = the kit's id)
├── skills/editor/SKILL.md
├── tabs/editor.wasm           the native tab, built from native/ and committed: what `native` names
├── tabs/editor/               the page, what Hangar 1.x opens (the manifest's `entry`)
│   ├── index.html             loads Hangar UI and the SDK, then the scripts below
│   ├── dotenv.js              reads a dotenv file, and writes back only what changed
│   ├── answer.js              the only sentences an agent ever gets
│   ├── validate.js            JSON, YAML, TOML
│   ├── diff.js                lines compared, and line endings a textarea drops, restored
│   ├── gitignore.js           git's ignore rules
│   ├── logic.js               the words in en and fr, params, the picker's order
│   ├── controller.js          the behaviour; takes window.hangar as an argument
│   ├── view.js                draws a state with Hangar UI's classes, never markup
│   ├── app.js                 wires them in the page
│   └── style.css              the editor's layout, in Hangar UI's tokens
├── test/                      node --test, no dependency, a fake window.hangar over made-up files
├── native/                    the same tab in Rust, on Hangar's kit SDK (hangar-kit-sdk)
│   ├── src/                   one file per file of the page: dotenv.rs, answer.rs, validate.rs,
│   │                          diff.rs, gitignore.rs, logic.rs, editor.rs (controller.js), view.rs
│   ├── tests/                 the cases of test/, ported, through the SDK's test harness
│   ├── Cargo.toml, Cargo.lock the SDK from Hangar's repository, pinned to a commit
│   └── rust-toolchain.toml    the Rust that builds the module
├── scripts/build-native.sh    builds native/ into tabs/editor.wasm
└── package.json, run-tests.js let `node --test kits/editor/` find the tests on any Node
```

### The native tab

```
scripts/build-native.sh
```

builds `native/` for `wasm32-unknown-unknown` and copies the module to `tabs/editor.wasm`.
It needs Rust; `native/rust-toolchain.toml` names the version and the target, which rustup
installs the first time. Hangar never builds a kit: it runs the module the kit ships, so
`tabs/editor.wasm` is committed, rebuilt in the commit that changes `native/`. The build
takes the paths of the machine out of the module, so it is the same byte for byte wherever
it is built, and CI checks that the committed one is what the source makes.

The manifest keeps `"schema": 1` and its HTML `entry`, and adds `"native":
"tabs/editor.wasm"`: Hangar 1.x ignores the field and opens the page; the native Hangar opens
the module, and the page only when the module cannot load. To try a change, link this
folder in Hangar's Settings › Kits (it is read live), run the script, and reload the tab
from its bar.

## Tests

```
node --test kits/editor/
cargo test --manifest-path native/Cargo.toml
```

No install step for the first, the page's: Node alone. The second runs the native tab on
the host, with no WebAssembly and no Hangar: a fake Hangar answers its calls from files in
memory, under the manifest's three permissions, and the tests press, type and read its page
as a person would. No test reads a real file of yours: the fake Hangar holds its files in
memory, and the check of the ignore rules against `git check-ignore` runs in a temporary
folder.

## This repository

`node --test` runs the tests, with Node alone. Hangar's own files are not in this repository. CI takes Hangar UI and the docs from the Hangar tag the
manifest's `hangar` names, and `node test/hangar-ui.js --get` does the same here. Without them, these tests
skip what needs those files, and say so:

- the permissions and methods used are in the docs, when the docs are around
- Hangar UI's classes, and no colour of its own

CI runs the native tab's tests, builds the module and fails if the committed
`tabs/editor.wasm` is not the one the source makes. It also calls [kit-check](https://github.com/hangar-ai/.github/blob/main/.github/workflows/kit-check.yml). A release is
a tag, `v` and the manifest's version, and it reaches Hangar's gallery once
[the catalog](https://github.com/hangar-ai/catalog) is updated and reviewed.
