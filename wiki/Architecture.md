# Architecture

This page is the map. It explains how the process starts, how the code is organised, which threads exist, how a frame is scheduled and painted, and how data and user intent flow between the services, the notch, the configuration and the tools.

## One binary, two modes

`src/main.rs` declares every top-level module and picks what the process is from its first argument (`launch::Launch`):

1. **`Hook`**: started as `notchbase hook claude ...` or `notchbase hook codex ...`. The same executable acts as a small hook client for Claude Code or Codex: `assistant::run_hook_from_args()` reads the hook payload from stdin, forwards it to the running app over a local socket, prints the answer and exits. See [Assistant](Assistant.md).
2. **`Installer`**: `--install` or `--uninstall`, handled by `installer::run_from_args()`. See [Installing](Installing.md).
3. **`App`**: anything else. `installer::arrive()` waits for the copy it replaces (after an update or install), the process claims the single-instance lock, and `app::run()` builds the `winit` event loop and runs the app.

### One running copy per data directory

Only one notchbase runs per data directory. Before any window, service or the assistant server exists, `app::claim_instance` asks `platform::claim_instance()` for an `InstanceLock`, keyed by the data directory (`NOTCHBASE_DATA_DIR` or the default), so a development build with its own data directory runs beside the installed app. On Windows the lock is a named mutex, `Local\notchbase.<first 24 hex digits of the SHA-256 of the lower-cased data directory path>` (`platform/windows/instance.rs`); on macOS it is an exclusive `flock` on `<data dir>/notchbase.lock` (`platform/macos/instance.rs`); other systems always get the lock. The `App` keeps it for the life of the process and the OS releases it when the process ends, so a crash never leaves a stale lock. When the lock is taken, the new process does not start: it sends a `show` call to the running copy over the assistant endpoint (see [Assistant](Assistant.md#the-wire-protocol-wirers)), which shows "notchbase is already running" in its notch, prints why to stderr and exits with code 0. A launch with no arguments whose standard input is a pipe or socket is a stray (`Launch::Stray` in `launch.rs`, detected by `platform::input_is_piped`, macOS and other Unix only): that is how a hook runner that reads Claude's settings but ignores the hook's `args` (the Grok CLI does) starts the bare executable on every event, so it exits at once without claiming the lock, starting the app or sending `show`. Hooks and the `--install` / `--uninstall` commands never touch the lock, and previews (`NOTCHBASE_PREVIEW`) run unguarded so a preview can run beside the real app. A fresh launch tries once. A copy started by the updater or installer (`--replaces` / `--installed`) has already waited for the old process to exit, and still retries up to 20 times, a quarter of a second apart, in case the old one is slow to let go.

In release builds on Windows the binary uses the `windows` subsystem so no console window appears. Debug builds keep the console, and Ctrl+C (or closing the console) is turned into an orderly quit by `platform::on_console_quit`.

## Workspace layout

The repository is a Cargo workspace (`members = ["crates/*", "sdk", "setup", "tools/*"]`). Shared versions of `egui`, `image`, `serde`, `ureq`, `sha2`, `windows` and the internal crates live in `[workspace.dependencies]`, and every member takes `edition`, `license` and `rust-version` from `[workspace.package]`.

| Path | Crate | What it is |
| --- | --- | --- |
| `.` | `notchbase` | The app: windows, notch, pages, services, the tool host and platform code. Its `build.rs` only calls `notchbase-build`. |
| `setup/` | `notchbase-setup` | The web installer handed to people. It is an ordinary crate that depends on the shared crates below (no `#[path]` includes of app files). See [Installing](Installing.md). |
| `sdk/` | `notchbase-sdk` | The Elm-style SDK tools are written against, plus the wire protocol and plugin ABI. See [tools/Overview.md](tools/Overview.md). |
| `crates/*` | shared crates | Code both the app and the installer use. See the crate map below. |
| `tools/color-picker`, `tools/screenshot`, `tools/screen-recorder`, `tools/alerts` | bundled tools | Compiled into the app as ordinary crate dependencies and listed in `src/tools/bundled.rs`: Pick Color, Screenshot, Record Screen and [Alerts](Alerts.md). See [tools/Built-in-Tools.md](tools/Built-in-Tools.md). |
| `tools/battery`, `tools/camera-mic`, `tools/downloads`, `tools/focus-status`, `tools/keep-awake` | bundled tools | The [system tools](System-Tools.md), compiled in the same way. |
| `tools/calendar`, `tools/notes`, `tools/timer`, `tools/world-time` | tools | Further tools in the workspace; see [tools/Built-in-Tools.md](tools/Built-in-Tools.md) and [Calendar](Calendar.md). |
| `tools/focus-timer` | reference plugin | Built as a dynamic library and loaded at runtime like any third-party tool. See [tools/Plugins-and-Distribution.md](tools/Plugins-and-Distribution.md). |
| `examples/drag_out` | example | A standalone harness for the file drag-out code. See [Building and Testing](Building-and-Testing.md#the-drag-out-example). |
| `macos/Info.plist`, `scripts/bundle-macos.sh` | packaging | The macOS app bundle. See [Platform: macOS](Platform-macOS.md). |

### Workspace crate map

The crates under `crates/` hold everything the app and `notchbase-setup` share, so the installer looks and behaves like the app without copying its files.

| Path | Crate | Used by | What it holds |
| --- | --- | --- | --- |
| `crates/build` | `notchbase-build` | `build.rs` of the app and of `setup/` (build-dependency) | Build-script helper: `embed_icon` turns `assets/icon/notchbase.ico` into a Windows resource and links it on the MSVC target. |
| `crates/net` | `notchbase-net` | app (update, webvideo, stats, filedrag, outside tools, macOS media script), setup | One way to make HTTPS agents (`agent`, `config`, `Mode`, and the `user_agent!` macro) a hashed, size-limited download (`copy_hashed`, `Limits`, `CopyError`, `fraction`) and SHA-256 hex helpers (`hex`, `sha256_hex`, `file_sha256`). |
| `crates/release` | `notchbase-release` | app (`src/update`, `src/installer`), setup | `Version`, `Release`, `newest`, `platform`, the update `Server` (reads the newest GitHub release's `latest.json` for a platform's build) and `download` (checksum-verified, through `notchbase-net`), and the `install` contract the app and the installer agree on: the `--install` / `--uninstall` flags and `Choices`, install destinations, the macOS bundle helpers and the Windows uninstall registry key. |
| `crates/paint` | `notchbase-paint` | app (re-exported from `src/ui`), setup | The hand-painted UI kit: canvas, colors, metrics, shapes, waves, glass, the see-through `erase` textures, springs, scrolling, fonts (with system font lookup in `font/system.rs`), glyphs (one file per category drawn with a shared `Pen`), the liquid meter and liquid shape, controls, switches, buttons and `list` (cards, rows, dividers, switch rows). See [Rendering and Motion](Rendering-and-Motion.md). |
| `crates/spectrum` | `notchbase-spectrum` | app (`src/media`), setup (its dance) | The FFT and the six-band music `Analyzer` (`Bands`, `BANDS`). See [Media and Music](Media-and-Music.md#the-spectrum-analyzer). |
| `crates/wasapi` | `notchbase-wasapi` | app on Windows (`src/record/windows`, and `platform::Apartment` re-exported from `src/platform`), setup on Windows (its dance) | Windows WASAPI capture: the COM `Apartment` guard, `Audio` clients and `Packet`s, `Loopback` capture of the default output as mono floats, and the shared PCM format (`RATE`, `CHANNELS`, `FRAME_BYTES`). Everything but `pcm` is `cfg(windows)`. |

## Module map

Everything below lives in `src/`.

| Module | Responsibility | Page |
| --- | --- | --- |
| `app` | The `winit` application: the `App` struct that owns every service, the event loop and frame scheduling, one notch window per display, configuration following, hotkeys, drag and drop, feeding tools, and performing actions and tool requests. | this page |
| `notch` | The notch itself: modes, geometry, morph springs, activities and tool notices, live activities, music, ask prompts, drop zone, pager, header and the page panel. | [Notch](Notch.md) |
| `pages` | The pages inside the open panel: player, tools, a tool's own page, shelf, mixer, stats, settings. | [Pages](Pages.md) |
| `ui` | The GL window (`ui/gl_window`), the element renderer for tools (`ui/element`), text fields, segmented control, swipes, motion and pictures; it re-exports the painting kit from `notchbase-paint` (`ui::canvas`, `ui::glyph`, `ui::glass`, ...). | [Rendering and Motion](Rendering-and-Motion.md) |
| `launch.rs` | Tells a hook process, an install or uninstall command and the app apart from the first argument. | this page |
| `bridge` | The types the app and the notch pass each other: `Page`, `Tab`, `Action`, `PanelData`, `Ask`, `ToolSlot`, `ToolView`, `ToolInfo`, `Locations`. | this page |
| `config` | `config.json`: the model, loading, saving, reloading when the file changes, and migration from the old settings file. | [Configuration](Configuration.md) |
| `tools` | The tool registry and host: bundled tools, plugin loading, per-tool runtimes, feeds, shortcut bindings and the image store. `tools::shortcuts` has the keycaps the UI shows. | [tools/Overview.md](tools/Overview.md) |
| `media` | Now playing, media controls, output capture and spectrum analysis. | [Media and Music](Media-and-Music.md) |
| `audio` | Per-app and device volume. | [Sound Mixer](Sound-Mixer.md) |
| `shelf` | The file shelf model and its file operations, and sending with AirDrop (`shelf::airdrop`). | [Shelf](Shelf.md) |
| `filedrag` | Dragging shelf files out to other apps, thumbnails, reveal in Finder / Explorer. | [Shelf](Shelf.md) |
| `webvideo` | Downloading the video on a page: recognising YouTube and Instagram addresses, keeping yt-dlp, Deno and ffmpeg in the data folder, running a download and following its progress. `webvideo::browser` reads what the browser in front is showing. | [Video downloads](Video-Downloads.md) |
| `find` | The file index and search behind the Find page. | [Find](Find.md) |
| `whats_new` | The notes a release carries, compiled into the build, and whether the latest have been seen. | [Updates](Updates.md#whats-new) |
| `stats` | Hardware readings and plan usage for Claude and Codex. | [Stats](Stats.md) |
| `assistant` | Hook client, local server, prompt board and hook installer. | [Assistant](Assistant.md) |
| `capture` | Color sampler, region and area pickers, recording dim. | [Screen Capture](Screen-Capture.md) |
| `annotate` | The annotator: a window for drawing and writing on a screenshot, its editor, the typesetter, the rasterizer and the clipboard worker. | [Annotate](Annotate.md) |
| `record` | The screen recorder, and `unique_file` naming shared with screenshots. | [Screen Recording](Screen-Recording.md) |
| `snap` | Window layouts: drag a window to the notch and drop it on a layout. | [Configuration](Configuration.md#snap) |
| `update` | Checking for, downloading and installing updates, on top of `notchbase-release`. | [Updates](Updates.md) |
| `installer` | `--install` / `--uninstall`: copying the app into place, registering it and handing off to the new copy. | [Installing](Installing.md) |
| `system` | Watchers and switches for the operating system, one folder each: `bluetooth`, `calendar`, `volume`, `brightness`, `power`, `session`, `sensors`, `downloads`, `awake`, `focus`, `menus`, `summon` and `permissions`. | [Bluetooth](Bluetooth.md), [Calendar](Calendar.md), [Volume and Media Keys](Volume-and-Media-Keys.md), [System tools](System-Tools.md) |
| `poll` | What the background threads share: workers that stop when dropped, polls that publish only changes, and mailboxes back to the app. | [System tools](System-Tools.md) |
| `platform` | Window styles (the notch, the overlays and the annotator window), click-through, focus, notch placement, known folders, the data, config and tools locations, screen capture access, the single-instance lock. | [Windows](Platform-Windows.md), [macOS](Platform-macOS.md) |

### The platform pattern

Every module with OS-specific code follows the same shape: a `macos` module, a `windows` module and an `other` module (stubs that compile anywhere and report "not supported"), selected with `cfg` and imported under a common alias, usually `imp` (sometimes `backend`). The three implementations expose the same functions and types, so the rest of the crate never branches on the OS. For example `src/audio/mod.rs` picks `audio::macos`, `audio::windows` or `audio::other` as `imp`, and the mixer driver only ever talks to `imp::Backend`.

Keep this pattern when you add platform code: add the function to all three implementations, keep the signatures identical, and make the `other` version a harmless no-op or a clear error.

## The `App` (`src/app`)

`App` (`app/mod.rs`) owns everything the process runs:

| Field(s) | What it is |
| --- | --- |
| `notches` | the notch windows, one per display (`app/notches/mod.rs`, `app/notches/window.rs`) |
| `config` | the `ConfigStore` for `config.json` |
| `tools`, `bindings`, `tabs`, `replies`, `locations` | the tool registry, the global hotkey bound to each tool shortcut, the header tabs, replies waiting to be delivered to tools, and the tools folder and config file paths shown in Settings |
| `panel` | the services pages read: `Mixer`, `Stats`, `Shelf`, the current now playing, and the shelf notice (`app/panel.rs`) |
| `media`, `volume`, `bluetooth`, `recorder`, `inbox` | now playing, volume keys, Bluetooth, the screen recorder, and the assistant server |
| `picker`, `sampler`, `framer`, `recording_dim` | the region picker, color sampler, area picker and recording dim |
| `pending_capture`, `capture_reply`, `recording_options` | a capture waiting for the notch to collapse, the tool request a capture will answer, and the options a requested recording will use |
| `hotkeys`, `keys`, `typing` | the global hotkey manager, the shortcut being rebound (if any), and whether a tool text field has focus |
| `drops`, `clipboard`, `front_app` | file drag and drop state, the clipboard, and what to hand focus back to: an app, or the annotator (`ReturnTo`, `app/return_to.rs`) |
| `airdrops` | the AirDrop windows still open and when to collapse the notch after the tiles lift off (`app/shelf/airdrop.rs`) |
| `news` | whether this version's [What's New](Updates.md#whats-new) notes are still unseen, and since when their page has been showing (`app/whats_new.rs`) |
| `annotating` | the open [annotator](Annotate.md) window, if any, with its clipboard copier, the copy debounce, who has the keyboard, the app to hand focus back to when it closes, the copier's reports, and the copiers of closed windows that still owe the clipboard a picture (`app/annotate/`) |

### Start-up

`App::new`:

1. Opens the configuration: `ConfigStore::open(platform::config_file(), <data dir>/settings.txt)`. A missing `config.json` is created with defaults, carrying over an old `settings.txt` if one exists. A broken file is reported and the defaults are used. See [Configuration](Configuration.md).
2. Loads the tools with `Tools::load` (bundled tools plus plugins from the tools folder; see [Tools](#tools) below). Load problems are kept to be shown once the notch exists.
3. Starts the services: recorder, media, volume, Bluetooth, the panel's mixer, stats and shelf, and the assistant server.
4. `adopt_tools` writes every loaded tool into `config.json` if it is not there yet (its setting defaults, its default shortcuts, and its place in `layout.tools`), then builds the shortcut bindings and the tabs.

When `winit` calls **`resumed`**, the app opens the notch windows, shows the start-up problems as notch messages, installs global hotkeys and registers the tool shortcuts, **starts the tools** (sending each one its `Start` with its settings, data folders, platform and host version, and performing the requests they return), connects the assistant (installing hooks if configured), shows the [What's New](Updates.md#whats-new) notice if this version's notes are unseen and no start-up message is showing, and applies a preview if `NOTCHBASE_PREVIEW` is set. With no screen to open a notch on (a Mac started while locked, or with every display off, lists none), the app doesn't quit: `waiting_for_screen` makes `about_to_wait` look again every second, and all of the above runs (`start_up`) once the first notch opens.

### The event handlers (`app/handler.rs`)

- **`user_event`** handles a global hotkey (`app/tools/shortcuts.rs`: Escape cancels a capture; any other hotkey is delivered to the tool that owns it as a `Shortcut` event), a **wake-up** from a background thread, a clipboard failure, and quit.
- A **wake-up** syncs the recorder, pumps the tools, syncs stats, the shelf and assistant prompts, and redraws the notches.
- **`window_event`** routes events to the notch window that owns them (redraws paint; file hovers and drops go to drag and drop), to the recording dim, to the [annotator](Annotate.md) window (which is also shown every other window's focus changes, to know when its keyboard is on loan), or to a capture overlay.
- **`about_to_wait`** runs one frame of bookkeeping (below).

### A frame (`app/frame.rs`)

Every pass through `about_to_wait` runs `run_frame`, in this order:

1. **Follow the configuration** (`app/config.rs`): the store re-reads `config.json` if its size or modification time changed, at most once a second. A change reloads the tools if tools were enabled, disabled or reordered; otherwise it sends new settings to the tools and rebinds shortcuts if they changed. It also rebuilds the tabs, re-places the notches if "all displays" changed, restarts the volume indicator if it changed, and reconnects the assistant if the provider or "questions in the notch" changed. A broken file is reported as a notch message.
2. Re-arrange notch windows if requested, let the mixer driver do maintenance, and sync the recorder, recording dim, now playing, volume and Bluetooth into the notches (and into the tools, see below), and the calendar and the [system feeds](System-Tools.md) into the tools. The [annotator](Annotate.md) is followed here too (`sync_annotator` in `app/annotate/frame.rs`): the copier's reports are read, the keyboard's loan is settled, the window's outcomes are drained (a changed picture re-arms the copy debounce, a changed brush is saved, a close drops the window), a due copy is handed to the clipboard thread and a due repaint is requested.
3. Start a pending capture if the notches have collapsed or its deadline has passed.
4. **Pump the tools** (below).
5. Refresh the data for the visible page. Expensive sources are only sampled while their page is on screen, once per `stats.refresh_ms` (1 second by default): the mixer for the Media, Sound and Tools pages, stats for the Stats page, and a check for vanished files on the Shelf page. While a tool tile or page on screen contains a timer, the notch is also redrawn once per second.
6. Release a tool text field's keyboard focus if the panel has closed, and expire the shelf notice.
7. Unless a shortcut is being rebound or a text field has focus, make the notch nearest the cursor the active one. Work out the cursor position relative to it and track drags (files dragged in, shelf files dragged out).
8. Re-check notch placement if it is due (about once a second).
9. **Drive each notch**: tick its state machine with the time and cursor, switch the window between click-through and input as the cursor enters or leaves the shape, and request a redraw if anything moved or an animation is running.
10. Advance capture overlays, pace the frame, and choose the next wake-up.

The next wake-up is the earliest of: a 20 ms cursor poll, any deadline a notch asks for (hover dwell, leave grace, an activity expiring, the next second of a live timer), egui's own repaint request (if within a second), a pending capture, the collapse after an AirDrop lifts off, the shelf notice expiring, the next tool timer tick, the next second boundary while a tool timer is on screen, and the annotator's copy debounce or next repaint. The loop sleeps with `ControlFlow::WaitUntil`, so an idle notch wakes about fifty times a second only to poll the cursor, and paints nothing.

Painting happens on `WindowEvent::RedrawRequested` (`app/handler.rs`), which calls `paint_notch` (`app/notches/mod.rs`): the app builds `PanelData`, asks the window to paint, and handles the returned actions.

Frame time (`dt`) follows the wall clock between 0 and 1/20 s. After a stall longer than 0.1 s the clock restarts at a nominal 1/60 s so springs do not jump. On Windows, after a frame is presented the loop sleeps until the display's next vertical blank, read from the compositor's timing (`DwmGetCompositionTimingInfo`), and never longer than two frames, so animation stays in step with the display. It used to wait with `DwmFlush`, but on some monitors the compositor only composes when something else changes, and the notch then stalled until the next mouse move. At start-up the process also opts out of Windows power throttling, so its timers are never slowed down in the background. See [Rendering and Motion](Rendering-and-Motion.md).

### One notch window per display

The app keeps a list of notch windows, one per display that should show a notch. By default only the main display gets one (the display with a hardware notch if there is one, otherwise the primary display). With `displays.all` ("All displays" in Settings), every monitor gets its own notch. `platform::placements` decides where notches go and which style each uses: a display whose hardware notch can be measured gets the **Notch** style, every other display gets the **Island** style (see [Notch](Notch.md)).

Placement is re-checked about once a second, so plugging in or removing a monitor, or changing resolution, rearranges the windows: existing windows are kept and moved when their monitor still exists, spare windows are reused, and new ones are opened as needed. The **active** notch is the one nearest the cursor; it receives hover and input, and the others see the cursor as "elsewhere". Every notch receives the same pushed updates (messages, notices, now playing, prompts, the live activity).

Each notch window is a transparent, undecorated, always-on-top window with a fixed logical canvas of 680 x 600 points, centered at the top of its monitor. It is click-through except where the notch shape is.

### Input, click-through and keyboard focus

The notch window must never steal clicks from the desktop around the shape. Each frame the app asks the notch whether it wants input at the current cursor position (its input zone is the current silhouette, plus the open panel or prompt, plus a small margin). If yes, the window stops being click-through and receives mouse events; if no, it becomes click-through again and egui is told the pointer is gone. While the window accepts input, the app also feeds egui the pointer position from the platform cursor query every frame, so hover state stays correct even when the OS does not send a move event.

On macOS the notch takes focus when clicked, so the app remembers which app was frontmost when the pointer entered and hands focus back when it leaves. While the annotator window has the keyboard, or has lent it to the notch or a capture overlay, what is remembered is the annotator instead of an app (`ReturnTo::Annotator`): when the pointer leaves the notch, or a capture or a text field ends, the annotator is focused again if the keyboard is still on loan, and nothing is done if it already has the keyboard or the user has moved to another app. So nothing is activated over the window you are drawing in, and a click on the notch does not leave the annotator without the keyboard (see [Annotate](Annotate.md#focus)).

Two things need the keyboard (`app/tools/shortcuts.rs`): **rebinding a shortcut** and **typing into a tool's text field**. Both call `hold_keyboard`: the active notch is **pinned** open (so the pointer leaving does not close it), the window is made focusable and focused (`platform::begin_keyboard_focus`), the window keeps accepting input (in Find only over the notch itself, so a click beside it reaches what is underneath and closes Find; see [Find](Find.md#the-app-side)), and the frontmost app is remembered. While a shortcut is being rebound, all tool hotkeys are unregistered so the keys reach the notch. When a shortcut is saved or cancelled, the hotkeys are registered again. When neither needs the keyboard any more (the shortcut is done, the text field was submitted or cancelled, the panel closed, or the window lost focus), `free_keyboard` unpins the notch, makes the window non-activating again and gives focus back.

## Data flow

The core loop between the app and the notch is deliberately one-directional:

```
services (media, mixer, stats, shelf, assistant, recorder, bluetooth, volume) and tools
        |
        v
App builds PanelData each frame  (a borrowed, read-only view)
        |
        v
Notch::ui(ui, &PanelData, dt)   paints the shape, the layers and the visible page
        |
        v
Vec<Action>                      what the user asked for this frame
        |
        v
App::handle(action)              calls into the services, updates config.json, talks to tools
```

- **`PanelData`** (`src/bridge/mod.rs`) is a struct of references: shortcut views, tabs, tool views, the tool catalog, the tools folder and config file locations, the image store, the mixer, now playing, the stats snapshot, the shelf and its notice, the assistant and the few core settings the Settings page shows. The notch never owns or mutates a service.
- **`Action`** (`src/bridge/action.rs`, handled in `app/actions.rs`) is the only way the notch asks for change:

  | Actions | Handled by |
  | --- | --- |
  | `Tool(slot, surface, handler, value)` | delivering a UI event to a tool and pumping the tools. The surface is the tile, the page, the live activity or, for a [notice's controls](Notch.md#tool-notices), the notice |
  | `SetToolSetting`, `SetToolEnabled`, `ReloadTools`, `OpenPath` | saving a tool setting to `config.json` and resending settings; enabling or disabling a tool and reloading all tools; rescanning the tools folder; opening a folder or file |
  | `RecordShortcut`, `SetShortcut`, `ClearShortcut`, `CancelRecording`, `Typing` | rebinding tool shortcuts and text field focus |
  | `SetVolume`, `SetMuted`, `SetDefaultDevice` | the mixer |
  | `RefreshStats` | a manual usage refresh |
  | `ListApps`, `QuitApp` | which gauge the Stats page has unfolded (apps for CPU or Memory, storage for Disk), and quitting or force quitting an app |
  | `RevealPath` | show a file or folder from the storage list in Finder |
  | `SetAssistant`, `SetQuestionsInNotch`, `SetVolumeIndicator`, `SetBrightnessIndicator`, `SetAllDisplays` | saving the core settings and applying them |
  | `Answer` | replying to an assistant prompt |
  | `ShelfRun`, `ShelfCancel`, `ShelfRemove`, `ShelfClear`, `ShelfReveal`, `DragOut` | the shelf |
  | `Media` | media controls |
  | `FindQuery`, `FindShown`, `FindPick`, `FindClose`, `SetFindEnabled`, `FindForget`, `FindHideAccess` | [Find](Find.md#the-app-side): asking, the start and end of a visit (`FindShown(true)` is routed through `find_opened_on`, which keeps Find open on one display at a time), open, reveal, drag or shelve a result, closing with Escape, the on/off switch, clearing what Find learned, and dismissing the access row |
  | `ViewChanges`, `ChangesSeen` | [What's New](Updates.md#whats-new): opening the page of this release's notes and marking them seen in `updates.seen` (`ViewChanges` is routed through `view_changes` with the window that sent it, so only that display's notch opens), and the page's **Done**, or a press in the header that leaves the page while the notes are due, marking them seen |

- **Direct notch updates.** Some things are pushed into every notch outside of `ui`: `show_message`, `show_notice` and `dismiss_notice` (tool notices), `set_live` (the tool live activity), `show_volume`, `show_brightness`, `show_device`, `set_now_playing`, `set_music_levels`, `anticipate`, `show_ask`, `set_drop_hover`, `open_page`, `set_pinned` and `collapse`.

## Tools

A tool is an Elm-style program (model, `update`, views, subscriptions) written against the SDK. The app hosts it; see [tools/Overview.md](tools/Overview.md) for the tool author's side. This is the host's side.

### Loading (`src/tools/mod.rs`, `bundled.rs`, `plugin.rs`)

`Tools::load(config, wake)`:

1. Constructs every **bundled** tool from `bundled::all()`: `color_picker::ColorPicker`, `screenshot::Screenshot`, `screen_recorder::ScreenRecorder` and `alerts::Alerts`, each boxed as a `Driver` by `notchbase_sdk::boxed`.
2. Scans the **tools folder** (`platform::tools_dir()`: `tools` next to the executable, or `NOTCHBASE_TOOLS_DIR`) with `plugin::libraries`: each sub-folder contributes one dynamic library (`.dll`, `.dylib` or `.so`), preferring the library named after its folder (ignoring a `lib` prefix, case, and `-` versus `_`), otherwise the first alphabetically. Loose files in the tools folder itself are ignored. Each library is loaded with `libloading`, its entry symbol (`notchbase_tool_<name>`, from the library's file name) is called, its ABI version is checked, and its manifest is requested. A plugin with the same id as a bundled tool replaces it. A library that fails to load is reported ("Couldn't load the tool at …") and listed in the catalog with its problem.
3. Sorts the tools by their position in `layout.tools` in `config.json` (unlisted tools go last).
4. Builds the **catalog** (`ToolInfo`: id, name, version, location or "built in", enabled, problem) that Settings shows, including disabled and broken tools.
5. Creates a `ToolHost` for every tool whose `tools.<id>.enabled` is not `false`, numbered with a `ToolSlot` from 1.

A change to `layout.tools`, from `config.json` or from dragging an icon on the Tools tab (`Action::MoveTool`), only re-sorts the running tools (`Tools::reorder`); their slots and state stay. Enabling or disabling a tool in Settings, the Reload button, or a change to enabled tools in `config.json` reloads the whole registry: captures are cancelled, the image store is kept, the tools are loaded and started again, and shortcuts are re-registered.

### The host (`src/tools/host.rs`)

A `ToolHost` drives one tool's `Driver`:

- **Inbox.** Events are queued with `send` (for one tool) or `broadcast` (for every tool whose `Wants` asks for that kind). Nothing runs until the next pump.
- **Timers.** A tool's `Wants.every_ms` list becomes timers; when one is due the host queues a `Tick`. A late timer skips ahead instead of firing a burst.
- **Waking.** A tool's own background work (for example Alerts' webhook) can wake the host through its `Waker`; the host flags the tool and wakes the app.
- **Pump.** If the tool has events or was woken, the host calls `handle(events)` once with all of them and absorbs the returned `Effects`: the view revision, the new `Wants`, and the **requests**.
- **Views.** When the revision changed, the host asks for the tool's views (tile, page, live activity) and caches them.
- **Isolation.** Every call into the tool runs inside `catch_unwind`. A panic marks the tool failed ("the <id> tool stopped: …"), clears its inbox, timers and views, and it is never called again; its page shows "This tool stopped working" and the catalog shows the problem.

### Feeding tools (`app/tools/feeds.rs`, `app/tools/mod.rs`)

Each pump (`App::pump_tools`) first **feeds** current state to the tools that subscribed, sending each kind only when it changed:

| Feed | Source | Sent when |
| --- | --- | --- |
| recording state | the recorder (`Idle`, `Choosing`, `Starting`, `Recording { since }`, `Saving`) | it changes; plus `Saved` and `Failed` announcements |
| audio inputs | the mixer's input devices | the list changes (only while a tool wants it) |
| now playing | title, artist, playing, position, duration | it changes |
| notch state | whether the panel is open and which page key is visible | it changes |
| Bluetooth | connected, disconnected and updated devices | each change, as it happens |
| volume | the output level | each change, as it happens |
| power | the battery level and whether the charger is in | it changes |
| screen lock | whether the screen is locked | it changes |
| sensors | whether the camera or microphone is in use, by which apps, and the input mute | it changes |
| downloads | a browser download's progress, finish or cancellation | each change, as it happens |

A tool that subscribes later is immediately sent the current value. Then queued **replies** (for example from a background image save) are delivered, all hosts are pumped, the returned requests are performed, and the tool views are refreshed: if any view changed, the tabs are rebuilt and the first tool that provides a live activity becomes every notch's live activity.

### Performing requests (`app/tools/requests.rs`)

| Request | What the app does |
| --- | --- |
| `Notify { text }` | shows a message activity |
| `Notice { notice }` / `DismissNotice` | shows the tool's notice in every notch / ends it early if it is the one showing (see [Notch](Notch.md#tool-notices)) |
| `CopyText { text }` | puts text on the clipboard |
| `CopyImage { image }` | copies a captured image from the image store to the clipboard on a background thread |
| `Annotate { image }` | opens the captured image in the annotator window to draw on, or focuses the window if it already shows that image; an image the store no longer holds in full is answered with "That image is no longer available". See [Annotate](Annotate.md) |
| `SaveImage { image, folder, prefix, format, reply }` | writes a PNG or JPEG on the `notchbase-save-image` thread to the given folder or `Pictures/Screenshots`, named with `record::unique_file` ("Screenshot 2026-09-28 at 14.03.27.png", numbered if taken), and replies `Saved` with the path or an error |
| `PickColor { reply }` / `CaptureScreen { reply }` | starts the color sampler or region picker and remembers the reply id; the result (or nothing, if cancelled or another capture is running) comes back as a `Color` or `Capture` reply. Captures are stored in the image store and referred to by `ImageId`. See [Screen Capture](Screen-Capture.md) |
| `StartRecording { options }` / `StopRecording` | runs the area picker and starts the recorder with the options, or stops it. See [Screen Recording](Screen-Recording.md) |
| `CancelCapture` | cancels any capture in progress |
| `SaveSetting { key, value }` | writes `tools.<id>.settings.<key>` to `config.json` and resends the tool's settings |
| `Shelve { paths }` | adds files to the [Shelf](Shelf.md) |
| `Reveal { path }` / `Open { target }` | selects a file in Finder or Explorer / opens a file, folder or URL |
| `OpenPage` | opens the notch on the tool's own page |
| `Collapse` | collapses the notch |
| `Media { control }` | play/pause, next or previous |
| `KeepAwake { awake }` | holds or releases the keep-awake assertion for that tool (see [System tools](System-Tools.md#keep-awake)) |
| `MuteMicrophone { muted }` | mutes or unmutes the default input device |
| `Log { text }` | writes `notchbase: <id>: <text>` to stderr |

The **image store** (`src/tools/images.rs`) keeps captures for tools: full images of the four most recent captures and 192 px thumbnails of the last 64, plus thumbnails of image files tools reference by path.

### Shortcuts (`src/tools/bindings.rs`)

`bindings(tools, config)` turns every shortcut a tool declares in its manifest into a `Binding`: the `ShortcutRef`, the tool id, the shortcut id and title, and the hotkey, taken from `tools.<id>.shortcuts.<shortcut>` in `config.json`, or the manifest's default if the config has no entry (an explicit `null` means "no shortcut"). The app registers each hotkey with the `global-hotkey` crate; a hotkey that cannot be registered is reported ("<title> shortcut is unavailable"). Rebinding in the notch checks for clashes with other tools ("In use by <title>") and with other apps ("In use by another app") before saving. See [tools/Settings-and-Shortcuts.md](tools/Settings-and-Shortcuts.md).

### Tabs

The header tabs are built in `app/tools/mod.rs`, `tabs()`. First the keys listed in `layout.tabs` in `config.json`, in that order: a core page (`media`, `tools`, `shelf`, `sound`, `stats`) or the id of a loaded tool whose manifest asks for a header tab (`.header_tab()`) and has a page; unknown keys and repeats are skipped. Then every other such tool, in tool order. A tool tab is titled with its page title and shown with its `page_icon`. Such a tool gets no automatic launcher tile on the Tools tab, since its tab opens it (a tile it draws itself still shows). Settings is never a tab; it is behind the gear. When tools are loaded, `remember_tools` drops from `layout.tabs` the ids of loaded tools that don't ask for a tab and keys that are neither a core page nor a tool in `tools`; a known tool that isn't loaded right now keeps its place.

## Threads

The UI runs on the main thread (AppKit requires it on macOS). Anything that can block runs on a named background thread and hands its results back through shared state plus a wake-up. The long-lived threads are:

| Thread name | Owner | Job |
| --- | --- | --- |
| `notchbase-media` | `media::watcher` | Polls the now-playing source and runs media controls. |
| `notchbase-levels` | `media::listener` | Captures system output while music plays and computes the spectrum bands. |
| `notchbase-audio` | `audio::driver::worker` (Windows) | Owns the WASAPI mixer backend. On macOS the mixer runs inline on the main thread. |
| `notchbase-stats` | `stats::sampler` | Reads CPU, GPU, memory and disk on request. |
| `notchbase-claude`, `notchbase-codex` | `stats` | One short-lived thread per usage refresh. |
| `notchbase-shelf` | `shelf::worker` | Probes shelf items, builds thumbnails, runs file operations. |
| `notchbase-video`, `notchbase-video-out`, `notchbase-video-cover`, `notchbase-tab` | `webvideo`, `app/videos.rs` | One per video download and two reading its output; a short-lived one fetching its cover; a short-lived one asking the browser what a dragged tab shows. See [Video downloads](Video-Downloads.md). |
| `notchbase-record` | `record::windows` | The Windows recording pipeline. |
| `notchbase-bluetooth` | `bluetooth::watch` | Polls connected devices. |
| `notchbase-power`, `notchbase-session`, `notchbase-sensors`, `notchbase-downloads` | `power`, `session`, `sensors`, `downloads` | Poll the battery, the screen lock, camera and microphone use, and the Downloads folder, each only while a tool subscribes. |
| `notchbase-volume` | `volume::watch` | Applies volume and brightness key presses and watches the output level. |
| `notchbase keys` / `notchbase volume keys` | `volume` | The low-level keyboard hook (Windows) or event tap run loop (macOS). |
| `notchbase sampler input` | `capture::color::windows` | Mouse and keyboard hooks while the Windows color loupe is open. |
| `notchbase-clipboard`, `notchbase-save-image` | `app` | Copies large images to the clipboard, and writes images for tools, without stalling a frame. |
| `notchbase-annotate-copy` | `annotate::copier` | One per open annotator window, kept after the window closes until its last picture has been reported, and joined at quit if it is still owed. Composites the screenshot with its lines and words and writes it to the clipboard, always the newest picture it was handed. See [Annotate](Annotate.md#the-clipboard). |
| `notchbase-find` | `find::writer` | Find's single writer: applies listings and file-system changes to the index, keeps the usage model, rebuilds and saves. While Find is hidden and nothing is pending it sleeps on its inbox until the next real deadline (an hourly save, a weekly reconcile), so an idle Find wakes nothing. A writer that dies is started again by the next order. See [Find](Find.md#threads). |
| `notchbase-find-query` | `find::asking` | Answers Find's questions, newest first; blocks while nobody asks. |
| `notchbase-find-scan` | `find::scan` | One to four directory listers, alive only while there is listing to do; the only threads that run at background priority. |
| `notchbase-find-watch` | `find::os::windows` | Windows only: drains `ReadDirectoryChangesExW` completions. On macOS FSEvents uses a dispatch queue instead. |
| `notchbase-find-icons` | `find::icons` | Draws file icons by type for Find's rows; blocks while nothing is wanted. |
| `notchbase-find-recents` | `find::crew` | macOS: one short run at start and when Find opens (at most every 5 minutes) reading recently used files from Spotlight. |
| `notchbase-find-apps` | `find::apps` | Reads the name each app shows and how it is used, for Find's app rows; blocks until the list of apps changes or Find opens. |
| `notchbase-summon`, `notchbase summon key` | `summon` | The first follows the **Replace Spotlight** / **Replace the Start menu** switch every 250 ms; the second exists only while it is on and runs the event tap (macOS) or the keyboard hook (Windows) that turns ⌘Space or a lone Windows key into a press. |
| `assistant-accept`, `assistant-sweep`, `assistant-hook` | `assistant::server` | Accepts hook connections, expires prompts and runs (ending those whose window process exited), serves one connection each. |

Tools themselves are called on the main thread by the host; a tool that needs background work starts its own thread and wakes the host when it has something.

### Waking the event loop

Background work never touches the UI directly. Each service is given a wake closure built from the `winit` `EventLoopProxy` (`app/events.rs`); tools get the same through an SDK `Waker`. When a thread publishes something new (a stats sample, a finished shelf operation, a recording that started, a new assistant prompt, a tool's webhook message) it calls the closure, which sends `UserEvent::Wake`. The app then pulls the latest state from each service on the main thread, pumps the tools and redraws. This keeps all rendering state single-threaded and makes the services easy to test on their own.

## Configuration and data on disk

- **Configuration**: `config.json` next to the executable, except in a macOS app bundle, where it lives in the data directory so the signed bundle is never written to (`platform::config_file_in`; override with `NOTCHBASE_CONFIG`). `ConfigStore` (`src/config/store.rs`) owns the file: it creates it with defaults on first run (migrating `settings.txt`), saves the whole file whenever the app changes a value, re-reads it about once a second when it changed on disk, and never overwrites a file it could not parse. Every change is also **published** as a shared snapshot, so any module can read the current settings with `config::current()` (the notch reads hover and close delays, notice durations and music options this way; the Bluetooth, media and stats pollers read their intervals this way). See [Configuration](Configuration.md) for every key.
- **App data** lives in the data directory: `%APPDATA%\notchbase` on Windows and `~/Library/Application Support/notchbase` on macOS (override with `NOTCHBASE_DATA_DIR`). It holds `shelf.json` (the shelf's file list), `video-engine` (the programs [video downloads](Video-Downloads.md#the-engine-webvideoenginers) use, fetched on first use), `assistant.json` (the port and token the hook client uses), `usage-history.jsonl` (two weeks of plan usage samples, see [Stats](Stats.md#history-statshistory)), `notchbase.lock` on macOS (the [single-instance lock](#one-running-copy-per-data-directory)), the `Shelf` fallback output folder, the legacy `settings.txt` if one was ever written, the `find` folder (Find's `index.bin` and `usage.bin` and a transient `icons` probe folder; on Windows it is under `%LOCALAPPDATA%\notchbase` instead, see [Find](Find.md#files)), and `tools/<id>/`, the data folder each tool is told about in its `Start`.
- **Plugins** are loaded from the `tools` folder next to the executable (override with `NOTCHBASE_TOOLS_DIR`). See [tools/Plugins-and-Distribution.md](tools/Plugins-and-Distribution.md).

## Errors and messages

User-facing problems are shown as a short message activity in the notch (for example "Allow Screen Recording for notchbase in System Settings") and also written to stderr with a `notchbase:` prefix. Background failures that do not need the user's attention only go to stderr. Services prefer degrading to crashing: a mixer that cannot start reports a reason that the mixer shows, a media capture that fails falls back to a synthetic beat, a failed shelf operation becomes a notice, a broken `config.json` keeps the last good settings, and a tool that panics is switched off on its own.
