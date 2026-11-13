# notchbase tools

Every tile on the notch's Tools page is a tool: a crate written against [`notchbase-sdk`](../sdk/README.md). This folder holds the bundled tools that ship inside the app and the example plugins that are built and installed on their own.

## The tools

### Bundled

Linked into the app as path dependencies and registered in `src/tools/bundled.rs` through `notchbase_sdk::boxed`.

| Crate | Type | Description |
| --- | --- | --- |
| `color-picker` | `ColorPicker` | Pick any color on screen and copy it |
| `screenshot` | `Screenshot` | Capture a region of the screen to the clipboard or a folder |
| `screen-recorder` | `ScreenRecorder` | Record an area of the screen with microphone and desktop audio |
| `timer` | `Timer` | A countdown timer with a physical wheel picker, a draining ring and an alarm |
| `keep-awake` | `KeepAwake` | Keeps the computer and its display awake, for a while or until switched off |
| `roam` | `Roam` | One keyboard and mouse across Mac and Windows computers on the same network |
| `alerts` | `Alerts` | Shows alerts sent to a local webhook in the notch |
| `calendar` | `Calendar` | Your next meeting in the notch, with a button to join it |
| `battery` | `Battery` | Charging, low battery and fully charged alerts in the notch |
| `camera-mic` | `CameraMic` | Shows when the camera or microphone is in use, with a mute button |
| `downloads` | `Downloads` | Browser downloads in the notch, landing on the Shelf when they finish |
| `focus-status` | `FocusStatus` | Shows in the notch when a Focus or Do Not Disturb turns on or off |

### Plugins

Built as `cdylib` (and `rlib`, for tests) and loaded at runtime from the `tools` folder next to the executable.

| Crate | Type | Description |
| --- | --- | --- |
| `focus-timer` | `FocusTimer` | A Pomodoro-style focus timer with breaks, a live countdown and a session tally |
| `notes` | `Notes` | Quick notes in the notch: jot, pin one as a live activity, and keep a Markdown copy in any folder |
| `world-time` | `WorldTime` | Your cities side by side, hour by hour, so the time difference is obvious |

`notes` is also the example tool shipped in the [tools kit](../sdk/KIT.md).

## How a tool uses the SDK

Each crate depends on `notchbase-sdk`, implements the `Tool` trait and exports itself:

```rust
use notchbase_sdk::prelude::*;

struct Counter {
    count: u32,
}

#[derive(Clone)]
enum Msg {
    Add,
}

impl Tool for Counter {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("counter", "Counter")
    }

    fn init(_context: &Context) -> (Self, Command<Msg>) {
        (Counter { count: 0 }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Add => self.count += 1,
        }
        Command::none()
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        Some(Tile::new(format!("{}", self.count)).on_press(Msg::Add))
    }
}

notchbase_sdk::export!(Counter);
```

- **Model and messages.** The tool struct is the model; `update` handles one message at a time.
- **Views as data.** `tile`, `page` and `live` return `Tile`, `Element` and `Live` values that the host paints with its own UI kit.
- **Effects through commands.** A tool never touches the OS directly; it returns a `Command` (fetch, copy, capture, save a setting, show a notice) and receives the result as a message.
- **Events through subscriptions.** Timers, shortcuts, media, power, calendar, downloads and other system state arrive through `Subscription`.
- **One source, two builds.** `export!` produces a native plugin entry point, or the WebAssembly exports when built for `wasm32-unknown-unknown`, which gives a portable, sandboxed tool.

See the [tools wiki](../wiki/tools/Overview.md), especially [Getting started](../wiki/tools/Getting-Started.md), [Plugins and distribution](../wiki/tools/Plugins-and-Distribution.md) and [Built-in tools](../wiki/tools/Built-in-Tools.md).

## Building and installing a plugin

```sh
cargo build -p focus-timer
cargo run
```

A debug build loads plugins from `target/debug/tools/<name>/`. Copy the built library (`focus_timer.dll`, `libfocus_timer.dylib`) into that folder, or into `tools/<name>/` next to an installed notchbase, and the app loads it without a restart. For a portable build:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p focus-timer --target wasm32-unknown-unknown
```

## Platform notes

Tools are platform-neutral: OS work happens in the host and reaches a tool through commands and subscriptions. `downloads` is the one crate here with `cfg`-selected code.

## Testing

```sh
cargo test --workspace
cargo test -p calendar
```

Most tools keep a `tests.rs` next to their code and drive the tool with `notchbase_sdk::testing::Harness`: send messages and events, then check the model, saved settings and requests. See [Testing tools](../wiki/tools/Testing.md).
