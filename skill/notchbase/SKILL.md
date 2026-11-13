---
name: notchbase
description: Everything about notchbase, the fluid notch / Dynamic Island app for macOS and Windows. Use it to answer any question about using, setting up, configuring or troubleshooting notchbase (config.json and every setting in it, where files live, tabs, tools, the calendar, the assistant integration with Claude Code and Codex, updates, installing, permissions, platform differences and quirks), to build, test and install notchbase tools (plugins) with its Rust SDK, even for people who have never written Rust.
---

# notchbase

notchbase is a small black shape at the top of the screen that grows when you hover it and opens into a panel: a music player with the volume under it and a per-app sound mixer one click away, a grid of tools (color picker, screenshots, screen recording, timer, alerts), a shelf for files, hardware and AI-plan stats, and a calendar. On a Mac with a notch it wraps the hardware notch; elsewhere it's a floating island. Everything it remembers is in one `config.json`. It updates itself, and anyone can add tools to it as plugins written in Rust.

This skill does two jobs:

1. **Answer questions** about notchbase: how something works, where a setting is, why something behaves the way it does, how to change it. Follow [Answering questions](#answering-questions).
2. **Build tools**: new tiles, pages, live activities or plugins. Read `tools.md` in this skill and follow it from the start.

## Answering questions

- **Look it up, don't recall it.** Find the topic in the map below and read that reference before answering. The references are the project's own documentation, kept in step with the code; the quirks list at the end of this file covers what people trip over most.
- **Be exact.** Quote keys (`notch.hover_delay_ms`), values, defaults, paths and menu names (**Settings › Tools**) exactly as the references give them. If the answer differs between Windows and macOS, say which is which.
- **Give something to paste.** For a `config.json` change, show a partial file with just the keys involved; partial files are valid and the rest keeps its defaults. Say whether it takes effect straight away (almost everything does, within a second) or restarts the tools.
- **Prefer the notch.** If a setting is on the Settings page or a tile, say where, and mention the file as the alternative.
- **Keep it short** unless they ask how something works inside. Skip source file names for users; include them for contributors.
- **Say when you don't know.** If the references don't cover it, say so plainly rather than guess, and point to what they do say.

## Where to look

| Question about | Read |
| --- | --- |
| What notchbase is, the feature list | `references/Home.md` |
| `config.json`: every key, default, built-in tool setting, environment variable | `references/Configuration.md` |
| The tabs and pages: Media (player and mixer), Tools, Shelf, Stats, Calendar, the optional Sound tab, Settings, notifications, reordering tabs | `references/Pages.md` |
| How the notch opens, closes, peeks, scrolls and switches tabs; activities, notices, the header | `references/Notch.md` |
| Installing, the installer, uninstalling, open at login, staying on top | `references/Installing.md` |
| Automatic updates, publishing releases | `references/Updates.md` |
| The Calendar: iCal links, Apple Calendar, Days and Month views, events, colors, links, join buttons | `references/Calendar.md` |
| Claude Code and Codex prompts and progress in the notch, the hooks it installs | `references/Assistant.md` |
| The Stats tab, Claude and ChatGPT plan usage, "Expired" | `references/Stats.md` |
| The Alerts webhook: sending notifications from scripts | `references/Alerts.md` |
| The Shelf: dropping files, converting, compressing, PDFs, dragging out | `references/Shelf.md` |
| Per-app volume, devices | `references/Sound-Mixer.md` |
| Volume and media keys, the volume indicator | `references/Volume-and-Media-Keys.md` |
| Now playing, the dancing edge | `references/Media-and-Music.md` |
| Bluetooth device notices and batteries | `references/Bluetooth.md` |
| Battery and charger alerts, the camera and microphone dots, downloads in the notch, Focus, Keep Awake | `references/System-Tools.md` |
| The notch on the lock screen, the unlock padlock, making room for menus, haptics | `references/Notch.md` |
| Color picker and screenshots | `references/Screen-Capture.md` |
| Drawing on a screenshot: the Annotate button, the annotator window, the brush and its keys, the clipboard | `references/Annotate.md` |
| Screen recording | `references/Screen-Recording.md` |
| Windows specifics | `references/Platform-Windows.md` |
| macOS specifics, permissions, the app bundle | `references/Platform-macOS.md` |
| Building notchbase from source, running tests | `references/Building-and-Testing.md` |
| How the code is organised, threads, data flow | `references/Architecture.md` |
| Drawing, springs, motion | `references/Rendering-and-Motion.md` |
| Pinning the notch in a state for screenshots and testing | `references/Previews.md` |
| Contributing code | `references/Contributing.md` |
| Writing tools: the SDK, elements, commands, subscriptions, settings, testing, plugins | `tools.md`, then `references/tools/` |

## Where things live

| What | Windows | macOS |
| --- | --- | --- |
| The app | `%LOCALAPPDATA%\Programs\notchbase\notchbase.exe` | `/Applications/notchbase.app` (or `~/Applications`) |
| `config.json` | next to `notchbase.exe` | `notchbase.app/Contents/MacOS/config.json` |
| Data folder (shelf, assistant data, each tool's own files in `tools/<id>`) | `%APPDATA%\notchbase` | `~/Library/Application Support/notchbase` |
| Plugins (a folder each; `<crate>` is the package name with `-` as `_`). A portable tool is `<crate>.wasm` on both | `%APPDATA%\notchbase\tools\<id>\<crate>.dll` | `~/Library/Application Support/notchbase/tools/<id>/lib<crate>.dylib` |

**Settings › Tools** shows the paths of `config.json` and the tools folder, each with an Open button. `NOTCHBASE_CONFIG`, `NOTCHBASE_DATA_DIR` and `NOTCHBASE_TOOLS_DIR` move them (see Configuration).

## Quirks worth knowing

**config.json**
- It lives next to the app, not in the data folder. A checkout run with `cargo run` has its own at `target/debug/config.json` (or `target/release/`), separate from the installed app's.
- notchbase rereads it about once a second, so edits apply while it runs. Changing `layout.tools` (or dragging an icon on the Tools tab, which rewrites it) reorders tools without restarting them. Changing any tool's `enabled` restarts all tools, which lose anything they only kept in memory.
- A typo never costs you the file: invalid JSON or a wrong type shows a message with the line number in the notch, and notchbase keeps the last good settings. Until it's fixed nothing is saved, including changes made in the notch.
- When notchbase saves (for example after a shortcut is rebound in the notch) it rewrites the whole file, dropping keys it doesn't know. JSON has no comments, so notes in the file won't survive.
- A shortcut set to `null` is deliberately unbound and stays unbound. A missing one means the default. macOS writes `Option` where Windows writes `Alt`.
- Some tool settings aren't on the Settings page. The screenshot `folder` and `format`, the Alerts `token`, the recorder's `folder`, the timer's `last_ms` and the calendar's `setup_skipped` and `view` are only in the file; the recorder's microphone and desktop audio choices are on its tile.
- Floors: `bluetooth.poll_ms` and `stats.refresh_ms` never go below 250, `media.poll_ms` never below 100.
- A pre-version-2 file with the old `notch.hover_delay_ms` of 300 was moved to 120 once; any other value is kept.

**Tabs and tools**
- `layout.tabs` orders the header: the built-in keys (`media`, `tools`, `shelf`, `sound`, `stats`) and the ids of tools that have a header tab, such as `calendar`. The default is `media`, `tools`, `shelf`, `stats` (Calendar follows on its own); the mixer is on the Media tab (one Output slider row, with the device pickers, Input and Apps behind the chevron at its end), and adding `sound` gives it a tab of its own. Leaving out a built-in key hides it. A tool with a header tab that isn't listed still gets a tab, at the end; to hide the Calendar tab, turn the tool off (`tools.calendar.enabled: false`).
- `notch.start_page` can be any tab's key, `calendar` included; if that tab isn't shown it falls back to Tools (`sound` without a Sound tab opens Media).
- Settings is never a tab: it's behind the gear. Each tool's settings are on their own page, from the cog beside the tool in **Settings › Tools** (or **Settings › Indicators** for background tools such as Battery, Camera & Microphone, Downloads, Focus and Alerts), and that page keeps the notch open while the pointer is away so a link can be pasted from elsewhere.
- Plugins reload by themselves about a second after their file changes ("Reloaded" and the name of the tool's folder in the notch). There's also **Reload tools** in **Settings › Tools**, where a tool that failed to load or stopped has the reason in yellow under its name.

**The notch**
- Windows has no hardware notch, so it's always an island there. On a Mac with a notch, the notch style wraps it.
- The open notch closes when the pointer leaves, after `notch.close_delay_ms`, unless something holds it: typing, rebinding a shortcut, the calendar's event form, or a tool's settings page.
- On a trackpad, scrolling up and down scrolls pages, lists and picker wheels, sideways swipes move sliders, the shelf row and the calendar, and a sideways swipe elsewhere switches tabs. A mouse wheel drives whatever control is under the pointer. On Windows, touchpads and fine-scrolling wheels count as trackpads.
- The volume indicator replaces the system's volume pop-up while `volume.indicator` is on.
- After a screenshot, the notice shows an **Annotate** button for 5 seconds (it stays while the pointer is on the button or within a few points of it, not anywhere else on the notice). It opens the screenshot in its own window to draw on: five colours (keys 1 to 5), a size slider (`[` and `]`), undo and redo (Cmd/Ctrl+Z, with Shift to redo), and Esc, Enter, Cmd/Ctrl+W or **Done** to close. While a line is being drawn, Esc drops that line and keeps the window open, and the other ways of closing finish the line first; holding one of those keys down never closes the window by its repeats. About a quarter of a second after each change the annotated picture is copied to the clipboard again, and the top right of the window says "Copied" once it is. The drawing reaches the clipboard only: a file kept by **Save screenshots** stays as captured. The brush is remembered as `annotate.color` and `annotate.size` in `config.json`. A size in the file outside 2 to 32 is pulled into range, and one that is no usable number counts as 5. On a Mac the window floats above other apps' windows, since notchbase has no Dock icon to bring it back with, and it is allowed into the Space of a full-screen app.

**Calendar**
- Windows reads iCal links only (a Google, Outlook or iCloud "secret address"), and iCal calendars are read-only. On a Mac, Apple Calendar is the default and its events can be added, moved and deleted. Accounts are added in System Settings › Internet Accounts, not in notchbase.
- New events without Apple Calendar are kept by notchbase on this computer only (`tools/calendar/events.json` in the data folder).
- Event colors are notchbase's own (`colors.json`), since Apple Calendar only colors whole calendars; iCal events keep their calendar's color.
- A meeting link in the event's link, URL, location or notes gets a join button named for the service (Zoom, Meet, Teams, FaceTime…). notchbase can't create a FaceTime link: make it in FaceTime with Create Link and paste it.
- iCal links are refetched when the notch opens, when the Calendar comes up, every 15 seconds while it's on screen, and every minute otherwise.

**Assistant (Claude Code and Codex)**
- Turning on prompts or progress in the notch installs hooks into the assistant's own config: `settings.json` in `~/.claude` (or `CLAUDE_CONFIG_DIR`) for Claude Code, `hooks.json` in `~/.codex` (or `CODEX_HOME`) for Codex. Only if that folder exists, keeping a one-time backup `<file>.notchbase.bak` and never touching other hooks. Switching assistants or turning both off removes them.
- Codex asks you to trust new hooks: open Codex and run `/hooks`.
- Hooks point at the executable that installed them; after moving notchbase, toggle the setting off and on. If `NOTCHBASE_DATA_DIR` is set for the app, the assistant must see the same variable.
- The Claude usage card reads the Claude desktop app's history first, then asks Anthropic with Claude Code's login. "Expired" means that login has expired: sign in to Claude Code again. The Microsoft Store build of the Claude app keeps its files under `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude`, which notchbase also checks.

**macOS**
- A new install asks for its permissions in a walkthrough in the open notch, one per step (Accessibility, Bluetooth, Calendars, Downloads, Music and Spotify, System Audio, Screen Recording, Full Disk Access), and ends on **You're all set**; **Settings → Permissions** brings the steps back later. Background features never ask by themselves for anything the walkthrough put to you (`permissions.asked`); features you start still ask when first used (a screenshot, a recording, dragging a window, an app's volume, the Keychain for Claude usage). A denied one shows what to allow and where.
- Run it from `notchbase.app`: permissions belong to the app bundle, and a bare binary can't be installed.

**Installing, updating, uninstalling**
- The installer downloads the newest release every time it runs, so one installer build stays current.
- Updates are checked 3 seconds after start, every 3 minutes, and when the notch opens (at most once a minute). With `updates.automatic` on they install once the notch has been idle for 20 seconds. Updates keep `config.json` and the tools folder beside the app.
- Uninstalling removes the app's folder, but first copies the `config.json` and `tools` folder beside the app into `kept-settings` in the data folder, and installing again brings them back. The rest of the data folder (shelf, assistant data, tool data, plugins) stays. On macOS this needs `notchbase --uninstall`; dragging the app to the Trash takes `config.json` with it.

**Alerts**
- The webhook listens only on `127.0.0.1` (not `::1`), port `47832` by default. Any local program can send alerts unless a `token` is set, and the token is set in `config.json` only. Web pages can't reach it. To send from another machine, tunnel over SSH.
