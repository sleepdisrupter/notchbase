# Installing

notchbase is one executable. It can install itself as an ordinary app, with no installer to build.

## The installer (`setup/`)

`notchbase-setup` is the file to hand to people. It is a small web installer: it does not carry notchbase inside it, it reads the newest GitHub release's `latest.json` for the build that fits the computer it runs on, downloads it with the same code the updater uses (the `notchbase-release` crate: `Server`, `newest`, `platform` and `download`, on top of `notchbase-net`), checks its SHA-256 and runs the downloaded app with `--install`. So one installer build keeps installing the latest version forever.

It is drawn with notchbase's own UI kit, the `notchbase-paint` crate, so it looks like the open notch: the black squircle body (`shape`), glass cards (`glass`, with rows from `list`), the green switches from Settings (`switch`), the blue and glass buttons from the Ask prompts (`buttons`), the volume slider (`controls`) as its progress bar, and the same system fonts (`font`). While sound is playing its bottom edge dances like the notch (`setup/src/dance.rs`), using the same loopback capture (`notchbase-wasapi`), spectrum analyzer (`notchbase-spectrum`) and liquid wave (`liquid`, `wave`) (Windows only; on macOS it stays still rather than ask for audio permission). On Windows the compositor's frame and the one-pixel line winit leaves for its shadow are switched off (`setup/src/window.rs`).

| Page | Shows |
| --- | --- |
| Options | **Open at login**, **Automatic updates**, **Claude Code and Codex** (questions and progress in the notch) and, on Windows, **Desktop shortcut**; the newest version and where it installs; **Install** |
| Installed already | **Update** instead of **Install**, and **Uninstall** beside it |
| Uninstall | asks first ("Your shelf and settings stay"), then runs the installed app's `--uninstall`, which keeps `config.json` and the `tools` folder in the data directory |
| Installing / Uninstalling | what it is doing (downloading with the size, installing, starting) and a progress bar |
| Done / Something went wrong | a green check, or a red badge with the reason and **Try Again** |

The options become flags for `--install`: `--no-login`, `--no-updates`, `--no-assistant` and `--desktop` (`Choices` in `notchbase-release`'s `install` module, `crates/release/src/install/choices.rs`, which the app's `src/installer` and the installer both use). Turning updates or the assistant off writes that into the new copy's `config.json`.

Build it with the script for your OS. It installs from the repository named in `NOTCHBASE_RELEASES_REPO` at build time, or `sleepdisrupter/notchbase`:

- **Windows:** double-click `build-installer.bat`. It writes `dist/notchbase-setup.exe`.
- **macOS:** run `./build-installer.sh`. It builds a universal `Install notchbase.app` (Apple silicon and Intel) and packs it into `dist/notchbase-setup.dmg`. Set `NOTCHBASE_SIGN_IDENTITY` to sign with a Developer ID.

`NOTCHBASE_SETUP_SHOT=<file.png>` with `NOTCHBASE_SETUP_PAGE` (`welcome`, `installed`, `confirm`, `working`, `removing`, `done`, `failed`) renders a page to a PNG, with sample data, and quits.

## Installing

- **From the installer:** run `notchbase-setup` (see above).
- **From the source, on Windows:** double-click `install.bat`. It builds a release into `target/x86_64-pc-windows-msvc/release` and runs it with `--install`.
- **From a running copy:** Settings → **Install notchbase**. The row only appears while notchbase is running from somewhere other than its install location.
- **From a terminal:** `notchbase --install`.

Installing:

1. Stops any other running notchbase. Processes started as `notchbase hook` are left alone.
2. Copies the app to its home. If a copy is already there, it is moved aside first and put back if the copy fails.
3. Brings `config.json` and the `tools` folder along if the new home doesn't have them yet: first from the copy being installed, then from `kept-settings` in the data directory, where an earlier uninstall left them.
4. Registers the app, turns on **Open at login** and relaunches from the new home with `--installed <pid>`. The new copy waits for the old one to exit and then shows "notchbase is installed and opens at login", or, when it was installed over a copy that has not seen this version's notes, the [What's New](Updates.md#whats-new) notice in its place.

| | Windows | macOS |
| --- | --- | --- |
| Home | `%LOCALAPPDATA%\Programs\notchbase\notchbase.exe`, a per-user install with no admin prompt | `/Applications/notchbase.app`, or `~/Applications` when `/Applications` isn't writable |
| In the launcher | a Start menu shortcut, `notchbase.lnk` | Launchpad and Spotlight find it in Applications |
| Open at login | the `notchbase` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | `~/Library/LaunchAgents/app.notchbase.plist`, which opens the app through Launch Services |
| Uninstall | Settings → Apps → notchbase → Uninstall, which runs `notchbase.exe --uninstall` | run `notchbase --uninstall`, or drag the app to the Trash (which takes `config.json` inside the bundle with it) |

On macOS, install from `notchbase.app` (see [Platform: macOS](Platform-macOS.md#bundling-scriptsbundle-macossh)). A bare binary can't be installed.

## Uninstalling

`--uninstall` stops notchbase, turns off Open at login, removes the Start menu shortcut and the Apps entry, copies `config.json` and the `tools` folder from beside the app into `kept-settings` in the data directory (`%APPDATA%\notchbase` on Windows, `~/Library/Application Support/notchbase` on macOS, or `NOTCHBASE_DATA_DIR`), replacing what an earlier uninstall kept there, and then deletes the install folder once the process has exited. If they can't be kept, it stops and leaves the install folder in place. Your shelf, assistant data and tool data in the data directory stay put, and installing again brings the kept settings back.

## Open at login

Settings → **Open at login** works for any copy, installed or not. It always points the login item at the copy you flip it in.

## Staying on top

The notch is a topmost window. Windows lets other topmost windows, such as the taskbar, Task Manager or game overlays, cover it. So notchbase puts its notch back on top once a second, and again as soon as the pointer reaches it. It doesn't do this while the screenshot, color or area pickers are open, because those overlays have to stay above the notch.

## The icon

A black tile holding a pearl island with a drop hanging from it on a thread.

| File | Used for |
| --- | --- |
| `assets/icon/notchbase.ico` | the Windows executable, Start menu and Apps list. 16 to 32 px use a bolder drawing, with a thicker thread and a larger drop, so the mark stays readable |
| `assets/icon/notchbase.png`, `assets/icon/notchbase-macos.png` | 1024 px masters: full bleed, and the macOS grid with its shadow |
| `macos/notchbase.icns` | the app bundle (`CFBundleIconFile`), copied in by `scripts/bundle-macos.sh` |

`embed_icon` in the `notchbase-build` crate (`crates/build/src/icon.rs`, called from the root `build.rs` and `setup/build.rs`) turns the `.ico` into a Windows resource file and links it into the executable when building for the MSVC target. It needs no resource compiler.

## How it is built

| Piece | Where |
| --- | --- |
| The command line, install, uninstall and Open at login | `src/installer/mod.rs` |
| Where the running app is, swapping and copying, keeping settings across an uninstall | `src/installer/location.rs` |
| Relaunching, waiting for the old process, stopping others | `src/installer/handoff.rs` |
| Registry, Start menu shortcut, Apps list entry | `src/installer/windows/` |
| Applications folder and the launch agent | `src/installer/macos.rs` |
| The Settings rows | `src/pages/settings/app.rs`, `src/app/setup.rs` |
| The install contract shared with the installer: `--install` / `--uninstall`, `Choices`, destinations, the macOS bundle and the Windows uninstall key | `crates/release/src/install/` |

### The installer's own code (`setup/src`)

| Piece | Where |
| --- | --- |
| The window, its frame, page fades and the icon | `setup/src/app/mod.rs` |
| Which page is showing, keys (Enter takes the main way forward, Escape closes or steps back), the progress bar's pace | `setup/src/app/flow.rs` |
| Titles, the status line and the tile that names where the app goes | `setup/src/app/view.rs` |
| The pages | `setup/src/pages/` (`welcome`, `confirm`, `working`, `finish`) |
| Looking up the newest release, downloading, running the child `--install` / `--uninstall` | `setup/src/install/` (`lookup.rs`, `child.rs`, `windows.rs`, `macos.rs`) |
| Shared widgets (buttons, rows, glyphs, fonts) | `setup/src/widgets.rs` |
| The bottom-edge dance | `setup/src/dance.rs` |
| Rendering a page to PNG (`NOTCHBASE_SETUP_SHOT`) | `setup/src/shot.rs` |
| Windows frame and border tweaks | `setup/src/window.rs` |
