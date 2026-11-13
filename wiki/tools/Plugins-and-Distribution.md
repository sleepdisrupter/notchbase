# Plugins and Distribution

The same tool crate can be compiled into notchbase or loaded from a dynamic library at startup. This page covers both, with most of the detail on plugins, since those are what you share with other people.

| | Bundled | Plugin |
| --- | --- | --- |
| Where the code lives | `tools/<name>` in the notchbase repository | Anywhere |
| How it's built | With notchbase, as a path dependency | On its own, as a `cdylib` |
| How it's installed | It's part of the executable | A folder under `tools/` next to the executable |
| Needs `export!` | No (harmless) | Yes |
| Used for | The tools that ship with notchbase | Everything else |

## The SDK kit

`notchbase-sdk.zip` is everything someone needs to write plugins without a notchbase checkout. Build it from the repository with `scripts/build-sdk.ps1` (Windows) or `scripts/build-sdk.sh` (macOS); it lands next to `notchbase-skill.zip` in the `Distributable` folder, which isn't committed. Every release attaches both zips, so people download them from the [latest release](https://github.com/sleepdisrupter/notchbase/releases/latest). Both zips are the same for Windows and macOS: each carries the `.ps1` and the `.sh` scripts. Share the two together: the skill lets Claude or ChatGPT write the tool (and answer questions about notchbase), and the kit holds what it compiles against.

It unpacks to one folder:

```
notchbase-tools/
  README.md          how to start, in five steps
  notchbase-sdk/     the SDK crate
  notes/             a complete example plugin with tests
  docs/              these tool pages
  install.ps1        build a tool and copy it into the tools folder (Windows)
  install.sh         the same for macOS
```

New tools go next to `notchbase-sdk`, as `notchbase-tools/<id>/`, and depend on it with `notchbase-sdk = { path = "../notchbase-sdk" }`. Each tool's `Cargo.toml` also carries an empty `[workspace]` table so Cargo treats it as its own project wherever the folder is unzipped. `install.ps1 <id>` (or `sh install.sh <id>`) from the kit folder builds the tool and installs it.

## Making a crate loadable

Two things turn a tool crate into a plugin.

**1. Build a dynamic library.** In `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib", "rlib"]
```

`cdylib` produces the library notchbase loads. Keeping `rlib` as well lets the crate be linked normally (for bundling, and for integration tests in `tests/`). Unit tests in `src/` work either way.

**2. Export the entry point.** End `src/lib.rs` with:

```rust
notchbase_sdk::export!(Snippets);
```

The macro exports one C function named `notchbase_tool_<crate name>`, where the crate name is your package name with `-` replaced by `_`. It hands notchbase a small table of C functions that drive your tool (see [Protocol and ABI](Protocol-and-ABI.md)). Use it exactly once per library.

## Portable tools

A tool can be built two ways from the same source.

| | Portable | Native |
| --- | --- | --- |
| File | `<crate>.wasm` | `<crate>.dll`, `lib<crate>.dylib`, `lib<crate>.so` |
| Runs on | Every notchbase: Windows and macOS, Intel and Apple Silicon | Only the system and processor it was built for |
| Build | `cargo build --release --target wasm32-unknown-unknown` (once: `rustup target add wasm32-unknown-unknown`) | `cargo build --release` |
| Can use | The SDK, and any crate that builds for WebAssembly | Anything a Rust program can |
| If it crashes or hangs | notchbase stops that tool and carries on | The same for a panic; a hang or a hard crash takes notchbase with it |

A portable tool is WebAssembly, run by an interpreter inside notchbase. It is sandboxed: it has no files, network, threads or clock of its own, and reaches the outside only through the SDK.

| Instead of | Use |
| --- | --- |
| `SystemTime::now()`, `Instant` | [`now_ms()`](Elm-Architecture.md) |
| an HTTP crate | [`Command::fetch`](Commands.md#fetch) |
| `std::fs` | [`Command::read_file` and `Command::write_file`](Commands.md#read_file-and-write_file) |
| asking the OS for the time zone | `context.time_zone()` |
| a background thread | `Command::perform` still works, but runs at once on the spot, so it suits computing and not waiting |

Calling one of the missing pieces compiles but stops the tool when it runs, and the tool then shows "This tool stopped working". Why is under the tool in **Settings › Tools**: "it crashed:" followed by what the panic said and the line it came from, such as "it crashed: time not implemented on this platform" for `Instant::now()` or "operation not supported on this platform" for `std::fs` (see [When something goes wrong](#when-something-goes-wrong)). notchbase gives each call a generous budget of work; a tool that never answers is cut off with "it took too long to answer and was stopped". It runs several times slower than native code, which ordinary tools never notice.

When a tool's folder holds both kinds, notchbase loads the native one. `install.sh` and `install.ps1` build portable unless told `--native` / `-Native`, and remove the other kind from the folder so the one you just built is the one that runs.

## Building

```sh
cargo build --release
```

| Platform | Library |
| --- | --- |
| Windows | `target\release\<crate>.dll` |
| macOS | `target/release/lib<crate>.dylib` |
| Linux | `target/release/lib<crate>.so` |

`<crate>` is the package name with `-` replaced by `_`: a package named `focus-timer` builds `focus_timer.dll` and `libfocus_timer.dylib`.

Your Rust dependencies are compiled into the library, so a plugin is normally a single file.

## Installing

Put the library in a folder of its own inside your **tools folder**:

```
tools/
  snippets/
    snippets.dll
  tally/
    tally.dll
```

| Platform | Your tools folder |
| --- | --- |
| Windows | `%APPDATA%\notchbase\tools` |
| macOS | `~/Library/Application Support/notchbase/tools` |

That folder belongs to you: updates never touch it. notchbase also reads a `tools` folder beside its executable (`target/release/tools/` for a checkout, `notchbase.app/Contents/MacOS/tools/` inside the app), which is handy for development but is replaced when the Mac app updates. To use one folder only, set `NOTCHBASE_TOOLS_DIR` before starting notchbase.

notchbase notices new, rebuilt and removed plugins by itself, about a second after the file stops changing, reloads its tools and says so in the notch ("Reloaded notes"). You can also press **Reload** in **Settings › Tools**, or restart notchbase. notchbase looks for plugins when it starts, when you press Reload, and whenever it restarts its tools (which it does when a tool's `enabled` changes). A new plugin gets an entry in `config.json` and a place at the end of the Tools tab. Its page, if it has one, opens from its tile there, unless its manifest asks for a header tab with `.header_tab()`.

### Finding the tools folder from the notch

Open the notch, click the gear, and choose **Tools**. That view shows:

- **Tools folder**: your tools folder, with an **Open** button that creates it if it's missing and opens it in Explorer or Finder.
- **config.json**: where the settings file lives, with an **Open** button.
- **Installed**: every tool notchbase found, built in or plugin. Each row shows the tool's name and version and where it came from (*Built in*, or the path of the plugin's library), with a switch that turns the tool on or off (the same as `tools.<id>.enabled` in `config.json`), and a cog that opens its settings when it has any. A plugin that failed to load is listed with its error in yellow.
- **Reload tools**: rescans the tools folder and restarts every tool, so a plugin you just copied in shows up without restarting notchbase.

### The reference plugin

The repository's `tools/focus-timer` is a complete plugin, and the quickest way to see one working. From a notchbase checkout:

```sh
cargo build -p focus-timer --release
```

Then copy the library into a `focus-timer` folder in the `tools` folder next to the notchbase executable:

**Windows** (PowerShell):

```powershell
New-Item -ItemType Directory -Force <install dir>\tools\focus-timer
Copy-Item target\release\focus_timer.dll <install dir>\tools\focus-timer\
```

**macOS**:

```sh
mkdir -p <install dir>/tools/focus-timer
cp target/release/libfocus_timer.dylib <install dir>/tools/focus-timer/
```

`<install dir>` is the folder holding the notchbase executable: `target/release` for a `cargo run --release` build, or `notchbase.app/Contents/MacOS` for the app. Restart notchbase and the **Focus** tile and tab appear.

### How notchbase finds plugins

At startup notchbase looks at every folder directly inside the `tools` folder:

- In each folder, it loads one file with this platform's library extension: `.dll` on Windows, `.dylib` on macOS, `.so` elsewhere. If the folder holds several, it picks the one named after the folder (`focus-timer/focus_timer.dll`, `focus-timer/libfocus_timer.dylib`), and otherwise the first in alphabetical order, so helper libraries can ship alongside. Other files are ignored, so a README, a license or images can sit alongside too.
- Libraries placed directly in `tools/`, outside a folder, are ignored.
- One folder holds one tool. Because each platform only looks at its own extension, a folder can carry the Windows, macOS and Linux builds side by side.

It then works out the entry point from the library's file name: on macOS and Linux it drops the `lib` prefix the toolchain adds, it replaces `-` with `_`, and it looks for `notchbase_tool_<that>`. So `tally.dll` and `libtally.dylib` both lead to `notchbase_tool_tally`, and a package named `libre` still loads as `libre.dll` on Windows.

**Don't rename the library.** A renamed file looks for an entry point that doesn't exist and won't load.

### Replacing a built-in tool

A plugin whose manifest `id` is the same as a bundled tool's replaces it. Settings and shortcuts are keyed by id, so the replacement inherits the user's choices. To ship your own screenshot tool, for example, use `Manifest::new("screenshot", …)` and read the same setting keys.

If two plugins share an id, the one found last (by folder name) wins. Give every other tool an id no one else is likely to use.

### Updating a plugin

Copy the new library over the old one while notchbase runs. notchbase loads a private copy of each plugin (in the system temp folder), so the file in your tools folder is never locked, and it reloads the tool once the new file has finished copying. The development loop is just: edit, `cargo build --release`, copy, look.

## When something goes wrong

**The plugin doesn't load.** notchbase skips it and shows why in the notch ("Couldn't load the tool at …"):

| Reason | What to check |
| --- | --- |
| It isn't a library notchbase can load | Built for the right platform and architecture? Copied the right file? |
| It doesn't export `notchbase_tool_<name>` | Did you end `lib.rs` with `notchbase_sdk::export!(YourTool);`? Is the file name the one Cargo produced? |
| It was built for a different tool ABI | Rebuild against the SDK that matches your notchbase |
| It couldn't create itself | Its `create` function returned null. SDK-built plugins never do; this points at a hand-written entry point |
| It needs a newer notchbase | It was built with a newer SDK than this notchbase knows. Update notchbase (see [Compatibility](#compatibility)) |
| It isn't a WebAssembly tool | The `.wasm` file is damaged or half copied. Run the install script again |

The same reason stays in yellow under the tool's name in **Settings › Tools**, which also lists every tool that did load with the path of its file, and has **Reload tools**. If a tool is in neither list, its file isn't in a folder of its own inside the tools folder.

The install scripts stop before copying anything when the build fails, and say what to do about the usual causes: no Rust (`cargo`) on this computer, a folder that isn't a tool, a missing `wasm32-unknown-unknown` target, or a dependency that can't be built for WebAssembly (build with `--native` / `-Native`, which they accept before or after the tool's name).

**The tool panics.** Every call into a tool is guarded. A panic in `init`, `update`, a view or a subscription stops that tool for the rest of the run; notchbase and every other tool carry on. Its page says "This tool stopped working", and the reason is written in yellow under the tool in **Settings › Tools**: the panic message for a native tool, and "it crashed: <the panic message> (<file>:<line>)" for a portable one, for example "it crashed: index out of bounds: the len is 0 but the index is 3 (src/lib.rs:44)". Installing a fixed build starts it again, and so does **Reload** in **Settings › Tools** or restarting notchbase.

A portable tool built with an older kit can't say why: it reports only "it crashed: wasm `unreachable` instruction executed". Rebuild it with the current kit to get the message.

Don't build plugins with `panic = "abort"` in the release profile. An aborting panic can't be caught, so it would close notchbase itself.

Panics on your own background threads (from `Command::perform` or `Command::spawn`) end that thread only.

## Compatibility

A plugin talks to notchbase through a handful of C functions that pass JSON (see [Protocol and ABI](Protocol-and-ABI.md)). Nothing Rust-specific crosses the boundary, so a plugin doesn't need to be built with the same Rust version, or the same versions of shared crates such as `serde`, as notchbase. What has to match is:

- the ABI version: `notchbase_sdk::ffi::ABI_VERSION`, currently `1`. notchbase refuses plugins built for another one;
- the protocol version: `notchbase_sdk::protocol::PROTOCOL_VERSION`, currently `5` (version 2 added folder settings, `shown_when`, the full notch state and holding the notch open; version 3 added the power, screen lock, sensors and downloads subscriptions, `keep_awake` and `mute_microphone`; version 4 added `fetch`, `read_file` and `write_file`, the time zone, the canvas and pan elements, and background tools; version 5 added `annotate` and notices with controls). A plugin built with a newer SDK than the notchbase running it is refused with "it needs a newer notchbase". The SDK stamps its own version into every tool it builds, so a tool built with SDK 0.4.0 needs a notchbase that speaks protocol 5 even if it uses none of what 5 added (see [Protocol and ABI](Protocol-and-ABI.md#what-version-5-means-for-older-hosts));
- the shape of the JSON messages, which the SDK takes care of. Build against the SDK from the notchbase version you're targeting.

## Sharing a plugin

A tool is shared as a file. Zip the folder:

```
snippets/
  snippets.dll
  libsnippets.dylib
  README.md
```

People unzip it into their `tools` folder and restart notchbase.

### macOS notes

- Build for both Apple Silicon and Intel and merge the two with `lipo`, so one `.dylib` works on every Mac:

  ```sh
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output libsnippets.dylib \
    target/aarch64-apple-darwin/release/libsnippets.dylib \
    target/x86_64-apple-darwin/release/libsnippets.dylib
  ```

- Files downloaded from the internet are quarantined by macOS, which can stop the library from loading. After unzipping, the user can clear the flag with `xattr -dr com.apple.quarantine <folder>`, or you can sign and notarize the library.
- The default `tools` folder is inside `notchbase.app`. If you'd rather not modify the app bundle, point `NOTCHBASE_TOOLS_DIR` at a folder elsewhere.

## Bundling a tool into notchbase

For tools that should ship with notchbase itself:

1. Put the crate in `tools/<name>`. The workspace already includes everything under `tools/`.
2. Depend on the SDK by path: `notchbase-sdk = { path = "../../sdk" }`.
3. Add the crate to notchbase's own `[dependencies]`: `<name> = { path = "tools/<name>" }`.
4. Add `boxed::<your_crate::YourTool>` to the host's list of bundled tools (currently `all()` in `src/tools/bundled.rs`). `notchbase_sdk::boxed::<T>` turns a tool type into a constructor that takes a `Waker` and returns a `Box<dyn Driver>`.
5. Add tests (see [Testing](Testing.md)) and run `cargo test -p <name>`.

Bundled tools don't need `export!` or `crate-type`, but keeping them costs nothing and lets the same crate be built as a plugin. The built-in tools do this.

## Checklist before you share

- [ ] The manifest id is unique and final, and so are your shortcut ids and setting keys
- [ ] `version(env!("CARGO_PKG_VERSION"))` is set
- [ ] The library ends with `notchbase_sdk::export!(…)`
- [ ] Nothing slow runs in `update` or the views
- [ ] Settings you read are clamped and validated
- [ ] Unit tests cover the main flows
- [ ] Release profile doesn't use `panic = "abort"`
- [ ] Builds for each platform you support, with the file names Cargo gave them
