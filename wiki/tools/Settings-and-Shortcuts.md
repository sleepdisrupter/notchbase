# Settings and Shortcuts

Tools declare their settings and global shortcuts in the manifest. notchbase stores the user's choices in [`config.json`](../Configuration.md#tools), shows settings in the notch's **Settings** page, shows shortcuts as keycaps people can click to rebind, and tells your tool whenever anything changes.

## Declaring settings

```rust
fn manifest() -> Manifest {
    Manifest::new("clipper", "Clipper")
        .setting(SettingSpec::toggle("copy", "Copy results", true).detail("Put each result on the clipboard"))
        .setting(SettingSpec::choice("format", "Format", [("png", "PNG"), ("jpeg", "JPEG")], "png"))
        .setting(SettingSpec::number("interval", "Check every", [1.0, 60.0], 1.0, 5.0).unit("min"))
        .setting(SettingSpec::toggle("save", "Save clips", false))
        .setting(SettingSpec::folder("folder", "Clips folder").placeholder("Pictures/Clips").shown_when("save"))
        .setting(SettingSpec::choice("source", "Source", [("cloud", "Cloud"), ("server", "Own server")], "cloud"))
        .setting(SettingSpec::text("token", "Token", "").placeholder("Paste your token").secret().shown_when_is("source", "server"))
        .setting(SettingSpec::data("history", Value::Array(Vec::new())))
}
```

| Constructor | Stored as | Default |
| --- | --- | --- |
| `SettingSpec::toggle(key, title, default: bool)` | `true` / `false` | `default` |
| `SettingSpec::choice(key, title, options, default: &str)` | the chosen option's value, a string | `default` |
| `SettingSpec::number(key, title, [min, max], step, default: f64)` | a number | `default` |
| `SettingSpec::text(key, title, default: &str)` | a string | `default` |
| `SettingSpec::folder(key, title)` | a folder path, a string | `""` (meaning: your own default folder) |
| `SettingSpec::data(key, default: Value)` | any JSON | `default` |

`choice` options are `(value, label)` pairs: the value is stored, the label is shown.

| Builder | Effect |
| --- | --- |
| `.detail(text)` | A line of explanation under the title |
| `.unit(text)` | A unit shown with a `number`, such as `"min"` or `"px"` |
| `.placeholder(text)` | Hint text for a `text` or `folder` setting. For a folder, say where files go when none is chosen |
| `.secret()` | For a `text` setting holding a key, token or private link: the Settings page shows it as dots, even while it's being typed |
| `.shown_when(toggle_key)` | Only show this row while the toggle setting `toggle_key` is on |
| `.shown_when_is(choice_key, value)` | Only show this row while the choice setting `choice_key` is set to `value` |
| `.hidden()` | Don't show it in the Settings page |

`unit` only affects `number` settings, `placeholder` only `text` and `folder` settings, and `secret` only `text` settings; on other kinds they do nothing.

Keys are stored in people's `config.json`, so treat them like an API: keep them short, lowercase and stable.

## Where settings show up

Each tool's settings have their own page in the notch: click the gear, choose **Tools**, then the cog beside your tool. The cog only appears when your tool has at least one visible setting. You can also open that page yourself with [`Command::open_settings()`](Commands.md#open_settings), for a "set this up" button. The page lists your visible settings, each row with the title, the detail line under it, and a control:

| Kind | Shown as |
| --- | --- |
| `toggle` | A switch |
| `choice` with up to 4 options | A segmented control of the labels |
| `number` | − and + buttons around the value and its unit, moving by `step` within the range |
| `choice` with more than 4 options | Not shown; `config.json` only |
| `folder` | The chosen folder's path (or the placeholder, dimmed), a **Choose…** button that opens the system folder picker, and a × that goes back to the default |
| `text` | A text field. Click it to type; Enter, clicking elsewhere or closing the notch saves it (trimmed), and Esc throws the edit away. A × beside a filled field clears it. With `.secret()` it shows dots instead of the text |
| `data` | Never shown |

Hidden settings (`.hidden()`) don't appear either. A setting with `.shown_when(key)` appears only while that toggle is on, and one with `.shown_when_is(key, value)` only while that choice has that value, sliding in under it. The built-in Screenshot tool uses the first: **Screenshot folder** and **File format** only show once **Save screenshots** is on. The Calendar uses the second: on a Mac, **iCal link** only shows once **Source** is set to iCal link. Everything is stored the same way, and can always be edited in `config.json`. Hide a setting when your own UI already controls it: the built-in recorder keeps its microphone and desktop-audio choices hidden, for example, because the chips on its tile set them. A text field saves only when the edit ends, so your tool isn't sent a new value on every keystroke. To react while people type, put a [`text_input`](Elements.md#text-input) on your page instead.

## Defaults in config.json

The first time notchbase sees your tool, it writes every setting (and every shortcut) into `config.json` at its default. Settings you add in a later version are written the same way when they first appear. Values already in the file are never replaced, so changing a default in a new version only affects people who haven't got the setting yet. If you need to change what an existing setting means, give it a new key.

## Reading settings

`init` gets the settings through `context.settings()`, and [`Subscription::settings`](Subscriptions.md#settings) delivers them again whenever they change. Either way you get a `Settings` with the stored values laid over your manifest's defaults, so every declared key has a value.

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub copy: bool,
    pub jpeg: bool,
    pub minutes: f64,
    pub folder: Option<PathBuf>,
    pub history: Vec<String>,
}

impl Prefs {
    pub fn from(settings: &Settings) -> Self {
        Self {
            copy: settings.bool("copy"),
            jpeg: settings.text("format") == "jpeg",
            minutes: settings.number("interval"),
            folder: settings.path("folder"),
            history: settings.parse("history").unwrap_or_default(),
        }
    }
}
```

| Method | Returns | When the value is missing or the wrong type |
| --- | --- | --- |
| `bool(key)` | `bool` | `false` |
| `number(key)` | `f64` | `0.0` |
| `text(key)` | `&str` | `""` |
| `string(key)` | `Option<String>`, trimmed | `None`, also for blank text |
| `path(key)` | `Option<PathBuf>`, for `folder` settings | `None`, also when no folder was chosen |
| `parse::<T>(key)` | `Option<T>` for any `serde::Deserialize` type | `None` |
| `get(key)` | `Option<&Value>`, the raw JSON | `None` |
| `values()` | `&Values`, every key | |

A convenient pattern, used by all the built-in tools, is a small `Prefs` struct built from `Settings` in both `init` and the settings message.

People can edit `config.json` by hand, and the range of a `number` or the options of a `choice` aren't enforced there. Clamp and validate what you read:

```rust
let minutes = settings.number("interval").clamp(1.0, 60.0);
```

`Settings::new(values)` builds one from a map, which is handy in tests.

## Saving settings

```rust
Msg::Mic(on) => {
    self.options.microphone = on;
    Command::save_setting("microphone", on)
}
```

[`Command::save_setting(key, value)`](Commands.md#save_setting) writes the value to `config.json`. The SDK's copy of your settings takes the new value at once, and notchbase then sends the full updated settings back through `Subscription::settings`. Update your model right away as well, as above, so the UI doesn't wait for the round trip.

`save_setting` always applies to your own tool's settings. Declare every key you save in the manifest, so it has a default and a known kind.

## Saving state with data settings

A `data` setting is a hidden slot for any JSON, which makes it the simplest way to keep state across restarts: a history, a counter, a list of favorites.

```rust
.setting(SettingSpec::data("history", Value::Array(Vec::new())))
```

```rust
fn init(context: &Context) -> (Self, Command<Msg>) {
    let history: Vec<String> = context.settings().parse("history").unwrap_or_default();
    (Self { history }, Command::none())
}

fn remember(&mut self, item: String) -> Command<Msg> {
    self.history.push(item);
    Command::save_setting("history", self.history.clone())
}
```

`parse` works with your own types too, if they derive `serde::Deserialize` (add `serde` with the `derive` feature to your crate). To save such a type, turn it into a `Value` with `serde_json::to_value`.

Keep data settings small. `config.json` is rewritten on every save and is meant to stay readable by people. For anything large, write your own file under [`context.data_dir()`](Elm-Architecture.md#context), from a background thread.

## Shortcuts

### Declaring

```rust
Manifest::new("clipper", "Clipper")
    .shortcut("capture", "Capture", Some("Ctrl+Shift+X"))
    .shortcut("paste", "Paste Last", None)
```

`shortcut(id, title, default)` declares a global shortcut: it works while any app is in front. `default` is the binding a new user starts with, or `None` to start unbound. The `id` is how you refer to it everywhere else and how it's stored in `config.json`.

### Writing key combinations

A combination is modifiers and one key joined with `+`, modifiers first. Case doesn't matter.

| Modifiers | Accepted names |
| --- | --- |
| Control | `Ctrl`, `Control` |
| Alt / Option | `Alt`, `Option` |
| Shift | `Shift` |
| Windows key / Command | `Super`, `Cmd`, `Command` |
| Command on macOS, Control elsewhere | `CmdOrCtrl`, `CmdOrControl`, `CommandOrCtrl`, `CommandOrControl` |

| Keys | Accepted names |
| --- | --- |
| Letters and digits | `A`–`Z`, `0`–`9` (also `KeyA`, `Digit0`…) |
| Function keys | `F1`–`F24` |
| Editing and navigation | `Space`, `Enter`, `Tab`, `Backspace`, `Delete`, `Insert`, `Escape` (`Esc`), `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right` (also `ArrowUp`…), `PrintScreen`, `CapsLock`, `NumLock`, `ScrollLock`, `Pause` |
| Punctuation | `` ` `` `-` `=` `[` `]` `\` `;` `'` `,` `.` `/` (or `Backquote`, `Minus`, `Equal`, `BracketLeft`, `BracketRight`, `Backslash`, `Semicolon`, `Quote`, `Comma`, `Period`, `Slash`) |
| Numeric keypad | `Num0`–`Num9`, `NumAdd`, `NumSubtract`, `NumMultiply`, `NumDivide`, `NumDecimal`, `NumEnter`, `NumEqual` (also `Numpad0`…) |
| Media and volume | `MediaPlayPause`, `MediaPlay`, `MediaPause`, `MediaStop`, `MediaTrackNext`, `MediaTrackPrevious`, `VolumeUp`, `VolumeDown`, `VolumeMute` |

Examples: `Ctrl+Shift+C`, `Ctrl+Alt+4`, `Alt+Space`, `F5`, `CmdOrCtrl+Shift+K`.

When notchbase writes a combination back to `config.json` it uses the friendly spelling: `Ctrl`, `Alt` (`Option` on macOS), `Shift`, `Super` (`Cmd` on macOS), then the key, as in `Ctrl+Shift+C`.

Choose defaults that are unlikely to clash with common apps. The built-in tools use `Ctrl+Shift+C`, `Ctrl+Alt+S` and `Ctrl+Alt+R`.

### Receiving presses

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    Subscription::shortcut("capture", Msg::Capture)
}
```

Each press sends the message. notchbase registers the keys from your manifest and the user's configuration, whether or not you subscribe; without a subscription, presses are simply dropped.

A common pattern is for the same shortcut to start and cancel an action:

```rust
Msg::Capture if self.capturing => {
    self.capturing = false;
    Command::cancel_capture()
}
Msg::Capture => {
    self.capturing = true;
    Command::capture_screen(Msg::Captured)
}
```

### Letting people rebind

Show the shortcut and people can change it by clicking it:

- `Tile::shortcut(id)` puts it at the bottom left of your tile.
- The `shortcut(id)` element puts it anywhere on your page.

Both show the current keys as keycaps, or **Add Shortcut** when unbound. Clicking starts recording: the next combination pressed becomes the new binding, a × button clears it, and clicking again cancels. If a combination can't be used, the field says why.

### Where bindings are stored

```json
{
  "tools": {
    "clipper": {
      "shortcuts": {
        "capture": "Ctrl+Alt+X",
        "paste": null
      }
    }
  }
}
```

| Value in `tools.<id>.shortcuts.<shortcut id>` | Meaning |
| --- | --- |
| a combination such as `"Ctrl+Alt+X"` | Bound to that combination |
| `null` | Deliberately unbound |
| absent | The manifest's default |

notchbase writes each shortcut's default in the first time it sees your tool, so normally every shortcut is listed. Edits to this file take effect within about a second. See [Configuration](../Configuration.md#tools).
