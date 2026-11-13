# Contributing

Thanks for helping with notchbase. This page describes how the code is written and what a change needs before it is merged. Read [Architecture](Architecture.md) first for the lay of the land, and [Building and Testing](Building-and-Testing.md) for the commands.

## The two platform rules

1. **macOS is the primary platform** (macOS 27 is the target). Features are designed for the Mac first, and the Mac is where the notch has to feel best.
2. **Windows must work 100%.** Every feature ships on both platforms. A change that only works on one of them is not finished, and neither platform's build may break.

In practice:

- Put OS-specific code behind the [platform pattern](Architecture.md#the-platform-pattern): a `macos`, a `windows` and an `other` implementation with identical signatures, selected with `cfg`. Keep `other` compiling as a harmless stub.
- If you develop on Windows, cross-check the Mac with `cargo check --target aarch64-apple-darwin` and clippy (see [Building and Testing](Building-and-Testing.md#cross-checking-the-macos-target)); if you develop on a Mac, do the reverse for Windows. Mention in the pull request which platform you actually ran it on.
- Degrade gracefully: when a permission is missing or an API is unavailable, show a short, actionable message ("Allow System Audio Recording for notchbase in System Settings") instead of failing silently or crashing.

## Code style

### Comments

Comments are few, short and plain. They say what a file is for and why something is the way it is, never what the next line does.

- **Every file opens with a `//!`** of a line or two: what the module is for, in the words someone new to it would use.
- **`///` where a name isn't enough**: a function with a catch, a type with a rule its fields don't show, a tuning constant whose unit or purpose isn't obvious.
- **`//` only for a why**: a platform quirk, a workaround for an API, an order that matters, the reason a timeout has the value it has, the reasoning behind an `unsafe` block. If you don't know the reason for sure, leave the comment out rather than guess.
- **Nothing else**: no banners or dividers, no commented-out code, no TODOs, no restating the code in English.
- **Wrap by hand** at the 120 column limit, since rustfmt doesn't wrap comments.
- **Tests get no doc comments.** The name says it.

The code still explains itself first:

- **Names carry the meaning.** Prefer `LEAVE_GRACE`, `hover_zone`, `claim_free_name`, `blip_is_not_a_disconnect` over short names plus a comment.
- **Constants over magic numbers.** Every tuning value is a named `const` at the top of its file (`const PEEK_AFTER: Duration = Duration::from_millis(150);`).
- **Small functions.** If a block needs explaining, it usually wants to be a function whose name is the explanation.
- **Tests as documentation.** A test name is a short statement of the behaviour (`hover_peeks_then_opens_after_dwell`, `rows_hold_still_under_pointer`), with no leading "a" or "the", and short enough to read at a glance: keep it to about 36 characters. If something is subtle, write a test for it.
- **Wiki for the big picture.** How a subsystem fits together, and anything that spans files, belongs in these pages. When you change how something works, update its page in the same change.

### Small, modular files

Files are short and focused, and most are under 250 lines including their tests. When a file grows, split it by responsibility into a folder module (see how `src/notch/music`, `src/audio/macos/taps`, `src/shelf/pdf`, `crates/paint/src/glyph` (one file per glyph category) or `setup/src/app` (`mod.rs`, `flow.rs`, `view.rs`) are organised). Modules expose a small surface (`pub(super)`, `pub(in crate::...)`) and keep their internals private.

Related modules share a folder: the OS watchers live under `src/system`, the notch's motion under `src/notch/moves`. Put new code next to its neighbours rather than at the top of `src/`.

### Formatting

`rustfmt.toml`:

```toml
edition = "2024"
max_width = 120
use_small_heuristics = "Max"
```

Run `cargo fmt --all` before committing. Imports are written in groups separated by blank lines: `std`, then external crates, then `crate::`, then `super::` and local modules.

### Lints

`cargo clippy --workspace --all-targets -- -D warnings` must pass, on both platforms. Don't silence lints with `allow` unless there is no reasonable alternative; the existing code uses `#[cfg_attr(..., expect(dead_code))]` or `allow` only for variants that exist on one platform and not another.

### Other conventions

- **No panics on the UI thread for recoverable problems.** Use `Option`/`Result`, and turn errors into a notch message or a stderr line prefixed with `notchbase:`. Background workers that could panic on bad input wrap their work in `catch_unwind` (see the shelf worker and the tool host).
- **No blocking on the main thread.** Anything that can wait on I/O, the network, a system service or a child process runs on a named background thread and hands results back with a [wake-up](Architecture.md#waking-the-event-loop). The two exceptions that run inline on macOS (the Core Audio mixer and AppKit calls) are there because the APIs are main-thread friendly and fast.
- **Animate with springs.** Anything that moves uses `ui::spring::Spring` with a named `SpringConfig`, so motion is frame-rate independent and interruptible. See [Rendering and Motion](Rendering-and-Motion.md#springs).
- **Draw with the canvas and glass**, not egui widgets, so everything matches.
- **Share through `crates/`, never through `#[path]`.** Code the app and the installer both need lives in a workspace crate (`notchbase-paint` for drawing, `notchbase-net` for HTTPS and downloads, `notchbase-release` for versions, releases, downloads and the install contract, `notchbase-spectrum`, `notchbase-wasapi`, `notchbase-build` for build scripts; see the [crate map](Architecture.md#workspace-crate-map)). Make one HTTPS agent with `notchbase_net::agent` rather than building `ureq` agents by hand, and hash downloads with `notchbase_net::copy_hashed` / `sha256_hex`. A new glyph goes into the category file it belongs to in `crates/paint/src/glyph/` and draws through the shared `Pen`.
- **Keep `unsafe` small and local.** FFI calls are wrapped in small safe functions near where they are declared; resource handles get a `Drop` impl that releases them.
- **Respect the user's files.** Never overwrite: claim a free name (see the shelf's output naming), stage long writes in a temporary file, and back up before editing someone else's config (see the assistant's hook installer).
- **Privacy.** Never log tokens, file contents or personal data. Debug output of secrets is redacted.

## Tests

- **Tests live next to the code**: a `#[cfg(test)] mod tests` at the end of the file, or a `tests` folder module beside a larger subsystem. There is no top-level `tests/` directory.
- Every new behaviour gets a test whose name states the behaviour. Bug fixes come with a test that failed before the fix.
- Prefer pure, deterministic tests: pass times and cursor positions in (the notch's `tick(now, cursor)` is built for this), use synthetic data, and write files only under the system temp directory, cleaning up afterwards.
- UI can be tested headlessly: the notch has a test rig that runs full egui frames without a window, and new notch states should get a [preview](Previews.md) and be added to the paint-every-state test.
- Tests that touch the real screen, real audio, real devices, show windows or need a built binary must be `#[ignore = "reason"]` so `cargo test` stays safe and quiet on any machine.
- Run `cargo test --workspace` before sending a change.

## Adding a feature

- **A new quick action in the notch** (something with a tile, a shortcut, maybe a page or a live activity) is usually best written as a **tool** against the SDK, not as app code. Tools are isolated, testable on their own, and do not need changes to the app shell. Start with [tools/Getting-Started.md](tools/Getting-Started.md).
- **A new system integration** (a new kind of data or control) is app code: a service module with a platform pattern, a background thread if it can block, data exposed through `PanelData` or pushed into the notch, and, if tools should use it, an event or request in the SDK protocol plus the host side: a feed or broadcast in `app/tools/feeds.rs` and `src/tools/mod.rs`, or a case in `app/tools/requests.rs` (see [Architecture](Architecture.md#tools), [tools/Subscriptions.md](tools/Subscriptions.md) and [tools/Commands.md](tools/Commands.md)).
- **A new app setting** goes into the configuration model (`src/config/model.rs`) with a sensible default, so older `config.json` files keep loading; read it with `config::current()` where it is used, and react to changes in `app/config.rs` if it needs more than being read again. A setting that belongs to a tool is declared in that tool's manifest instead. See [Configuration](Configuration.md).
- **A new page** follows the page contract in [Pages](Pages.md#writing-a-new-page).

## Pull requests

- Keep changes focused; split unrelated work.
- Describe what changed, why, and how you tested it, on which OS.
- Include before and after screenshots for visual changes; [previews](Previews.md) make them easy.
- Make sure formatting, clippy (both targets) and the test suite pass.
- notchbase is under the AGPL 3.0 (`LICENSE`, and the `license` field in `Cargo.toml`). The SDK and the skill's tool template are MIT, so tools can use any license.
