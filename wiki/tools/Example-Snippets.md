# Example: Snippets

A complete tool that goes beyond a single tile. **Snippets** keeps a list of short texts (email templates, addresses, commands) and copies them with one click or a shortcut.

It shows:

- a page with a text field, a list and a shortcut field
- a tile that opens the page
- saving data across restarts with a hidden `data` setting
- a visible `number` setting, read defensively
- a notice with an extra line below
- tests for each flow

## Cargo.toml

```toml
[package]
name = "snippets"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
notchbase-sdk = { path = "../notchbase/sdk" }
```

## src/lib.rs

```rust
use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

pub const ID: &str = "snippets";
const COPY_NEWEST: &str = "copy-newest";
const SAVED: &str = "saved";
const LIMIT: &str = "limit";
const PREVIEW_CHARS: usize = 40;

pub struct Snippets {
    saved: Vec<String>,
    draft: String,
    limit: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Open,
    Draft(String),
    Add,
    Copy(usize),
    CopyNewest,
    Remove(usize),
    Configured(Settings),
}

impl Tool for Snippets {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Snippets")
            .description("Keep short texts one click away")
            .version(env!("CARGO_PKG_VERSION"))
            .page("Snippets")
            .shortcut(COPY_NEWEST, "Copy Newest Snippet", Some("Ctrl+Alt+V"))
            .setting(SettingSpec::number(LIMIT, "Keep at most", [5.0, 50.0], 5.0, 20.0).unit("snippets"))
            .setting(SettingSpec::data(SAVED, Value::Array(Vec::new())))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let settings = context.settings();
        let snippets = Self {
            saved: settings.parse(SAVED).unwrap_or_default(),
            draft: String::new(),
            limit: limit(settings),
        };
        (snippets, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Open => Command::open_page(),
            Msg::Draft(text) => {
                self.draft = text;
                Command::none()
            }
            Msg::Add => {
                let snippet = std::mem::take(&mut self.draft).trim().to_owned();
                if snippet.is_empty() {
                    return Command::none();
                }
                self.saved.retain(|saved| *saved != snippet);
                self.saved.insert(0, snippet);
                self.saved.truncate(self.limit);
                self.save()
            }
            Msg::Copy(index) => self.copy(index),
            Msg::CopyNewest => self.copy(0),
            Msg::Remove(index) => {
                if index < self.saved.len() {
                    self.saved.remove(index);
                }
                self.save()
            }
            Msg::Configured(settings) => {
                self.saved = settings.parse(SAVED).unwrap_or_default();
                self.limit = limit(&settings);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([
            Subscription::shortcut(COPY_NEWEST, Msg::CopyNewest),
            Subscription::settings(Msg::Configured),
        ])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let count = text(self.saved.len().to_string()).value().secondary();
        Some(Tile::new("Snippets").icon(Icon::Note).shortcut(COPY_NEWEST).on_press(Msg::Open).accessory(count))
    }

    fn page(&self) -> Option<Element<Msg>> {
        let composer = row![
            text_input(&self.draft).placeholder("New snippet").on_change(Msg::Draft).on_submit(Msg::Add),
            button("Add").filled().on_press(Msg::Add),
        ];
        let saved = if self.saved.is_empty() {
            card(text("Nothing saved yet").secondary())
        } else {
            list(self.saved.iter().enumerate().map(|(index, snippet)| snippet_row(index, snippet)))
        };
        let hint = row![text("Copy the newest").caption().secondary(), spacer(), shortcut(COPY_NEWEST)];
        Some(column![composer, saved, hint].spacing(10.0).into())
    }
}

impl Snippets {
    fn copy(&self, index: usize) -> Command<Msg> {
        let Some(snippet) = self.saved.get(index) else { return Command::notify("No snippets saved yet") };
        let notice = Notice::new(icon(Icon::Checkmark).tone(Tone::Green), text("Copied").callout())
            .below(text(preview(snippet)).caption().secondary())
            .duration(Duration::from_millis(1800));
        Command::copy_text(snippet.clone()).and(Command::notice(notice))
    }

    fn save(&self) -> Command<Msg> {
        Command::save_setting(SAVED, self.saved.clone())
    }
}

fn limit(settings: &Settings) -> usize {
    settings.number(LIMIT).clamp(5.0, 50.0) as usize
}

fn snippet_row(index: usize, snippet: &str) -> Element<Msg> {
    let actions = row![
        button("Copy").plain().on_press(Msg::Copy(index)),
        button("").icon(Icon::Xmark).plain().on_press(Msg::Remove(index)),
    ];
    keyed(snippet, row![text(preview(snippet)), spacer(), actions].spacing(6.0))
}

fn preview(snippet: &str) -> String {
    let mut chars = snippet.chars();
    let short: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    if chars.next().is_some() { format!("{short}…") } else { short }
}

notchbase_sdk::export!(Snippets);
```

## How it works

### The tile opens the page

The tile shows how many snippets are saved, and clicking it sends `Msg::Open`, which returns `Command::open_page()`. The shortcut field at the bottom of the tile lets people rebind **Copy Newest Snippet** without opening anything.

### Typing

`text_input(&self.draft)` shows the draft from the model. Every keystroke sends `Msg::Draft` with the whole text, which the model stores; Enter sends `Msg::Add`, as does the **Add** button. `std::mem::take` empties the draft as the snippet is added, so the field clears.

### The list

Each snippet is a row in a `list`, wrapped in `keyed(snippet, …)`. Because adding a snippet inserts it at the top, every row moves down one place; the key keeps each row's hover state with its own snippet. Adding removes any existing copy first (`retain`), so keys stay unique.

Long snippets are shortened for display by `preview`; the full text is what gets copied.

### Saving across restarts

The list lives in a `data` setting, which is hidden from the Settings page:

```rust
.setting(SettingSpec::data(SAVED, Value::Array(Vec::new())))
```

`init` reads it with `settings.parse(SAVED)`, and every change saves it with `Command::save_setting(SAVED, self.saved.clone())`. A `Vec<String>` converts into a JSON array directly. In `config.json` it looks like:

```json
{
  "tools": {
    "snippets": {
      "enabled": true,
      "shortcuts": {
        "copy-newest": "Ctrl+Alt+V"
      },
      "settings": {
        "limit": 20,
        "saved": ["see you tomorrow!", "https://example.com/meet"]
      }
    }
  }
}
```

After each save notchbase sends the settings back through `Subscription::settings`, and `Msg::Configured` reloads the list from them. That's also what makes hand edits to `config.json` show up in the tool.

### A visible setting, read defensively

**Keep at most** appears on the Settings page with − and + buttons that move it from 5 to 50 in steps of 5. The range only binds the Settings page; `config.json` can hold anything, so `limit()` clamps what it reads.

### Copying

`copy` puts the text on the clipboard and shows a notice with a check mark, "Copied", and the snippet's preview on a line below. If there's nothing to copy, a plain message says so.

## Tests

```rust
#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Event, Request, Start, Values};
    use notchbase_sdk::{Driver, Runtime, Waker};

    use super::*;

    fn started(saved: &[&str]) -> Runtime<Snippets> {
        let mut settings = Values::new();
        settings.insert(SAVED.into(), saved.iter().map(|text| text.to_string()).collect::<Vec<_>>().into());
        let mut runtime = Runtime::new(Waker::noop());
        runtime.start(Start { settings, ..Start::default() });
        runtime
    }

    #[test]
    fn adding_saves_the_list_newest_first() {
        let mut runtime = started(&["old"]);
        runtime.send(Msg::Draft("  new  ".into()));
        let effects = runtime.send(Msg::Add);
        let [Request::SaveSetting { key, value }] = &effects.requests[..] else { panic!("{:?}", effects.requests) };
        assert_eq!(key, SAVED);
        assert_eq!(value, &Value::from(vec!["new", "old"]));
    }

    #[test]
    fn the_shortcut_copies_the_newest_and_says_so() {
        let mut runtime = started(&["hello", "world"]);
        let requests = runtime.handle(vec![Event::Shortcut { id: COPY_NEWEST.into() }]).requests;
        assert_eq!(requests[0], Request::CopyText { text: "hello".into() });
        assert!(matches!(&requests[1], Request::Notice { notice } if notice.below.is_some()));
    }

    #[test]
    fn edits_to_config_json_reach_the_list() {
        let mut runtime = started(&[]);
        let mut values = Values::new();
        values.insert(SAVED.into(), vec!["from disk"].into());
        runtime.handle(vec![Event::Settings { values }]);
        assert_eq!(runtime.tool().unwrap().saved, ["from disk"]);
    }

    #[test]
    fn long_snippets_are_shortened_for_display() {
        assert_eq!(preview("short"), "short");
        assert_eq!(preview(&"x".repeat(41)).chars().count(), 41);
    }
}
```

## Try it

```sh
cargo test
cargo build --release
```

Copy `target/release/snippets.dll` (Windows) or `target/release/libsnippets.dylib` (macOS) into `tools/snippets/` next to the notchbase executable and restart notchbase. The **Snippets** tile appears at the end of the Tools tab, and clicking it opens the page. To also give Snippets its own tab at the end of the header, add `.header_tab()` to the manifest. Its tile stays, because the tool draws it itself; only a tool without a tile of its own loses the automatic launcher tile once it has a tab.

See [Getting Started](Getting-Started.md) and [Plugins and Distribution](Plugins-and-Distribution.md) for the details.

## Ideas for extending it

- Let people rename snippets with a second text field and a `data` setting holding `{ "title": …, "text": … }` objects, read with `settings.parse::<Vec<YourType>>(…)`.
- Add a `segmented` control on the page to sort by most recent or alphabetically.
- Save whatever text is on the clipboard as a new snippet with a second shortcut. The SDK can write the clipboard but not read it, so read it yourself (for example with the `arboard` crate) inside `Command::perform`.
