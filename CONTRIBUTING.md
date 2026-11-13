# Contributing to notchbase

Thanks for wanting to help. This is the short version of how the code is written. The longer guide is [wiki/Contributing.md](wiki/Contributing.md), and the build and test commands are in [wiki/Building-and-Testing.md](wiki/Building-and-Testing.md).

## Both platforms

notchbase runs on macOS and Windows, and every feature has to work on both. A change that only works on one of them isn't done yet, and neither build is allowed to break.

Code that differs per system always has the same shape: a `macos`, a `windows` and an `other` module with the same signatures, picked with `cfg`. `other` is a harmless stub, so the app still builds and tests on Linux.

```rust
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(windows, target_os = "macos")))]
use other as imp;
#[cfg(windows)]
use windows as imp;
```

You can check the system you're not on without its SDK, because `check` and `clippy` don't link:

```sh
cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings
cargo clippy --target aarch64-apple-darwin --workspace --all-targets -- -D warnings
```

## Where things are

`src/` has a folder for each part of the app. A few of them hold more than one thing:

- `src/app` is the running app. Each file adds an `impl App` block for one area, and the folders hold the bigger features.
- `src/notch` is the notch itself, split into how it moves, the open panel, live activities, notices, questions and music.
- `src/system` has the small OS watchers: power, Bluetooth, the volume and brightness keys, Focus, the calendar, downloads, the camera and mic, the lock screen, the menu bar, the summon key and permissions.
- `src/tools` is the tool host: built-in tools, plugins and portable tools.

Code the app and the installer share lives in `crates/`. The SDK for writing tools is `sdk/`, the tools themselves are in `tools/`, and the installer is `setup/`. Each of those has a README saying what's in it.

## Code style

### Comments

Comments are short and plain, and there aren't many.

- Every file starts with a `//!` line or two saying what it's for.
- `///` goes on things whose name doesn't tell the whole story, like a function with a catch or a tuning constant whose unit or purpose isn't obvious.
- `//` inside a function is only for a reason the code can't show: a platform quirk, a workaround, an order that matters. Don't narrate what the next line does.
- No banners, no commented-out code, no TODO lists. Open an issue instead.
- Wrap comments yourself at 120 columns. rustfmt won't do it.

Names still do most of the work. A tuning value gets a named `const` at the top of its file, and a block that needs explaining usually wants to be its own function. Anything that spans several files goes in the wiki.

### Formatting

`rustfmt.toml` sets `edition = "2024"`, `max_width = 120` and `use_small_heuristics = "Max"`. Run `cargo fmt --all` before you commit.

Imports go in groups with a blank line between them: `std`, then external crates, then `crate::`, then `super::` and local modules.

### Modules and visibility

- When a file gets big, split it by responsibility into a folder with a `mod.rs`.
- Keep visibility as narrow as it can be: private first, then `pub(super)` or `pub(in crate::...)`, and `pub` only for a crate's real API.

### Errors

- Use `Option` and `Result` for anything that can go wrong. Don't panic on the UI thread.
- Don't block the main thread on files, the network or a child process. Use a named background thread.
- Diagnostics go to stderr with `eprintln!` and start with `notchbase: `, like `eprintln!("notchbase: glass shader: {log}")`.

### Lints

`cargo clippy --workspace --all-targets -- -D warnings` has to pass on both platforms. Only allow a lint when there's really no other way.

## Tests

- Tests sit next to the code they test: a `#[cfg(test)] mod tests` at the bottom of the file, or a `tests.rs` or `tests/` folder beside it.
- A test's name says what it checks, and stays short enough to read at a glance: `blip_is_not_a_disconnect`, `hover_peeks_then_opens_after_dwell`. Leave out a leading "a" or "the". Tests don't need doc comments.
- Tests that need the real OS, a window, a device or the network get `#[ignore = "reason"]`, so `cargo test --workspace` stays quick and runs anywhere. Run those with `cargo test -- --ignored`.

Before you send a change:

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Commit messages

Subjects look like `<Area>: <what changed>`, written as a plain sentence about the result:

```
Find: a click anywhere outside the notch closes it
Shelf: AirDrop sends every file when none are selected
Music: a song the pointer brought up tucks away 0.3 s after it leaves
```

Releases use `Release <version>: <headline>`, like `Release 0.23.4: a click anywhere outside the notch closes Find`.

## Secrets

notchbase needs no keys to build or run. Never commit tokens, signing certificates or `.p12` files, and never paste them into code, logs, issues or pull requests.
