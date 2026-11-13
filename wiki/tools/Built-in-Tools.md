# Built-in Tools

notchbase's own tools are written with the same SDK as any plugin. They live in [`tools/`](../../tools) and are compiled into notchbase. Each is short, tested, and shows a different side of the SDK, so they make good templates.

| Tool | Folder | Id | Shows how to |
| --- | --- | --- | --- |
| Pick Color | [`tools/color-picker`](../../tools/color-picker) | `color-picker` | Ask notchbase a question and act on the answer; a choice setting; an adaptive notice |
| Screenshot | [`tools/screenshot`](../../tools/screenshot) | `screenshot` | Captures, clipboard and files; a `Prefs` struct; settings shown only while a toggle is on; a notice with a thumbnail and a button that opens the annotator |
| Record Screen | [`tools/screen-recorder`](../../tools/screen-recorder) | `screen-recorder` | A tile that changes with state; a compact dropdown and an icon chip on a tile; a live activity; recording and microphone subscriptions |
| Timer | [`tools/timer`](../../tools/timer) | `timer` | A page that swaps views with state; a picker, round buttons and a draining ring; a chime and a sticky alarm; an injectable clock for tests |
| Alerts | [`tools/alerts`](../../tools/alerts) | `alerts` | A long-running background server; settings that restart it; sticky, shaking banner notices; a tool with no tile or page |
| Keep Awake | [`tools/keep-awake`](../../tools/keep-awake) | `keep-awake` | A tile that is a switch; a host request held for as long as the tool asks; a timer only while a timed session runs; a setting that hides the live activity |
| Battery, Focus | [`tools/battery`](../../tools/battery), [`tools/focus-status`](../../tools/focus-status) | `battery`, `focus-status` | Tools with no tile or page that turn a system feed into notices; treating the first reading as the starting point; adaptive notices |
| Camera & Microphone, Downloads | [`tools/camera-mic`](../../tools/camera-mic), [`tools/downloads`](../../tools/downloads) | `camera-mic`, `downloads` | A live activity that comes and goes with a feed; controls on its ear; a banner and `Command::shelve` when a download lands; a question on the ear answered with `Command::perform` (unzipping) and timed out with `Command::spawn` |
| Calendar | [`tools/calendar`](../../tools/calendar) | `calendar` | Background downloads with a disk cache; the calendar subscription and event writes; a choice that reveals a secret text setting; an editor built from a picker and dropdowns; a live activity that picks its moment |

The ids are what their settings and shortcuts are stored under in [`config.json`](../Configuration.md#tools), and what a plugin would use to replace one of them. This page walks through the first three; Alerts and Calendar have pages of their own, [Alerts](../Alerts.md) and [Calendar](../Calendar.md), the system tools share [System tools](../System-Tools.md), and Alerts is summarized [below](#alerts).

---

## Pick Color

Click the tile or press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>, pick any pixel on screen, and its color is copied as HEX, RGB or HSL.

### Manifest

```rust
fn manifest() -> Manifest {
    Manifest::new(ID, "Pick Color")
        .description("Pick any color on screen and copy it")
        .version(env!("CARGO_PKG_VERSION"))
        .shortcut(PICK, "Pick Color", Some("Ctrl+Shift+C"))
        .setting(
            SettingSpec::choice(FORMAT, "Color format", Format::CHOICES, "hex").detail("How picked colors are copied"),
        )
}
```

`Format::CHOICES` is `[("hex", "HEX"), ("rgb", "RGB"), ("hsl", "HSL")]`. The setting is visible, so it appears on the Settings page.

### Model and messages

```rust
pub struct ColorPicker {
    format: Format,
    picking: bool,
}

pub enum Msg {
    Pick,
    Picked(Option<Color>),
    Configured(Settings),
}
```

### Update

```rust
fn update(&mut self, msg: Msg) -> Command<Msg> {
    match msg {
        Msg::Pick if self.picking => {
            self.picking = false;
            Command::cancel_capture()
        }
        Msg::Pick => {
            self.picking = true;
            Command::pick_color(Msg::Picked)
        }
        Msg::Picked(color) => {
            self.picking = false;
            color.map_or_else(Command::none, |color| self.copied(color))
        }
        Msg::Configured(settings) => {
            self.format = Format::named(settings.text(FORMAT));
            Command::none()
        }
    }
}
```

- The `picking` flag makes the shortcut a toggle: pressing it again while the picker is up cancels it with `cancel_capture`.
- `pick_color` answers with `None` when the user cancels, and the tool does nothing.
- `Format::named` falls back to HEX for anything it doesn't recognize, so a mistyped value in `config.json` is harmless.

### The copied notice

```rust
fn copied(&self, color: Color) -> Command<Msg> {
    let written = self.format.write(color);
    let notice = Notice::new(adaptive(swatch(color).size(18.0), swatch(color).size(20.0)), text(&written).value())
        .pop()
        .duration(COPIED_FOR);
    Command::batch([Command::copy_text(written), Command::notice(notice)])
}
```

The swatch is a little smaller beside a camera housing than on an island, thanks to `adaptive`, and `.pop()` makes it spring in.

### Tile and subscriptions

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    Subscription::batch([Subscription::shortcut(PICK, Msg::Pick), Subscription::settings(Msg::Configured)])
}

fn tile(&self) -> Option<Tile<Msg>> {
    Some(Tile::new("Pick Color").icon(Icon::Pipette).shortcut(PICK).on_press(Msg::Pick))
}
```

The whole tool is a tile with its shortcut along the bottom. `format.rs` holds the color math, with its own unit tests.

---

## Screenshot

Click the tile or press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>S</kbd>, drag out a region, and it's copied to the clipboard, saved to a file, or both. The notice that confirms it has an **Annotate** button, which opens the screenshot in a window to draw and write on. What is added there reaches the clipboard only.

### Manifest

```rust
fn manifest() -> Manifest {
    Manifest::new(ID, "Screenshot")
        .description("Capture a region of the screen")
        .version(env!("CARGO_PKG_VERSION"))
        .shortcut(CAPTURE, "Screenshot", Some("Ctrl+Alt+S"))
        .setting(SettingSpec::toggle(COPY, "Copy screenshots", true).detail("Put each screenshot on the clipboard"))
        .setting(SettingSpec::toggle(SAVE, "Save screenshots", false).detail("Also keep a file of each one"))
        .setting(SettingSpec::folder(FOLDER, "Screenshot folder").placeholder(DEFAULT_FOLDER).shown_when(SAVE))
        .setting(
            SettingSpec::choice(FORMAT, "File format", [("png", "PNG"), ("jpeg", "JPEG")], "png").shown_when(SAVE),
        )
}
```

Two settings are always on the tool's settings page. The folder and the file format use `.shown_when(SAVE)`, so they appear under **Save screenshots** only while it is on. `DEFAULT_FOLDER` is `"Pictures/Screenshots"`.

Neither setting changes what **Annotate** does. The button is offered whether the capture was copied or only saved, and the drawing reaches the clipboard only: each change in the annotator is copied again, even with **Copy screenshots** off, and the file kept by **Save screenshots** stays as captured, without the drawing.

### Prefs

```rust
struct Prefs {
    copy: bool,
    save: bool,
    folder: Option<PathBuf>,
    format: ImageFormat,
}

impl Prefs {
    fn from(settings: &Settings) -> Self {
        Self {
            copy: settings.bool(COPY),
            save: settings.bool(SAVE),
            folder: settings.path(FOLDER),
            format: if settings.text(FORMAT) == "jpeg" { ImageFormat::Jpeg } else { ImageFormat::Png },
        }
    }
}
```

All settings are read in one place, used by both `init` and the settings message. `settings.path(FOLDER)` is `None` for an empty folder, which `save_image` treats as "notchbase's default".

### Messages and update

```rust
pub enum Msg {
    Capture,
    Captured(Option<Capture>),
    Annotate(ImageId),
    Saved(Result<String, String>),
    Configured(Settings),
}
```

```rust
fn update(&mut self, msg: Msg) -> Command<Msg> {
    match msg {
        Msg::Capture if self.capturing => {
            self.capturing = false;
            Command::cancel_capture()
        }
        Msg::Capture => {
            self.capturing = true;
            Command::capture_screen(Msg::Captured)
        }
        Msg::Captured(capture) => {
            self.capturing = false;
            capture.map_or_else(Command::none, |capture| self.keep(capture))
        }
        Msg::Annotate(image) => Command::dismiss_notice().and(Command::annotate(image)),
        Msg::Saved(Err(err)) => Command::notify(err),
        Msg::Saved(Ok(_)) => Command::none(),
        Msg::Configured(settings) => {
            self.prefs = Prefs::from(&settings);
            Command::none()
        }
    }
}
```

The capture toggle works like Pick Color's, with `capture_screen` instead of `pick_color`. When a capture arrives:

```rust
fn keep(&self, capture: Capture) -> Command<Msg> {
    let copy = self.prefs.copy && !capture.on_clipboard;
    let copied = self.prefs.copy || capture.on_clipboard;
    let shown = notice(&capture, if copied { "Copied" } else { "Saved" });
    let annotate = button(ANNOTATE).on_press(Msg::Annotate(capture.image));
    let mut commands = vec![Command::notice_with_controls(shown, annotate)];
    if copy {
        commands.push(Command::copy_image(capture.image));
    }
    if self.prefs.save || !copied {
        let folder = self.prefs.folder.as_deref();
        commands.push(Command::save_image(capture.image, folder, PREFIX, self.prefs.format, Msg::Saved));
    }
    Command::batch(commands)
}
```

- It doesn't copy an image that's already on the clipboard (`capture.on_clipboard`).
- If the image wasn't copied at all, it's always saved, so a capture is never lost.
- A failed save (`Msg::Saved(Err(err))`) is shown with `Command::notify(err)`.
- The notice goes out with [`notice_with_controls`](Commands.md#notice_with_controls): the control is a plain `button("Annotate")` whose message carries the capture's `ImageId`. It is offered for every capture, copied or only saved. The tool saves the capture as it is and never hears about the drawing, so a saved file is always the original.
- Pressing it sends `Msg::Annotate(image)`, answered with two commands: `dismiss_notice`, so the notice makes way at once, and [`annotate`](Commands.md#annotate), which asks notchbase to open that image in the [annotator](../Annotate.md). The tool keeps no state for it; from there the window is notchbase's.

### The notice

```rust
fn notice(capture: &Capture, done: &str) -> Notice {
    let aspect = capture.width as f32 / capture.height.max(1) as f32;
    let [width, height] = if aspect >= WIDE_ASPECT { [30.0, 30.0 / aspect] } else { [20.0 * aspect, 20.0] };
    let thumbnail = image(ImageSource::Capture(capture.image)).size(width, height).radius(5.0);
    let size = text(format!("{}×{}", capture.width, capture.height)).value().secondary();
    let island = row![text(done).callout(), size].spacing(6.0);
    let size = text(format!("{}×{}", capture.width, capture.height)).value().secondary();
    let notch = row![icon(Icon::Checkmark).size(10.0).tone(Tone::Green), size].spacing(4.0);
    Notice::new(thumbnail, adaptive(notch, island)).pop().size(NoticeSize::Large).duration(SHOWN_FOR)
}
```

A thumbnail of the capture, fitted to its aspect ratio, on the leading side. On an island the trailing side says "Copied" or "Saved" and the pixel size; beside a camera housing, where there's less room, it is just a green check. `SHOWN_FOR` is 5 seconds, long enough to reach the button. On an island the button is tucked into the end of the capsule; on a notched Mac it ends the right wing while the thumbnail and the check share the left (see [Surfaces](Surfaces.md#controls)). A press opens the [annotator](../Annotate.md).

### Tests

The tool's tests drive a `Runtime` directly: a capture is copied and shown; the notice carries the Annotate control and lasts 5 seconds; a `ui` press on the `notice` surface answers with `DismissNotice` and `Annotate { image }`; and the button is there for a capture that is only saved.

---

## Record Screen

A tile with two round audio buttons beside its shortcut: a microphone menu and a desktop-audio chip. Click it or press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>R</kbd>, choose an area, and it records until clicked again. While recording, the closed notch shows a red dot and a running timer. The finished video lands on the Shelf.

### Manifest and options

The recorder keeps its settings in an `Options` type (`options.rs`), which both declares them and reads them:

```rust
pub fn declare(manifest: Manifest) -> Manifest {
    manifest
        .setting(SettingSpec::toggle(MICROPHONE, "Record the microphone", false).hidden())
        .setting(SettingSpec::toggle(DESKTOP_AUDIO, "Record desktop audio", false).hidden())
        .setting(SettingSpec::text(MICROPHONE_ID, "Microphone", "").hidden())
        .setting(SettingSpec::text(FOLDER, "Recording folder", "").placeholder("Videos").hidden())
        .setting(
            SettingSpec::choice(FRAME_RATE, "Frame rate", [("auto", "Auto"), ("60", "60"), ("30", "30")], "auto")
                .detail("Auto records 60 fps, or 30 on very large areas"),
        )
        .setting(SettingSpec::toggle(DIM, "Dim around recordings", true).detail("Shade the screen outside the area"))
}
```

The microphone, desktop-audio and microphone-choice settings are hidden because the tile's buttons set them. Reading `frame_rate` with `settings.text(FRAME_RATE).parse().ok()` turns `"60"` and `"30"` into numbers and `"auto"` into `None`, which is what `RecordingOptions::max_fps` expects.

### State

```rust
pub struct ScreenRecorder {
    options: Options,
    state: RecordingState,
    inputs: Vec<AudioInput>,
}
```

The tool doesn't record anything itself. It asks notchbase to, and mirrors notchbase's `RecordingState`:

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    Subscription::batch([
        Subscription::shortcut(RECORD, Msg::Toggle),
        Subscription::recording(Msg::Recording),
        Subscription::audio_inputs(Msg::Inputs),
        Subscription::settings(Msg::Configured),
    ])
}
```

### One button, four meanings

```rust
fn toggle(&mut self) -> Command<Msg> {
    match self.state {
        RecordingState::Idle => Command::start_recording(self.options.recording()),
        RecordingState::Choosing => Command::cancel_capture(),
        RecordingState::Starting | RecordingState::Recording { .. } => Command::stop_recording(),
        RecordingState::Saving => Command::none(),
    }
}
```

### Following the recording

```rust
fn follow(&mut self, update: RecordingUpdate) -> Command<Msg> {
    match update {
        RecordingUpdate::State { state } => {
            self.state = state;
            Command::none()
        }
        RecordingUpdate::Saved { path, seconds, note } => {
            let shown = match note {
                Some(note) => Command::notify(note),
                None => Command::notice(saved(seconds)),
            };
            Command::shelve([path]).and(shown)
        }
        RecordingUpdate::Failed { error } => Command::notify(error),
    }
}
```

The saved file goes onto the Shelf. Normally a small "Saved 0:42" notice confirms it; when notchbase attaches a `note` to the result, the tool shows that instead.

### A tile per state

```rust
fn tile(&self) -> Option<Tile<Msg>> {
    let tile = Tile::new("Record Screen").shortcut(RECORD).on_press(Msg::Toggle);
    Some(match self.state {
        RecordingState::Idle | RecordingState::Choosing => self.idle_tile(tile),
        RecordingState::Starting => live_tile(tile, "Recording").footer(text("Starting…").subheadline().secondary()),
        RecordingState::Recording { since } => {
            live_tile(tile, "Recording").footer(timer(since).style(TextStyle::Display))
        }
        RecordingState::Saving => live_tile(tile, "Saving…").enabled(false),
    })
}

fn live_tile(tile: Tile<Msg>, title: &str) -> Tile<Msg> {
    tile.title(title).symbol(icon(Icon::Stop).size(STOP).tone(Tone::White)).well(Tone::Red).tint(Tone::Red)
}
```

When idle, the tile shows a red dot in its well and two round buttons in its footer, beside the shortcut, so it fits in half a row next to the Timer:

```rust
fn idle_tile(&self, tile: Tile<Msg>) -> Tile<Msg> {
    let desktop =
        chip("", self.options.desktop_audio).icons(Icon::Speaker, Icon::SpeakerMuted).on_toggle(Msg::DesktopAudio);
    let controls = row![self.microphone_menu(), desktop].spacing(CHIP_GAP);
    tile.symbol(dot(Tone::Red, WELL_DOT)).footer(controls)
}

fn microphone_menu(&self) -> Element<Msg> {
    let off = [(Icon::MicrophoneMuted, NO_MICROPHONE.to_owned())];
    let names: Vec<(Icon, String)> = if self.inputs.is_empty() {
        vec![(Icon::Microphone, DEFAULT_MICROPHONE.to_owned())]
    } else {
        self.inputs.iter().map(|input| (Icon::Microphone, input.name.clone())).collect()
    };
    let chosen = self.options.microphone.then(|| self.selected().map_or(1, |index| index + 1));
    dropdown(off.into_iter().chain(names), chosen).compact().on_select(Msg::ChooseMicrophone).into()
}
```

The microphone menu does two jobs at once: its first option, "No Microphone", turns the microphone off, and any other option turns it on and records from that input. `selected()` finds the saved microphone in the current list, falling back to the system default. Both buttons save their choices with `Command::save_setting`, updating the model at the same time.

### The live activity

```rust
fn live(&self) -> Option<Live<Msg>> {
    let RecordingState::Recording { since } = self.state else { return None };
    let stop = button("").icon(Icon::Stop).on_press(Msg::Toggle);
    Some(Live::new(dot(Tone::Red, DOT), timer(since)).controls(stop))
}
```

Only while recording. The timer counts on its own; the tool has no tick subscription at all. The stop button appears when the pointer rests on the activity, so a recording can be ended without opening the notch.

---

## Timer

A countdown that looks and feels like the timer on a phone. Its manifest has a page titled "Timer", which opens from the tile on the Tools tab like every tool page, a "Start or Pause Timer" shortcut (Ctrl+Alt+T), and one hidden `last_ms` setting that remembers the last length you started (5 minutes the first time).

The page is one row: a round Cancel button, the middle, and a round button on the right.

- **Set** (`Countdown::Idle`): the middle is a `picker` of three looping wheels, hours (0 to 23), minutes and seconds. Each landed wheel sends `Msg::Pick(wheel, option)`, which sets that part of the `Length`. Cancel is disabled, and a green Start is disabled while the length is zero.
- **Running**: the middle is a 128 point orange ring that `counting_down(start, end)`, with the time left in the `Hero` style and a bell beside `timer(end).clock()`, the time it will go off. The right button is an orange Pause. A one-second tick checks for the end.
- **Paused**: the ring stops and turns gray at the share left, with "Paused" under the time, and the right button is a green Resume. Resuming moves `start` and `end` together so the ring carries on from where it stopped.

When the time is up the timer goes back to set, sends `Command::chime()`, and shows a sticky, shaking notice with the length that finished. The tile always opens the page, so a started timer can be paused, resumed or cancelled there; while running it shows the time left in an orange tile, and while paused "Paused · 8:55". The live activity is a 13 point ring that drains with the countdown beside the time left. The shortcut starts, pauses and resumes. `Timer<C: Clock>` takes its clock as a type, like Focus, so the tests in `tools/timer/src/tests.rs` drive it with a fake one.

## Alerts

Alerts turns a small webhook on your own computer into notch notifications: anything that can make an HTTP request can make the notch shake and show a title, a message and a colored badge. [Alerts](../Alerts.md) documents the webhook; from the SDK's point of view it's worth reading for a few patterns:

- **A server in the background.** `init` starts a local HTTP listener with `Command::spawn`, which runs for as long as the tool does and sends each request in as a message. A generation number in every message lets the tool ignore messages from a listener it has already replaced after a settings change.
- **Settings that restart work.** When `Subscription::settings` delivers a different port, token or on/off switch, the tool stops the old listener and starts a new one.
- **Notices from outside.** Each alert becomes a banner `Notice` (see [Banners](Surfaces.md#banners)), and callers can make it shake or stay until dismissed; a later request can take it down again with `Command::dismiss_notice()`.
- **Nothing on screen of its own.** It has no tile and no page. `tile` and `page` default to `None`, so a tool that only reacts to the outside world is a manifest, `update` and a subscription.

## The Focus plugin

`tools/focus-timer` is the reference **plugin**: a Pomodoro-style timer with a page of presets, a live countdown ring in the closed notch, notices when a session or break ends, and a session count kept in a `data` setting. It isn't bundled; build it and install it like any plugin (see [Plugins and Distribution](Plugins-and-Distribution.md#the-reference-plugin)). Its tests use an injectable clock (see [Testing](Testing.md#controlling-time)).

For a complete tool written for this wiki, see [Example: Snippets](Example-Snippets.md).
