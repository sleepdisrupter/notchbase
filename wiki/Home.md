# notchbase wiki

notchbase is a fluid notch / Dynamic Island for macOS and Windows. A small black shape sits at the top of the screen, grows when you hover it, and opens into a panel with a music player, a tool grid (color picker, region screenshots, screen recording, a countdown timer, webhook alerts and third-party plugin tools), a file shelf, a per-app sound mixer and system stats. It also shows live activities: volume changes, Bluetooth devices connecting, the song that is playing, tool notices and timers, questions from Claude Code or Codex that you can answer right in the notch, and a quiet indicator while they work.

It is a single Rust binary built on `winit` + `glutin` + `egui`, with almost everything painted by hand (custom shapes, glass, springs, glyphs). macOS (macOS 27 is the primary target) and Windows are both first-class: every feature has to work fully on both.

These pages explain how the app works so you can find your way around the code. File paths are relative to the repository root.

## Start here

- [Architecture](Architecture.md): the big picture. Process modes, workspace layout, module map, the `App`, the frame loop, how tools are loaded, fed and served, threads, and how data flows from services to pixels and back.
- [Building and Testing](Building-and-Testing.md): building, testing, linting, cross-checking the macOS target, bundling, environment variables.
- [Contributing](Contributing.md): code style (short comments, small modules, tests next to code), platform rules, review checklist.
- [Configuration](Configuration.md): `config.json`, every setting and how it is reloaded.
- Ask an AI: add the **notchbase skill** (`notchbase-skill.zip`, attached to every [release](https://github.com/sleepdisrupter/notchbase/releases/latest) and built by `scripts/build-skill.ps1` or `scripts/build-skill.sh`) to Claude or ChatGPT. It carries this whole wiki, answers questions about using and configuring notchbase, and builds tools.

## The notch and its UI

- [Notch](Notch.md): modes (Idle, Peek, Expanded, Activity, Drop, Ask), geometry and morph springs, island vs hardware-notch styles, attraction, lean and hug, activities and tool notices (sizes, shake, sticky), the tool live activity, music reveal and dance, dynamic tabs, the pager and the scrolling page panel, drop zone, ask prompts.
- [Rendering and Motion](Rendering-and-Motion.md): the GL window and frame pacing, the canvas, shapes, glass, springs, fonts, glyphs and controls.
- [Pages](Pages.md): the Media, Tools, tool, Shelf, Sound, Stats, What's New and Settings pages (including the installed-tools view and shortcut rebinding) and how pages talk to the app.
- [Previews](Previews.md): pinning the notch in any state with `NOTCHBASE_PREVIEW` for screenshots and visual checks.

## Features

- [Media and Music](Media-and-Music.md): now playing (Windows SMTC, macOS Music and Spotify), the spectrum analyzer that makes the notch dance, media controls.
- [Sound Mixer](Sound-Mixer.md): per-app volume. Windows WASAPI sessions, macOS Core Audio process taps.
- [Shelf](Shelf.md): the file shelf. Image, PDF, audio and video operations, the PNG optimizer, the worker thread, dragging files out, AirDrop on a Mac.
- [Video downloads](Video-Downloads.md): drag a browser tab onto the notch to download the video on it, or only its sound. How a tab is caught in every browser, which sites work, the download engine, the codecs that play, and the light under the notch.
- [Stats](Stats.md): CPU, GPU, memory and disk, plus Claude and ChatGPT (Codex) plan usage.
- [Assistant](Assistant.md): Claude Code and Codex hooks. The `notchbase hook` client, the local server, permission prompts and questions in the notch, installing hooks.
- [Screen Capture](Screen-Capture.md): the color sampler loupe, region screenshots, the area picker and the recording dim.
- [Annotate](Annotate.md): drawing and writing on a screenshot. The Annotate button in the Screenshot notice, the annotator window, the brush and the text tool, how strokes and words become pixels, and how every change is copied to the clipboard again.
- [Screen Recording](Screen-Recording.md): the recording pipeline. DXGI duplication and GDI fallback, the BGRA to NV12 converter, the Media Foundation H.264 writer and audio mixing on Windows; ScreenCaptureKit on macOS.
- [Bluetooth](Bluetooth.md): the connected-device tracker and the notch indicator.
- [Volume and Media Keys](Volume-and-Media-Keys.md): intercepting volume, brightness and media keys and the volume and brightness HUDs.
- [System tools](System-Tools.md): battery and charger alerts, the camera and microphone indicator with a mute button, browser downloads, the lock screen, and Keep Awake.
- [Alerts](Alerts.md): the bundled Alerts tool, which turns requests to a local webhook into notch notifications.
- [Dictation](Dictation.md): hold fn (Ctrl+Win on Windows) and talk. Groq's Whisper, the tidy-up, and the glossary that learns your words.
- [Calendar](Calendar.md): your next meeting as a live activity with a Join button, today's agenda, secret iCal links on every platform, and Apple Calendar (with adding and editing events) on macOS.
- [Installing](Installing.md): installing notchbase as an app, opening at login, uninstalling, the app icon.
- [Updates](Updates.md): automatic updates from GitHub releases, the What's New notes a release carries and shows once, and per-platform builds.

## Platforms

- [Platform: Windows](Platform-Windows.md): window styles, click-through, focus, DPI, COM, hooks and every Windows API the app relies on.
- [Platform: macOS](Platform-macOS.md): AppKit window setup, hardware notch detection, permissions, bundling and every macOS framework the app relies on.

## Tools

The Tools page is driven by a tool system: every tile is a tool written against an Elm-style SDK. Pick Color, Screenshot, Record Screen, Timer, Keep Awake, Alerts, Calendar and the other [system tools](System-Tools.md) are bundled crates; third-party tools are dynamic libraries loaded as plugins from the `tools` folder. [Architecture](Architecture.md#tools) covers the host side; these pages cover writing tools.

- [Tools overview](tools/Overview.md)
- [Getting started](tools/Getting-Started.md)
- [The Elm architecture](tools/Elm-Architecture.md)
- [Elements](tools/Elements.md)
- [Surfaces](tools/Surfaces.md)
- [Commands](tools/Commands.md)
- [Subscriptions](tools/Subscriptions.md)
- [Settings and shortcuts](tools/Settings-and-Shortcuts.md)
- [Plugins and distribution](tools/Plugins-and-Distribution.md)
- [Protocol and ABI](tools/Protocol-and-ABI.md)
- [Testing tools](tools/Testing.md)
- [Built-in tools](tools/Built-in-Tools.md)
- [Example: Snippets](tools/Example-Snippets.md)
