# Building a notchbase tool

Read this whole file before building or changing a tool.

You turn an idea ("show my next meeting in the notch", "a tile that pings my server") into a working notchbase plugin: a small Rust crate built as a dynamic library and dropped into the user's tools folder, where notchbase loads it within a second. It compiles against the notchbase SDK, which the user gets as `notchbase-sdk.zip` (the SDK kit) alongside this skill. The person you're helping may not know Rust or the SDK. Don't teach them; build it, test it, install it, and tell them where to click.

## How notchbase tools work (the whole model in one screen)

A tool is Elm-style: a **model** (your struct), **messages** (an enum), an **update** that changes the model and returns a **Command** (things for notchbase to do), and **views** that are pure functions of the model.

| Piece | What it is | You write |
| --- | --- | --- |
| `manifest()` | id, name, page title, shortcuts, settings | once |
| `init(&Context)` | builds the model from saved settings | once |
| `update(&mut self, Msg) -> Command<Msg>` | handles every message | the logic |
| `subscriptions(&self)` | what outside events to hear: timers, shortcuts, settings, media, volume, bluetooth, the battery, the screen lock, camera and microphone use, browser downloads, notch state, Apple Calendar events | as needed |
| `tile(&self)` | a card on the Tools tab | optional |
| `page(&self)` | a full view inside the open notch, opened from the Tools tab (or from its own header tab) | optional (needs `.page(title)` in the manifest) |
| `live(&self)` | a live activity in the closed notch: small leading + trailing views | optional |

Notices (the little pop-outs the notch shows) are sent with `Command::notice(...)`, or with `Command::notice_with_controls(notice, button)` when the notice should offer one thing to press. Settings declared in the manifest appear automatically on the tool's own settings page (the gear › **Tools** › the cog beside the tool) and are saved to `config.json`. Toggles, choices, numbers, folders and text fields (including masked secrets) are all editable there, so a tool rarely needs its own settings UI.

By default everything a tool shows in the open notch lives on the **Tools** tab: its tile, and its page, which opens when the tile is clicked (if the tile has no `on_press` of its own), from `Command::open_page()`, or from the tile notchbase makes automatically for a tool that has a page but no tile. A tool whose page people will open all day (like the built-in Calendar) can ask for its own tab in the header with `.header_tab()` in the manifest (at the end, or wherever the user lists its id in `layout.tabs` in `config.json`); it then gets no automatic launcher tile. Use it sparingly: the header shrinks titles and icons to fit, but every tab costs room. Global shortcuts work anywhere, open or closed.

Views describe *what* to show; notchbase draws everything (glass, fonts, animation, colors), so tools always look native. You choose from fixed text styles, tones, and icons (lists below). When that is not enough, a `canvas` lets a tool draw anything itself (see "Free drawing"); reach for it only when the ready-made elements can't do the job, because they are what keeps a tool looking native.

## Workflow

Follow these steps in order. Keep moving; ask the user only what you truly can't decide.

### 1. Pin down the idea (one short exchange at most)

Decide, choosing sensible defaults yourself:
- **What it shows and where**: tile (glanceable + a button or two), page (lists, inputs, anything bigger; opens from the tile), live activity (something ongoing: a countdown, a status), notices (brief feedback). A launcher-style tool (buttons that open apps or sites) is a page of buttons, or a tile per action. Only give a page a header tab (`.header_tab()`) if it's something people check constantly.
- **What triggers it**: a click, a global shortcut (`Ctrl+Alt+<letter>` is a good default), a timer, or an outside event.
- **What it remembers** (a `data` setting) and **what the user can change** (visible settings: they get a page of their own in Settings, so prefer settings to building your own options UI). Keys, tokens and private links are `text` settings with `.secret()`.
- **The id**: short, lowercase, hyphenated, unique (`meeting-clock`). It becomes the crate name and names the library.

Most ideas are one of a few shapes. Find the nearest and start there:

| The idea | Build it from |
| --- | --- |
| Show something from a website or an API (weather, a server's status, a price) | `Command::fetch` from `init` and on a `Subscription::every` tick, the answer kept in the model, a tile for the glance and a page for the rest; the address or key is a `text` setting (`.secret()` for keys) |
| A countdown, stopwatch or reminder | `timer(end_ms)` and `ring(…).counting_down(start_ms, end_ms)` in a live activity, `Subscription::every` only while it runs, a notice and `Command::chime()` when it ends |
| A launcher for apps, folders or sites | A page of buttons that each send `Command::open(address or path)` |
| A list people add to (notes, to-dos, snippets) | The template as it is: a text input and a `list`, saved in a `data` setting |
| React to the computer (battery, screen lock, Bluetooth, downloads, music, camera and microphone, Focus) | The matching subscription, answered with a notice or a live activity; `.background()` in the manifest when it shows nothing else |
| A status light in the closed notch | `live()` with a dot or icon and a few characters, `None` while there is nothing to say |
| Something on a schedule (every hour, at 9:00) | `Subscription::every` and a comparison with `now_ms()`; it only runs while notchbase does |
| A chart, a gauge, a game, anything hand-drawn | `canvas` (see "Free drawing") |
| The next meeting, today's events | `Subscription::calendar` (Apple Calendar, macOS only), or `fetch` an iCal link and read it yourself |
| Run a script or another program, watch a folder, talk to a device | A native build (see "Portable or native") with the work inside `Command::perform` or `Command::spawn` |

If something needs a capability the SDK doesn't have (see "Limits"), say so plainly and offer the nearest thing it can do.

### 2. Check the machine and find the SDK kit

The SDK kit is a folder named `notchbase-tools` (from `notchbase-sdk.zip`) that holds `notchbase-sdk/`, an example `notes/` tool, `docs/` and the `install.ps1` / `install.sh` scripts. Every tool is created **inside** it, as `notchbase-tools/<id>/`, next to `notchbase-sdk/`.

If you can run commands:
- Look for `notchbase-sdk/Cargo.toml` inside `~/notchbase-tools`, `~/Documents/notchbase-tools`, `~/Desktop/notchbase-tools` and `~/Downloads/notchbase-tools` (on Windows `~` is `%USERPROFILE%`). If it isn't there, ask the user where they unzipped `notchbase-sdk.zip`, or ask for the zip and unzip it into their home folder so it becomes `~/notchbase-tools`. If they have a notchbase checkout instead, use `<checkout>/sdk` as the SDK path.
- `cargo --version`. It has to be 1.88 or newer (`rustup update` if it is older). If Rust is missing, ask before installing, then:
  - Windows: `winget install --id Rustlang.Rustup -e`, then open a new terminal. Rust on Windows also needs the C++ build tools: `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`.
  - macOS: `xcode-select --install` (if `clang` is missing), then `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y` and `source "$HOME/.cargo/env"`.
- Portable builds need the WebAssembly target once: `rustup target add wasm32-unknown-unknown` (the install script adds it by itself when it can).
- Create the crate as `<kit>/<id>`, never inside notchbase's install folder.

If you can't run commands (a chat without a terminal, like ChatGPT): write every file in full with its path relative to the kit (`<id>/Cargo.toml`, `<id>/src/lib.rs`, `<id>/src/tests.rs`), tell the user to save them inside their unzipped `notchbase-tools` folder, then give the exact commands to run from that folder, one per code block (`cargo test --manifest-path <id>/Cargo.toml`, then the install command from step 6), and ask them to paste back any error. If they don't have the kit yet, tell them to unzip `notchbase-sdk.zip` first (into their home folder is best).

### 3. Scaffold from the templates

Copy `templates/Cargo.toml`, `templates/lib.rs` and `templates/tests.rs` from this skill into `<id>/Cargo.toml`, `<id>/src/lib.rs`, `<id>/src/tests.rs`, and replace every `__ID__` (the hyphenated id), `__NAME__` (the display name) and `__TYPE__` (the tool's type name in UpperCamelCase). Then reshape the code to the idea. The template compiles as-is and already has a tile, a page, a setting, a shortcut and tests, so start from it rather than from nothing.

The template's SDK dependency is `notchbase-sdk = { path = "../notchbase-sdk" }`, which is right for a crate inside the kit. For a notchbase checkout use `{ path = "<checkout>/sdk" }` instead. Keep the empty `[workspace]` table: it stops Cargo from mistaking an enclosing folder for the tool's workspace.

### 4. Write the tool

Read `references/tools/Elm-Architecture.md` once if anything about the model is unclear, and look things up in the other references as you go (index at the bottom). Follow the rules in "Rules that keep it compiling" below; they cover almost every compile error people hit.

### 5. Test it

Write tests with `notchbase_sdk::testing::Harness` (see `templates/tests.rs` and `references/tools/Testing.md`): start the tool, type into fields by placeholder, click by label or icon, pick a day in a month with `pick_day` or `pick_date` or tap a week with `tap_week_day` / `tap_week_event`, pick a color with `pick_color`, press a button on the last notice with `click_notice(label)`, drag or click a pan with `pan` / `tap_pan`, send a canvas a pointer with `pointer(n, Pointer::at(PointerKind::Click, x, y))`, scroll a week or month with `scroll_calendar`, press shortcuts, send notch, media or calendar events, and assert on the model, on what's shown and on the requests. Answer a `fetch`, `read_file`, `write_file`, `pick_color` or `capture_screen` with `answer(Reply::…)`, and let `Command::perform` or `spawn` finish with `wait_for(|model| …)`. `shows(text)` looks at text elements, the labels of buttons and chips, a week's event titles and canvas text; for a tile's title, a checkbox's label, segmented or dropdown options and what is typed in a field, look at `tile()`, `page()` or the model instead (the "Testing" line in the cheat sheet has every method). Run `cargo test`. Fix until green. Don't skip this: it's how you know the tool works without seeing the screen. When a build or a test fails, "When something goes wrong" below says what each message means.

### 6. Build and install

Run the installer from the kit folder, naming the tool:
- Windows: `powershell -ExecutionPolicy Bypass -File install.ps1 <id>`
- macOS: `sh install.sh <id>`

(The same scripts ship with this skill in `scripts/`; run from the crate folder they need no argument.)

By default it builds a **portable** tool: one `<crate>.wasm` file that runs on every notchbase, Windows or Mac, Intel or Apple Silicon. Add `--native` (`sh install.sh --native <id>`, `install.ps1 <id> -Native`) for a native library instead, when the tool needs something a portable tool can't do (see "Portable or native"). It copies the result into the user's tools folder (`%APPDATA%\notchbase\tools\<id>\` or `~/Library/Application Support/notchbase/tools/<id>/`), replacing the other kind if it was there. notchbase notices within about a second and shows "Reloaded <id>". No restart is needed, and rebuilding and re-running the script updates the tool in place.

`--native` may come before or after the tool's name. If the script stops, its last lines say why, and nothing is installed until the build passes. If notchbase instead says "it needs a newer notchbase", the user's notchbase is older than the SDK: have them update notchbase (Settings, or Settings › Notifications). Anything else that isn't "Reloaded <id>" is in "When something goes wrong" below.

### 7. Hand it over

Tell the user, briefly: where it appears (its tile on the Tools tab or its header tab, the page, the closed notch), the shortcut, the settings it added (gear › Tools › the cog beside the tool), and how to remove it (delete its folder in the tools folder). To share a portable tool with someone else, they send the one file `<id>/target/wasm32-unknown-unknown/release/<crate>.wasm`, which works on any computer; the other person puts it in `tools/<id>/` inside their own tools folder. A native tool is shared the same way but only works on the same kind of computer it was built on (`<crate>.dll` on Windows, `lib<crate>.dylib` on macOS). Offer one or two natural next steps.

## When something goes wrong

Read the first error, not the last: later ones usually follow from it. Fix the cause, run the same command again, and only involve the user when they have to do something (install Rust, update notchbase). Don't hand over a tool that hasn't passed its tests and been installed.

**Building and testing**

| It says | Cause | Do this |
| --- | --- | --- |
| `cargo: command not found`, or the script's "Rust isn't installed (there is no cargo)" | Rust is missing, or this terminal was opened before it was installed | Install it (step 2) and open a new terminal |
| `linker 'link.exe' not found` (Windows) | The C++ build tools are missing | The Build Tools command in step 2, then a new terminal |
| `xcrun: error: invalid active developer path` (macOS) | Apple's command line tools are missing | `xcode-select --install` |
| "the `wasm32-unknown-unknown` target may not be installed", `can't find crate for core` | The WebAssembly target is missing | `rustup target add wasm32-unknown-unknown`. No `rustup` means Rust came from a package manager: install it with rustup instead (step 2) |
| "failed to load manifest for dependency `notchbase-sdk`", "failed to read …/notchbase-sdk/Cargo.toml" | The crate isn't beside `notchbase-sdk` | Move its folder into the kit, or correct the `path` in `Cargo.toml` (step 3) |
| "current package believes it's in a workspace when it's not" | The empty `[workspace]` table is gone | Put it back at the end of `Cargo.toml` |
| An error inside a crate the tool depends on, only in the portable build | That crate can't be built for WebAssembly | Use a command instead (`fetch`, `read_file`, `write_file`) or a pure-Rust crate, or build native (see "Portable or native") |
| `no variant … named … found for enum` `Icon`, `Tone` or `TextStyle` | A name that isn't in the lists below | Pick one that is |
| `no associated function … named … found for struct` (`Command`, `Subscription`, …) or `no method named …`, for something this file lists | The kit's `notchbase-sdk/` is older than this skill | Have them unzip the current `notchbase-sdk.zip` over the kit, keeping their own tool folders |
| `the trait bound … is not satisfied`, `expected an Fn(…) closure, found Msg` or `expected … to return Element<_>` on `.on_press(…)`, `.on_change(…)`, `list(…)` or a notice | The handler or element has the wrong shape | Rules 6, 7 and 8 |
| `the trait bound …: Eq is not satisfied` on the message enum | A variant carries something without `Eq` | Rule 3: derive `PartialEq` only |
| "cannot find module or crate `serde`" or "`serde_json`" | `Cargo.toml` lacks them | Add `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` under `[dependencies]` (rule 9) |
| "rustc … is not supported by the following package", or a syntax error inside `notchbase-sdk` | Rust is older than 1.88 | `rustup update` |
| A test panics with "no control …; these can be clicked: …" or "no text field with the placeholder …" | The harness couldn't find what the test asked for, and lists what is on screen | Match the label or placeholder exactly, or put the tool in the state that shows it first |

**Installing and loading**

| What happens | Cause | Do this |
| --- | --- | --- |
| No "Reloaded <id>" and the tool isn't on the Tools tab | notchbase isn't running, the tool is switched off, or the file went somewhere else | Start notchbase. **Settings › Tools** lists every tool it found with the path of its file and a switch, has **Reload tools**, and shows where the tools folder is: `tools/<id>/` there must hold the `.wasm` (or the library) |
| "Couldn't load the tool at …: it needs a newer notchbase" | The kit is newer than the app | Have them update notchbase |
| "…: it doesn't export …; end the tool with notchbase_sdk::export!" | `export!` is missing, or the built file was renamed | Rule 2. Never rename the file Cargo built |
| "…: it isn't a WebAssembly tool", or a system error about the library | The file is damaged, half copied or built for another kind of computer | Run the install script again; build portable to share across computers |
| "…: it was built for tool ABI …" | The kit and the app are generations apart | Build with the kit that came with their notchbase |

A tool that couldn't load stays listed in **Settings › Tools** with the reason in yellow under it.

**Running**

| What happens | Cause | Do this |
| --- | --- | --- |
| The tool's page says "This tool stopped working", or its tile or live activity disappears | It panicked. notchbase stopped that tool only | Read the reason in yellow under the tool in **Settings › Tools**: it is the panic message, and for a portable tool also the file and line ("it crashed: index out of bounds: the len is 0 but the index is 3 (src/lib.rs:44)"). Fix it, add a test that would have caught it, install again: the tool restarts by itself |
| "it crashed: time not implemented on this platform" | `Instant::now()` or `SystemTime::now()` in a portable tool | `now_ms()` |
| "it crashed: … operation not supported on this platform" | `std::fs`, `std::net`, `std::process` or `std::thread` in a portable tool | The matching command (rule 14), or build native |
| "it crashed: wasm `unreachable` instruction executed" and nothing more | The tool was built with an older kit, which couldn't say why | Rebuild with the current kit, then read the reason |
| "it took too long to answer and was stopped" | One `update` or view did far too much work, or looped forever | Do less per message: spread the work over `Subscription::every` ticks, or build native and use `Command::perform` |
| A setting changed in Settings doesn't reach the tool | The tool only read it in `init` | `Subscription::settings(Msg::Configured)` and read it again in `update` |
| A shortcut does nothing | Something else has that combination, it was unbound, or there is no `Subscription::shortcut` for it | Show it with `Tile::shortcut(id)` or `shortcut(id)` so it can be rebound by clicking; the field says why a combination can't be used |
| Nothing comes back from `fetch`, `read_file` or `write_file` | The message got `Err(text)` and `update` dropped it | Handle the `Err` arm: keep the text in the model and show it, or `Command::notify` it |
| `Command::log` lines are nowhere to be found | They go to notchbase's standard error, which is only visible when notchbase was started from a terminal | While developing, rely on tests and on a temporary `Command::notify` instead |

## Rules that keep it compiling

1. Start `lib.rs` with `use notchbase_sdk::prelude::*;`. If you use the `column!` macro, also `use notchbase_sdk::column;` (Rust's standard library has its own `column!`, so the prelude can't export it). `row!` comes with the prelude.
2. `Cargo.toml` needs `[lib] crate-type = ["cdylib", "rlib"]`, and `lib.rs` must end with `notchbase_sdk::export!(YourTool);` exactly once.
3. The message enum: `#[derive(Clone, Debug, PartialEq)] pub enum Msg { … }`. Don't derive `Eq`: `Settings`, `NotchState`, `NowPlaying`, `VolumeLevel`, `CalendarFeed`, `RecordingUpdate`, `Pointer`, `f32` and `f64` don't implement it, so a message that carries one won't compile with it.
4. `update` must never block. Anything slow (network, disk, heavy work) goes in `Command::perform(move || work(), Msg::Done)`; the closure must own its data (`clone` before `move`) and be `Send`.
5. Views (`tile`, `page`, `live`) are called often and must be quick and side-effect free. Compute in `update`, store in the model, render in views.
6. Handlers take messages or closures: `.on_press(Msg::Save)`, `.on_toggle(Msg::SetLoud)` (a `fn(bool) -> Msg` like a tuple variant works), `.on_change(Msg::Draft)`, `.on_select(Msg::Pick)`. For a closure capturing a value: `.on_toggle(move |_| Msg::Pin(index))`.
7. `list(...)` takes elements: map to `Element<Msg>` with `.into()` or wrap rows with `keyed(key, row![…])`. Keys must be unique.
8. Notices take `Element<()>`: build them from `text`, `icon`, `row!` etc. with no handlers. The one place a notice takes a handler is the second argument of `Command::notice_with_controls(notice, button("Label").on_press(Msg::Go))`; a press doesn't dismiss the notice, so answer it with `Command::dismiss_notice().and(…)`.
9. Persist state with a hidden data setting: `.setting(SettingSpec::data("items", Value::Array(Vec::new())))`, read with `settings.parse("items").unwrap_or_default()`, save with `Command::save_setting("items", self.items.clone())`. Anything that is `serde::Serialize` can be saved after `serde_json::to_value`, and `settings.parse::<T>(key)` or `response.json::<T>()` read it back: derive `serde::Serialize, serde::Deserialize` on your own types. The template's `Cargo.toml` already depends on `serde` and `serde_json` for this; a `Cargo.toml` without them fails with "cannot find module or crate `serde_json`".
10. Read settings defensively (`clamp` numbers); people edit `config.json` by hand.
11. Times are Unix milliseconds: `now_ms()`. For a live countdown, show `timer(end_ms).remaining()`, which updates by itself with no ticks. Subscribe to `Subscription::every(...)` only while something is running.
12. Use only the icons and tones listed below. Any other name won't compile.
13. Never use `panic = "abort"`, `unwrap()` on user data, or `std::process::exit`: a panic stops only your tool, but keep it from happening. Index with `.get(i)`, parse with `.ok()` or `unwrap_or_default()`, and turn every `Err` into something the view can show.
14. For HTTP use `Command::fetch(HttpRequest::get(url).header("Accept", "application/json"), Msg::Loaded)`; the message gets `Result<HttpResponse, String>` with `.status`, `.body`, `.ok()` and `.json::<T>()`. Read and write text files with `Command::read_file(path, Msg::Read)` and `Command::write_file(path, text, Msg::Written)`: a plain name lands in the tool's own data folder, `~/…` is the user's home, an absolute path is used as is. These work in portable and native tools alike, so prefer them to a crate.
15. A setting that only matters for one choice uses `.shown_when_is("choice_key", "value")`; one that follows a toggle uses `.shown_when("toggle_key")`. When setup is needed, offer a button that sends `Command::open_settings()` rather than explaining where Settings is.

## API cheat sheet

Everything below comes from `notchbase_sdk::prelude::*`.

**Manifest**: `Manifest::new(ID, "Name").description("…").version(env!("CARGO_PKG_VERSION")).page("Page title").page_icon(Icon::Star).header_tab().shortcut("id", "Title", Some("Ctrl+Alt+K")).setting(spec)`. `page_icon` is used on the tile notchbase makes for a page-only tool and on a header tab. `.header_tab()` gives the page its own tab in the header (optional). `.background()` is for a tool with no tile and no page that only shows notices or a live activity: it is then listed under **Settings › Indicators** instead of **Settings › Tools**. Don't call `no_tab()`: it's deprecated and does nothing.

**Settings** (`SettingSpec`): `toggle(key, title, default)`, `choice(key, title, [("value", "Label"), …], "default")` (up to 4 options show as a segmented control), `number(key, title, [min, max], step, default).unit("min")`, `folder(key, title).placeholder("Documents/Notes")` (a folder picker row), `text(key, title, default).placeholder("…")` (a text field that saves when editing ends), `data(key, Value)` (hidden state). Builders: `.detail("…")`, `.secret()` (text shown as dots: keys, tokens, private links), `.shown_when("toggle_key")`, `.shown_when_is("choice_key", "value")`, `.hidden()`. Read: `settings.bool/number/text/string/path/parse::<T>/get(key)`.

**Context** (in `init`): `context.settings()`, `context.data_dir()` (a private folder for files), `context.dirs()` (`home`, `pictures`, `videos`, `desktop`, `downloads`, `documents` as `Option<String>`), `context.platform()` (`Platform::Windows`, `MacOs`, `Other`), `context.clock_24h()` (for time labels you format yourself; `timer(at).clock()` already follows it), `context.time_zone()` (the user's IANA time zone such as `Europe/Paris`, if known), `context.host_version()` (notchbase's version).

**Commands** (`Command::…`): `none()`, `batch([…])`, `a.and(b)`, `message(msg)`, `.map(Msg::Child)` (rewrap a child's command), `perform(task, map)`, `spawn(|emitter| …)` (a thread that can `emitter.send(msg)` many times), `notify("text")`, `notice(Notice)`, `notice_with_controls(Notice, element)` (a notice with a button that sends a message when pressed), `dismiss_notice()`, `copy_text(s)`, `copy_image(id)`, `annotate(id)` (open a captured image in notchbase's annotator window to draw on; each change is copied to the clipboard), `pick_color(Msg::Picked)`, `capture_screen(Msg::Captured)`, `save_image(id, folder, prefix, ImageFormat::Png, Msg::Saved)` (`folder` is `Option<&Path>`, so pass a model's `Option<PathBuf>` as `folder.as_deref()`; `None` is notchbase's own folder; `ImageFormat::Jpeg` exists too), `cancel_capture()` (stop a pick or capture in progress), `start_recording(RecordingOptions)`, `stop_recording()`, `save_setting(key, value)`, `fetch(HttpRequest::get(url), Msg::Loaded)` (also `HttpRequest::post(url, body)`, `.header(name, value)`, `.json(&value)`, `.timeout(duration)`), `read_file(path, Msg::Read)`, `write_file(path, text, Msg::Written)`, `shelve(paths)`, `reveal(path)`, `open("https://… or a path")`, `open_page()`, `open_settings()` (your tool's settings page), `collapse()`, `hold_open()`, `release()`, `media(MediaControl::PlayPause | Next | Previous)`, `chime()`, `keep_awake(bool)` (keep the computer and display awake until you send `false`), `mute_microphone(bool)`, `log("…")`, and on macOS `save_event(EventDraft { id, calendar_id, title, start, end, url: Some(link), ..EventDraft::default() })` / `delete_event(id)` to write to Apple Calendar.

**Subscriptions** (`Subscription::…`): `every(Duration, msg)`, `shortcut("id", msg)`, `settings(Msg::Configured)`, `media(Msg::Song)` (`Option<NowPlaying { title, artist, playing, position, duration }>`), `volume(Msg::Volume)` (`VolumeLevel { volume, muted }`), `bluetooth(Msg::Device)`, `power(Msg::Power)` (`Power { level: Option<u8>, plugged, charging }`), `screen_lock(Msg::Locked)` (`bool`), `sensors(Msg::Sensors)` (`Sensors { camera, microphone, apps, muted }`), `focus(Msg::Focus)` (`Focus { active, name: Option<String> }`; on a Mac only with Full Disk Access), `downloads(Msg::Download)` (`DownloadChange::Progress { download } | Finished { download, path } | Cancelled { download }`, where `Download` has `id`, `name`, `received`, `total: Option<u64>` and `share()`), `recording(Msg::Rec)`, `audio_inputs(Msg::Mics)`, `notch(Msg::Notch)`, `calendar(Msg::Calendar)` or `calendar_between(from_ms, to_ms, Msg::Calendar)` (Apple Calendar on macOS: `CalendarFeed::Events { events, calendars } | Waiting | Denied | Unavailable`, where each `CalendarEvent` has `id`, `title`, `start`, `end`, `all_day`, `location`, `notes`, `url`, `calendar`, `calendar_id`, `color`, `declined`, `editable`; on Windows it's always `Unavailable`). Combine with `Subscription::batch([...])` or `.and(...)`; `Subscription::none()` listens to nothing, and `.map(Msg::Child)` rewraps a child's subscription.

**What the messages carry**: `recording` gives `RecordingUpdate::State { state } | Saved { path, seconds, note } | Failed { error }` (`RecordingState::Idle | Choosing | Starting | Recording { since } | Saving`); `audio_inputs` gives `Vec<AudioInput { id, name, is_default }>`; `bluetooth` gives `DeviceChange::Connected { device } | Disconnected { device } | Updated { device }` with `Device { id, name, kind, battery: Option<u8> }`; `capture_screen` gives `Option<Capture { image, width, height, on_clipboard }>` (show it with `image(ImageSource::Capture(capture.image))`); `pick_color` gives `Option<Color>`; `save_image`, `read_file` and `write_file` give `Result<String, String>`; `calendars` in a calendar feed are `CalendarInfo { id, title, account, color, writable }`.

**Notch state** (`NotchState`): `open`, `page`, `phase` (`NotchPhase::Resting | Peeking | Open | Notice | Dropping | Asking`), `style` (`NotchStyle::Notch | Island`), `expansion` 0–1 (how open, including halfway), `hug` 0–1 (docked flat against the top), `float` 0–1 (island tab → capsule), `width`, `height`, `notch` (camera housing size), `held`, `settled`, `activities`, `music`; helpers `hugged()`, `floating()`, `opening()`, `fully_open()`, `resting()`.

**Surfaces**:
- `Tile::new("Title").icon(Icon::Bell).on_press(msg).shortcut("id").accessory(element).footer(element).well(Tone::Orange).tint(Tone::Orange).enabled(bool).symbol(element)` (the Tools tab shows each tool as its icon and title in a launcher grid, with the whole tile in a card that opens underneath for the icon the pointer lingers on; `tint` adds a ring and a running row, `wide()` no longer changes anything)
- `Live::new(leading, trailing).controls(element)`: keep both tiny (an icon, a ring, a dot / a short text or `timer(..)`). `controls` (one or two `button("").icon(Icon::Pause).on_press(…)`) appear and respond when someone hovers the activity's ear, so a timer can be paused or a recording stopped without opening the notch. `.urgent()` puts it in the middle ahead of every other activity and a new song while it lasts, for something the user is doing right now (Dictation uses it).
- `Notice::new(leading, trailing).below(element).duration(Duration).pop().shake().banner().sticky().size(NoticeSize::Small | Medium | Large)`. Its elements don't respond to clicks. Sent with `Command::notice_with_controls(notice, controls)` it also gets `controls` (one short `button("Label").on_press(msg)`, two at most): tucked into the end of the capsule on an island, in a row under the content beside a camera housing or under `below` or a banner. The notice stays while the pointer is on the controls (or within 6 points of them; resting elsewhere on the notice does not keep it), and `Command::dismiss_notice()` ends it at once, so give it a few seconds (`.duration(Duration::from_secs(5))`) for the pointer to get there. Controls need notchbase's tool protocol 5: a tool built with this SDK doesn't load in an older notchbase.

**Elements**: `text("…")` (+ `.title() .headline() .callout() .subheadline() .section() .caption() .value() .display() .hero() .secondary() .tertiary() .tone(Tone) .wrap() .align(Align)`), `icon(Icon).size(f32).tone(Tone)`, `button("Label").icon(Icon).filled()/.plain()/.destructive()/.tinted(color).enabled(bool).on_press(msg)`, `round_button("Go").tone(Tone).size(f32).on_press(msg)`, `toggle(on).on_toggle(f)`, `checkbox("Label", on).on_toggle(f)`, `chip("Label", on).icon(Icon).on_toggle(f)`, `segmented(["A", "B"], index).on_select(f)`, `dropdown(["A", "B"], Some(index)).placeholder("…").compact().on_select(f)`, `slider(value).range(min, max).step(s).icon(Icon).on_change(f)`, `text_input(&value).placeholder("…").on_change(f).on_submit(msg)`, `picker([wheel(options, index).unit("min").looping()]).on_change(|column, index| …)`, `progress(0.5).tone(Tone)`, `month(2026, 9).today(30).selected(day).mark(day, color).monday_first(bool).on_select(|year, month, day| …).on_scroll(|step| …)` (a full-width month calendar with event dots; neighbouring months' days can be picked too, so turn to the reported month), `week(days).on_day(|year, month, day| …).on_event(|id| …).on_scroll(|step| …)` (one column per `WeekDay { year, month, day, weekday, today, selected, events: Vec<WeekEvent { id, time, title, color }> }`, events stacked as tinted blocks), `ring(0.5).size(40.0).stroke(4.0).tone(Tone).center(element).counting_down(start_ms, end_ms)`, `timer(at_ms)` (`.remaining()` / `.clock()`, counts up by default), `image(ImageSource::File(path)).size(w, h).radius(r)`, `swatch(color).size(f32)`, `palette([Tone::Red, Tone::Blue]).selected(Some(index)).on_select(|index| …)` (a row of color circles to pick from), `dot(color, size)`, `badge(Icon, Tone, size)`, `keycaps(["Ctrl", "K"])`, `shortcut("id")` (rebindable keycaps), `spacer()`, `space(8.0)`, `divider()`.
Layout: `row![a, b].spacing(8.0).align(Align::Center)`, `column![a, b].spacing(10.0)`, `card(element).padding(12.0)`, `list(elements)`, `keyed("key", element)`, `adaptive(notch_version, island_version)`, `pan(fixed_width_row).offset(points).on_pan(f).on_tap(f)` (a clipped strip you can drag, swipe and click; see Elements), and `.width(w)`, `.height(h)`, `.frame(w, h)` on anything.

**Free drawing**: `canvas(width, height).push(shape)….stretch().on_pointer(Msg::Pointer).on_scroll(Msg::Scrolled)`. Coordinates are points from the canvas's top left; `.stretch()` scales the drawing to the page width (design for 340). Shapes (`draw::…`, in the prelude): `rect(x, y, w, h)`, `circle(x, y, r)`, `ellipse(x, y, rx, ry)`, `line(points)`, `polygon(points)`, `text("…", x, y)`, `icon(Icon, x, y)`, `run([texts and icons], x, y)` (lays parts of different sizes side by side, since a tool can't measure text; `.anchor`, `.gap`), `image(source, x, y, w, h)`, `glass(x, y, w, h)`, `shadow(x, y, w, h)`, `group(shapes)`. Refine with `.fill(color or draw::linear(from, to, angle) or draw::radial(inner, outer))`, `.stroke(width, color)`, `.radius(r)`, `.size(s)`, `.color(c)`, `.semibold()`, `.mono()`, `.anchor(Anchor::Center)`, `.centered()`, `.wrap(width)`, `.blur(b)`, and on anything `.opacity(a)`, `.offset(x, y)`, `.scale(s)`, `.clip(x, y, w, h)`. Curves: `draw::arc(x, y, r, from_deg, to_deg)` and `draw::curve(from, pull, push, to)` return points for `line`/`polygon`. `Pointer { kind, x, y, dx, dy }` with `PointerKind::{Hover, Leave, Press, Drag, Release, Click, Scroll}`; hit test with `pointer.within(x, y, w, h)` / `pointer.near(x, y, r)`. Animate by ticking `Subscription::every(Duration::from_millis(16), Msg::Frame)` only while something moves.

**Testing** (`notchbase_sdk::testing::Harness`; every action returns the requests it caused): `Harness::<Tool>::start()`, `start_with([(key, value)])`; read `model()`, `settings()`, `requests()`, `saved(key)`, `wants()`, `texts()`, `shows(text)`, `noticed()`, `tile()`, `page()`, `live()`; act with `send(msg)`, `click(label)`, `click_nth(label, n)`, `click_icon(Icon, n)` (the nth button or chip carrying that icon), `press_tile()`, `click_notice(label)`, `type_text(placeholder, text)`, `submit(placeholder)`, `set_toggle(n, on)`, `select(option)` (segmented or dropdown), `slide(n, value)`, `pan(n, by)`, `tap_pan(n, at)`, `pointer(n, Pointer)`, `pick_day(day)`, `pick_date(year, month, day)`, `pick_color(index)`, `scroll_calendar(steps)`, `tap_week_day(year, month, day)`, `tap_week_event(id)`; send what notchbase would with `shortcut(id)`, `tick(Duration)`, `change_settings([(key, value)])`, `notch(NotchState)`, `media(Option<NowPlaying>)`, `volume(level, muted)`, `calendar(CalendarFeed)`, `power(Power)`, `screen_lock(bool)`, `sensors(Sensors)`, `download(DownloadChange)`, `focus(Focus)`, `event(Event)`; answer the latest question with `answer(Reply::Fetched { result: Ok(HttpResponse { status: 200, body }) })` (also `Reply::Read { result }` for `read_file`, `Reply::Saved { result }` for `write_file` and `save_image`, `Reply::Color { color }`, `Reply::Capture { capture }`; `Reply` and `Event` are in `notchbase_sdk::protocol`); `wait_for(|model| done)` lets background work finish (up to 5 seconds).

**Text styles**: `Title`, `Headline`, `Body`, `Callout`, `Subheadline`, `Section`, `Caption`, `Value`, `Display`, `Hero`.

**Tones**: `Primary`, `Secondary`, `Tertiary`, `Quaternary`, `White`, `Blue`, `Green`, `Orange`, `Red`, `Purple`, `Teal`, `Yellow`, `Pink`, `Gray`. They're tuned to sit on the notch's black; prefer them over raw colors. notchbase's palette is deliberately small, so several names are one color: `White`, `Blue` and `Purple` are the same off-white, `Green` and `Teal` the same green, `Orange` and `Yellow` the same yellow, `Red` and `Pink` the same red. To tell things apart by color, use one from each group plus `Gray`, or raw colors.

**Colors**: wherever the sheet says `color`, pass `Color::rgb(r, g, b)`, `Color::rgba(r, g, b, a)`, `Color::white(alpha)` or `tone.color()`; `color.hex()` gives `#RRGGBB` text.

**Icons**: `Speaker`, `SpeakerMuted`, `Headphones`, `Display`, `Microphone`, `MicrophoneMuted`, `Device`, `Checkmark`, `ChevronDown`, `ChevronUp`, `ChevronLeft`, `ChevronRight`, `Xmark`, `Plus`, `Minus`, `Tray`, `Folder`, `Exclamation`, `Convert`, `Compress`, `Combine`, `Split`, `Gear`, `Note`, `Play`, `Pause`, `Stop`, `Record`, `Backward`, `Forward`, `Keyboard`, `Mouse`, `Gamepad`, `Phone`, `Watch`, `Bluetooth`, `Pipette`, `Viewfinder`, `Clock`, `Bell`, `Star`, `Grid`, `Gauge`, `Timer`, `Calendar`, `Video`, `Cloud`, `Link`, `Spark`, `Bolt`, `Battery`, `Lock`, `Unlock`, `Download`, `Cup`, `Moon`.

## Portable or native

Build portable unless the tool needs something only a native one can do.

A **portable** tool is WebAssembly: one file for every computer, and it runs in a sandbox inside notchbase. So in a portable tool:
- No `std::fs`, `std::net`, `std::process`, `std::thread`, `std::time::Instant` or `SystemTime::now()`: they don't exist there and fail when called. Use `now_ms()` for the time, `Command::fetch` for HTTP, `Command::read_file` / `Command::write_file` for files, `Command::open` to open things, and `context.time_zone()` (an IANA name such as `Europe/Paris`) for the user's time zone.
- `Command::perform` and `Command::spawn` still work but run at once, on the spot, so keep their work short; they are for computation, not for waiting.
- Every crate you add must build for `wasm32-unknown-unknown` (pure-Rust crates such as `serde`, `jiff` or `regex` do; anything that wraps a C library, opens sockets or talks to the OS doesn't).
- A tool that runs away or crashes is stopped by notchbase without taking anything else down. It is several times slower than native code, which is unnoticeable for ordinary tools; a view that draws thousands of canvas shapes on every pointer move is where you'd feel it.

Go **native** (`--native`) for: talking to the OS or hardware, running other programs, long-running background threads, sockets other than HTTP requests, big binary files, or a crate that won't build for WebAssembly. Say which you built and why when you hand the tool over. Tests (`cargo test`) always run natively, whichever you ship.

## Design taste (it should feel Apple-made)

- One idea per tool. A tile says one thing at a glance; the page holds the rest.
- Short labels, sentence case, no exclamation marks, no emoji. Numbers in `.value()`.
- Live activities are for things happening *now*; return `None` from `live()` when nothing is.
- Notices confirm actions ("Copied") in under two seconds; errors use `Command::notify` with a plain explanation. A notice with a button is the exception: leave it up about five seconds.
- Defaults should work with zero setup. Every setting needs a clear title and, if not obvious, a `.detail`.
- Empty states say what to do ("Nothing yet. Add one above.").

## Limits (say so instead of faking it)

Tools can't open their own OS windows, read other apps' windows, or run while notchbase is closed. Custom visuals go through `canvas`, which draws 2D shapes, text, images, gradients and soft shadows; there are no shaders, no blur of what is behind the notch, and no rotation other than by computing rotated points yourself. A portable tool reaches the outside world only through commands (HTTP, text files, opening things); anything else a Rust crate can do in the background (system calls, other programs, sockets) needs a native build, where it is fine inside `Command::perform`/`spawn`.

## References

Read these when you need detail. They're the SDK's own documentation.

| File | Read it for |
| --- | --- |
| `references/tools/Overview.md` | The big picture and how tools fit into notchbase |
| `references/tools/Getting-Started.md` | A complete first tool, end to end |
| `references/tools/Elm-Architecture.md` | Model, messages, update, views, `Context` |
| `references/tools/Elements.md` | Every element and its options, with examples |
| `references/tools/Surfaces.md` | Tiles, pages, live activities, notices |
| `references/tools/Commands.md` | Every command, including background work |
| `references/tools/Subscriptions.md` | Timers, shortcuts, media, volume, bluetooth, battery, screen lock, camera and microphone use, downloads, the notch state |
| `references/tools/Settings-and-Shortcuts.md` | Declaring, showing, reading and saving settings; shortcut syntax |
| `references/tools/Testing.md` | `Harness` and lower-level testing |
| `references/tools/Plugins-and-Distribution.md` | Building, installing, reloading, sharing, troubleshooting |
| `references/tools/Example-Snippets.md` | A complete mid-sized tool with explanation |
| `references/tools/example-notes.md` | A complete plugin: page, list, live activity, folder setting, notch state, background file writes, full tests |
| `references/tools/Built-in-Tools.md` | How the built-in tools use the SDK |
| `references/Calendar.md` | The built-in Calendar: header tab, month view, live activity, secret iCal settings, Apple Calendar reads and writes |
| `references/tools/Protocol-and-ABI.md` | The wire format, only for non-Rust plugins or deep debugging |
