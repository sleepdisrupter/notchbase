# Building and testing

## Requirements

- **Rust**, a recent stable toolchain. The crates use edition 2024 and its `let` chains (`if let ... && let ...`), so Rust 1.88 or newer is needed.
- **Windows**: the MSVC toolchain (the default `x86_64-pc-windows-msvc` or `aarch64-pc-windows-msvc` target) with the Visual Studio Build Tools for linking. The Windows API bindings come from the `windows` and `windows-sys` crates; no extra SDK setup is needed.
- **macOS**: the Xcode Command Line Tools. The bundle targets macOS 14.2 and newer.

## Building

```bash
cargo build              # debug build of the app
cargo build --release    # optimized build
cargo build --workspace  # the app, the installer, the shared crates, the SDK and the tool crates
cargo run                # build and start the app
```

The workspace (`Cargo.toml`, `members = ["crates/*", "sdk", "setup", "tools/*"]`) contains the app at the root, the installer in `setup/`, the shared crates in `crates/*` (`notchbase-build`, `notchbase-net`, `notchbase-release`, `notchbase-paint`, `notchbase-spectrum`, `notchbase-wasapi`), the SDK in `sdk/`, and the tool crates in `tools/*`. Shared dependency versions and `edition` / `rust-version` are set once in `[workspace.dependencies]` and `[workspace.package]`. The app's and the installer's `build.rs` are a few lines each: both call `notchbase_build::embed_icon` (the Windows icon resource). The bundled tools (`color-picker`, `screenshot`, `screen-recorder`, `alerts`) are ordinary dependencies of the app and are compiled into it; `tools/focus-timer` is the reference plugin and is built as a dynamic library. See [Architecture](Architecture.md#workspace-layout) and [tools/Overview.md](tools/Overview.md).

### Where a checkout keeps its files

Both `config.json` and the plugin `tools` folder live **next to the executable**, so a checkout run with `cargo run` uses `target/debug/config.json` and `target/debug/tools/` (`target/release/...` for release builds). That keeps development settings apart from an installed notchbase automatically. The exception is a macOS `.app` bundle, including the one `scripts/bundle-macos.sh` builds: its `config.json` is the one in the data folder, shared with an installed notchbase unless `NOTCHBASE_DATA_DIR` or `NOTCHBASE_CONFIG` points elsewhere. To try a plugin in a debug build:

```bash
cargo build -p focus-timer
mkdir -p target/debug/tools/focus-timer
cp target/debug/libfocus_timer.dylib target/debug/tools/focus-timer/   # macOS
cargo run
```

```powershell
cargo build -p focus-timer
New-Item -ItemType Directory -Force target\debug\tools\focus-timer
Copy-Item target\debug\focus_timer.dll target\debug\tools\focus-timer\
cargo run
```

The Settings page's Tools view shows both locations and has a Reload button that rescans the tools folder without restarting. See [tools/Plugins-and-Distribution.md](tools/Plugins-and-Distribution.md).

### Profiles

- **Dev**: `[profile.dev.package."*"] opt-level = 2` optimizes every **dependency** at level 2 even in debug builds, while notchbase itself stays unoptimized for fast incremental builds. Image decoding, PDF parsing, audio decoding and egui's tessellation are then fast enough that a debug build feels like the real thing.
- **Release**: thin LTO, one codegen unit, and stripped symbols.

### Keeping `target/` small

Cargo never deletes what an earlier build left behind, and every new `version` in the root `Cargo.toml` gives notchbase a whole new set of files in `target/` (two weeks of releases once came to 181 GB). `scripts/prune-target.sh` removes the leftovers:

```bash
./scripts/prune-target.sh --dry-run   # lists what would go
./scripts/prune-target.sh
```

- notchbase's own build output, and every incremental cache, that has not been written since the root `Cargo.toml` last changed. After a version bump that is exactly what the older versions built.
- Build output of the other workspace crates (the SDK, the tools, `crates/*`) that has not been rebuilt for a week. One that is still in use is compiled again by the next build, which takes seconds.

Dependencies from crates.io are never touched, so nothing slow is rebuilt, and a profile folder that a running cargo build holds is left alone. It needs `python3`.

In debug builds on Windows the app keeps its console window, which is where `notchbase: ...` log lines appear; release builds use the GUI subsystem and show no console. Press Ctrl+C in the console to quit a debug build cleanly.

### Running a development copy next to your real one

The data directory (shelf, assistant endpoint, tool data) is shared by every copy on the machine. Use separate folders so a development build does not touch your real shelf:

```bash
# bash / zsh
NOTCHBASE_DATA_DIR=/tmp/nb-data NOTCHBASE_CONFIG=/tmp/nb-data/config.json cargo run
```

```powershell
# PowerShell
$env:NOTCHBASE_DATA_DIR = "$env:TEMP\nb-data"; $env:NOTCHBASE_CONFIG = "$env:TEMP\nb-data\config.json"; cargo run
```

Be aware that a build with "Questions in the notch" turned on installs Claude Code or Codex hooks that point at **its own** executable path, and that the hook client looks for `assistant.json` in the data directory it sees. See [Assistant](Assistant.md#things-to-keep-in-mind). [Previews](Previews.md) never touch hooks.

## The macOS app bundle

```bash
./scripts/bundle-macos.sh
open target/release/notchbase.app
```

This builds a release binary with `MACOSX_DEPLOYMENT_TARGET=14.2`, assembles `target/release/notchbase.app` with `macos/Info.plist`, and code-signs it with `NOTCHBASE_SIGN_IDENTITY` (ad hoc if unset). Run notchbase from the bundle when testing features behind macOS permissions. See [Platform: macOS](Platform-macOS.md#bundling-scriptsbundle-macossh).

## The installer

The `setup` crate is the installer (`notchbase-setup`). It builds like any other crate: everything it shares with the app comes from the `crates/*` libraries (UI kit, release lookup and download, install contract, spectrum and Windows audio capture), not from `#[path]` includes of app files, so changing one of those crates rebuilds and retests both. It is part of the workspace, so `cargo test --workspace` and `cargo clippy --workspace` cover it. `build-installer.bat` and `build-installer.sh` build it into `dist/`. See [Installing](Installing.md#the-installer-setup).

## Testing

```bash
cargo test                 # the app's unit tests
cargo test --workspace     # also the installer, the shared crates, the SDK and the tools
cargo test -p notchbase-paint   # one shared crate, e.g. the UI kit
cargo test shelf::         # only tests whose path contains "shelf::"
cargo test -p notchbase-sdk
```

Tests live next to the code they test: a `#[cfg(test)] mod tests` at the bottom of a file, or a `tests` module folder for larger suites (for example `src/shelf/tests`, `src/assistant/tests`, `src/shelf/video/windows/tests`, `crates/paint/src/glyph/tests.rs`, `crates/release/src/server/tests.rs`). There is no separate `tests/` directory.

What the suite covers, broadly:

- **The shared crates**: the Windows icon (`crates/build`), HTTPS agents and hashed copies (`crates/net`), versions, choosing the newest release, the server and the install choices (`crates/release`), glyphs, the `Pen`, lists and fonts (`crates/paint`), the FFT and analyzer (`crates/spectrum`) and PCM conversion (`crates/wasapi`, the one module that builds off Windows).
- **Pure logic** everywhere: springs, geometry, layouts, parsers, state machines, the spectrum analyzer, the PNG optimizer, shelf naming, assistant boards and wire formats, config loading, reloading and migration, hotkey parsing.
- **The tool system**: `src/tools/tests.rs` loads the real bundled tools and checks their order, disabling, views, settings, shortcut bindings, late subscribers and shortcut delivery; `app/tools/mod.rs` checks how new tools are written into the configuration and how tabs are built; the SDK and each tool crate have their own tests (`cargo test --workspace`).
- **Headless UI**: the notch is driven through full egui frames without a window, including a test that paints every preview state in both styles (`notch/frame.rs`), and the loupe and picker drawing.
- **Real system integration** where it is safe and deterministic: Windows Core Audio enumeration, Media Foundation transcoding of generated fixture videos, shell thumbnails, and the assistant server and client over a real loopback socket in temporary folders.

Tests use temporary folders (named with the process id) and clean up after themselves. They never read or write your real shelf, configuration or assistant settings.

### Ignored tests

Tests that would record the screen, capture real audio, show a window, read personal devices, or need a built binary are marked `#[ignore]` with a reason. Run them deliberately:

```bash
cargo test -- --ignored                       # every ignored test
cargo test record::windows -- --ignored       # just the recorder's
cargo build && cargo test assistant -- --ignored   # the hook child-process tests need the built binary
```

Current ignored tests include: recording the real screen and desktop audio (`record/windows/tests.rs`), reading the real media session and loopback output (`media/windows/tests.rs`), reading this computer's Bluetooth devices (`system/bluetooth/windows`), briefly showing a window to check capture exclusion (`capture/color/windows/native.rs`), reading the real output device and hooking the keyboard (`system/volume/windows`), reading this Mac's display brightness and writing the same level back (`system/brightness/macos.rs`), running the `notchbase` binary as a hook process (`assistant/tests/child.rs`), writing PNGs of the notch's previews and of the annotator to `target/snapshots` (`notch/tests/snapshot.rs`, `annotate/editor/snapshot.rs`, see [Previews](Previews.md#looking-at-a-state-without-a-window)), and writing an annotated screenshot to the real clipboard (`annotate/copier/tests.rs`).

## Linting and formatting

```bash
cargo fmt --all                 # format
cargo fmt --all --check         # verify formatting
cargo clippy --workspace --all-targets -- -D warnings
```

`rustfmt.toml` sets edition 2024, `max_width = 120` and `use_small_heuristics = "Max"`. Clippy must be clean with warnings denied, for tests too. See [Contributing](Contributing.md).

## Cross-checking the macOS target

Most development can happen on either OS, but **both platforms must keep compiling**. From Windows, type-check the macOS code without a Mac:

```bash
rustup target add aarch64-apple-darwin
cargo check --target aarch64-apple-darwin --workspace --all-targets
cargo clippy --target aarch64-apple-darwin --workspace --all-targets -- -D warnings
```

`check` and `clippy` do not link, so no Apple SDK is required. Do this before sending any change that touches shared code or `cfg(target_os = "macos")` code. (The reverse, `--target x86_64-pc-windows-msvc` from a Mac, works the same way for the Windows code.)

## The drag-out example

`examples/drag_out` is a small standalone app that exercises `src/filedrag` directly:

```bash
cargo run --example drag_out              # drag two temp files out of the window; every drag event is printed
cargo run --example drag_out -- --reveal  # reveal the first temp file in Finder / Explorer and exit
cargo run --example drag_out -- --self-test   # Windows: style the window like the notch and script a drag onto it
```

## Environment variables

| Variable | Effect |
| --- | --- |
| `NOTCHBASE_DATA_DIR` | Data directory for `shelf.json`, `assistant.json`, the Shelf fallback output folder and each tool's data folder (`tools/<id>/`). Default: `%APPDATA%\notchbase` on Windows, `~/Library/Application Support/notchbase` on macOS. |
| `NOTCHBASE_CONFIG` | Path of the configuration file. Default: `config.json` next to the executable, or in the data folder for a macOS app bundle. See [Configuration](Configuration.md). |
| `NOTCHBASE_TOOLS_DIR` | Folder scanned for plugin tools. Default: `tools` next to the executable and `tools` in the data folder. See [tools/Plugins-and-Distribution.md](tools/Plugins-and-Distribution.md). |
| `NOTCHBASE_PREVIEW` | Pin the notch in a named state for screenshots and visual checks. See [Previews](Previews.md). |
| `NOTCHBASE_NOTCH` | `WIDTHxHEIGHT` in points: treat the primary display as having a camera notch that size, such as `190x33`. See [Previews](Previews.md#a-macbook-notch-anywhere). |
| `NOTCHBASE_SHELF_FILES` | With `NOTCHBASE_PREVIEW=shelf`, files to put on the shelf, separated like `PATH` (`;` on Windows, `:` on macOS). |
| `NOTCHBASE_PREVIEW_SELECTION` | `x0,y0,x1,y1` in physical pixels: start the Windows screenshot picker with this selection drawn. |
| `NOTCHBASE_PROFILE` | Print average UI, tessellation, paint and swap times per window every 120 frames. |
| `NOTCHBASE_OVERLAY_IN_CAPTURES` | Windows: let the picker overlays and recording dim appear in screen captures. |
| `NOTCHBASE_LOUPE_IN_CAPTURES` | Windows: let the color loupe appear in screen captures (it then sits beside the cursor). |
| `NOTCHBASE_SIGN_IDENTITY` | macOS bundle script: code-signing identity. Default: the `notchbase Local Signing` certificate if your keychain has it, otherwise ad hoc. See [Platform: macOS](Platform-macOS.md#signing-and-permissions). |
| `NOTCHBASE_RELEASES_REPO` | Build time: the GitHub repository (`owner/name`) the app and the installer update from. Default: `sleepdisrupter/notchbase`. See [Updates](Updates.md#releasing-a-fork). |

notchbase also honours the assistants' own variables: `CLAUDE_CONFIG_DIR` (Claude Code's config folder, for hooks and credentials) and `CODEX_HOME` (Codex's folder, for hooks and usage logs). See [Assistant](Assistant.md) and [Stats](Stats.md).
