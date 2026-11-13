# Platform: Windows

macOS is the primary target, but Windows must work fully: every feature ships on both. This page collects everything Windows-specific: how the notch window is set up, how input and focus work, and which Windows APIs each feature relies on. Most of the window code is in `src/platform/windows`; feature-specific code lives in each module's `windows` folder.

The crate uses both `windows-sys` (thin, for Win32 window and GDI calls) and `windows` (COM and WinRT: Core Audio, Media Foundation, DXGI, Shell, SMTC).

## Process basics

- **No console in release.** `main.rs` sets `windows_subsystem = "windows"` for release builds. Debug builds keep the console so `eprintln!` output is visible.
- **Ctrl+C.** `platform::on_console_quit` installs a console control handler for Ctrl+C, Ctrl+Break and closing the console window. It asks the event loop to quit, gives the app 1.5 seconds to shut down (dropping the app releases held assistant prompts, removes `assistant.json` and finishes a recording), then terminates the process. After a normal quit, `exit_now` terminates the process as soon as the app has been dropped, without waiting for background threads.
- **Data folder**: `%APPDATA%\notchbase` (override with `NOTCHBASE_DATA_DIR`).
- **Configuration and plugins**: `config.json` and the `tools` folder sit next to `notchbase.exe` (override with `NOTCHBASE_CONFIG` and `NOTCHBASE_TOOLS_DIR`). Plugins are `.dll` files, one folder per tool.
- **Known folders** (`platform/windows/system.rs`): `SHGetKnownFolderPath` for Profile, Pictures, Videos, Desktop and Downloads, falling back to `%USERPROFILE%\<name>`. Recordings go to Videos and saved screenshots to `Pictures\Screenshots`, unless a tool sets another folder. Tools receive these folders in their `Start`.
- **Opening things**: `ShellExecuteW` with the `open` verb.
- **Local time**: `GetLocalTime` (used for recording file names).
- **Modifier names** for shortcuts: Ctrl, Alt, Shift, Win.

## COM

Many features use COM. The rule is: **each thread that uses COM joins an apartment itself** and leaves it when done. `platform::Apartment` (re-exported from the `notchbase-wasapi` crate, `crates/wasapi/src/apartment.rs`) joins the multithreaded apartment and uninitialises on drop only if it joined. The media source, the volume watcher's output, and the audio worker hold one for their lifetime; the recorder and the shelf's video code initialise COM (and Media Foundation) for the duration of a job. File drag-out initialises OLE on the UI thread for the duration of the drag, and shell thumbnails make sure the calling thread has an apartment.

## The notch window

`platform/windows/window.rs` and `window_style.rs`.

- **Attributes**: undecorated, transparent, not resizable, always on top, not in the taskbar, no DWM shadow (`with_undecorated_shadow(false)`), no Windows 11 rounded corners (`CornerPreference::DoNotRound`), and created inactive so opening it never steals focus.
- **Extended styles** (applied after creation and re-applied on every placement): `WS_EX_TOOLWINDOW` (no taskbar button, no Alt+Tab entry), `WS_EX_NOACTIVATE` (clicking it does not activate it), `WS_EX_LAYERED`, and `WS_EX_APPWINDOW` removed. When a window first becomes layered, `SetLayeredWindowAttributes` sets it fully opaque so per-pixel alpha from GL still applies. Style changes go through `SetWindowPos(SWP_FRAMECHANGED)` without moving, sizing, reordering or activating.
- **Placement**: the 680 x 600 point canvas is converted to physical pixels with the monitor's scale factor and centered at the top of the monitor in physical coordinates, so it is exact on every DPI and on monitors placed left of or above the primary one. Position and size are only set when they changed.
- **Style**: Windows displays have no hardware notch (`hardware_notch` returns `None`), so every Windows notch uses the Island style.

### Click-through

`set_click_through` toggles `WS_EX_TRANSPARENT` (with `WS_EX_LAYERED`): when set, mouse input passes straight through the window to whatever is below. The app flips it every frame depending on whether the cursor is over the notch's shape (see [Architecture](Architecture.md#input-click-through-and-keyboard-focus)). While a shelf file is dragged out, hit testing is suspended entirely so the notch window is not a drop target for its own drag.

### Cursor and modifiers

- `cursor_in_window` uses `GetCursorPos` and converts to the window's logical coordinates with its scale factor. The app polls it, because a click-through window gets no mouse events.
- `held_modifiers` reads `GetAsyncKeyState` for Alt, Ctrl, Shift and the Windows keys.

### Keyboard focus

Because the notch window is `WS_EX_NOACTIVATE`, it never takes focus when clicked, so the app the user was in keeps it. When the notch needs typing (recording a shortcut, a tool's text input), `begin_keyboard_focus` removes `WS_EX_NOACTIVATE` and focuses the window; `end_keyboard_focus` restores it. `remember_front_app` / `restore_front_app` save and restore the foreground window (`GetForegroundWindow` / `SetForegroundWindow`) around captures, shortcut rebinding and typing into a tool's text field. While the notch holds the keyboard it is also pinned open (see [Architecture](Architecture.md#input-click-through-and-keyboard-focus)).

## Rendering and pacing

GL windows swap with vsync disabled, and after each presented frame the event loop sleeps until the display's next vertical blank, read from the compositor's timing (`DwmGetCompositionTimingInfo`), and never longer than two frames (`platform/windows/pacing.rs`). This keeps animations in step with the display across several windows. `DwmFlush` is not used: it waits for the next composition, and on some high-refresh and variable-refresh setups that only comes when something else on screen changes, which froze the notch between mouse moves. `setup_event_loop` also calls `SetProcessInformation(ProcessPowerThrottling)` to opt out of EcoQoS and timer-resolution throttling, since Windows 11 otherwise treats a notch that never takes focus as a background app. See [Rendering and Motion](Rendering-and-Motion.md#the-gl-window).

**Fonts**: Segoe UI and Segoe UI Semibold from `%WINDIR%\Fonts`, and Cascadia Mono (weight 600), then Consolas Bold, then Consolas for monospace.

## Overlays and capture exclusion

Capture overlays (the region picker, the area picker) and the recording dim are `WS_EX_TOOLWINDOW` windows. The pickers are one physical pixel taller than the monitor. Both kinds call `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` so they never appear in screenshots or recordings; the color loupe does the same so it can sit on top of the pixels it magnifies. Set `NOTCHBASE_OVERLAY_IN_CAPTURES=1` or `NOTCHBASE_LOUPE_IN_CAPTURES=1` to let them be captured, for documentation screenshots. `raise_window` re-asserts `HWND_TOPMOST` without activating, used to keep the notches above the dim.

## Hooks

Three features install **low-level hooks**, each on its own thread with its own message loop, because a low-level hook is called on the thread that installed it and that thread must keep pumping messages:

- the color loupe installs mouse and keyboard hooks (`WH_MOUSE_LL`, `WH_KEYBOARD_LL`) while it is open, to take clicks, the wheel and keys without them reaching other apps (see [Screen Capture](Screen-Capture.md#windows-a-custom-loupe));
- the volume indicator installs a keyboard hook for the volume and media keys (see [Volume and Media Keys](Volume-and-Media-Keys.md#windows-systemvolumewindows));
- the system's search key installs a keyboard hook for the Windows keys while **Replace the Start menu** is on (see [Find](Find.md#the-systems-search-key)).

Hooks are removed by posting `WM_QUIT` to the hook thread and joining it.

**No raw input.** While a process is registered for raw keyboard input, Windows does not call that process's low-level keyboard hooks for keys typed into its own windows; the hooks only hear the keys other apps get. `winit` registers every event loop for raw mouse and keyboard input (`DeviceEvents::WhenFocused`), and notchbase reads no device events, so `let_hooks_hear_our_windows` (`platform/windows/system.rs`, called in `app::run` as soon as the event loop is built) asks for none (`DeviceEvents::Never`, which removes the registration with `RIDEV_REMOVE`). Without it every keyboard hook goes deaf whenever a notchbase window has the keyboard: up to 0.19.1 a Windows key pressed with Find open opened Start instead of closing Find, and the volume keys' hook is under the same rule. Nothing else in the process may register raw keyboard input, and a feature that ever needs device events has to find another way to keep the hooks hearing.

Global shortcuts for tools use the `global-hotkey` crate (`RegisterHotKey` under the hood). During the area picker and the region picker, Escape is registered as a temporary hotkey so it cancels even without focus.

## Feature by feature

| Feature | Windows APIs | Page |
| --- | --- | --- |
| Per-app volume, devices | Core Audio: `IMMDeviceEnumerator`, `IAudioSessionManager2`, `ISimpleAudioVolume`, `IAudioEndpointVolume`, `IAudioMeterInformation`; `IPolicyConfig` to set the default device; version resources and `SHDefExtractIconW` for app names and icons | [Sound Mixer](Sound-Mixer.md) |
| Now playing | WinRT `GlobalSystemMediaTransportControlsSessionManager` | [Media and Music](Media-and-Music.md) |
| Music levels | WASAPI loopback capture | [Media and Music](Media-and-Music.md) |
| Volume and media keys | `WH_KEYBOARD_LL`, `IAudioEndpointVolume` | [Volume and Media Keys](Volume-and-Media-Keys.md) |
| Screenshots | DXGI Desktop Duplication, GDI `BitBlt` fallback | [Screen Capture](Screen-Capture.md) |
| Color picker | GDI `BitBlt`, low-level hooks, `WDA_EXCLUDEFROMCAPTURE` | [Screen Capture](Screen-Capture.md) |
| Screen recording | DXGI Desktop Duplication (GDI fallback), WASAPI capture and loopback, Media Foundation sink writer (H.264 + AAC) | [Screen Recording](Screen-Recording.md) |
| Shelf video and audio | Media Foundation source reader and sink writer | [Shelf](Shelf.md) |
| Shelf thumbnails, reveal, drag-out | `IShellItemImageFactory`, `SHOpenFolderAndSelectItems`, `SHDoDragDrop` with a shell data object and `IDragSourceHelper` | [Shelf](Shelf.md) |
| Drops from browsers and apps | notchbase's own `IDropTarget` (winit's drag and drop is off on the notch window), `IDropTargetHelper`, `CF_HDROP`, `FileGroupDescriptorW`/`FileContents`, URL formats | [Shelf](Shelf.md#dragging-things-in) |
| Bluetooth | SetupAPI device nodes and device properties | [Bluetooth](Bluetooth.md) |
| Battery, keep awake | `GetSystemPowerStatus`, `SetThreadExecutionState` | [System tools](System-Tools.md) |
| Camera and microphone use | the capability consent store in the registry; `IAudioEndpointVolume` on the default capture endpoint | [System tools](System-Tools.md#camera--microphone) |
| Screen lock | `WTSQuerySessionInformationW` | [System tools](System-Tools.md#the-lock-screen) |
| Focus | the quiet-hours state through `NtQueryWnfStateData` (undocumented) | [System tools](System-Tools.md#focus) |
| GPU usage, name | PDH GPU Engine counters, DXGI adapter description | [Stats](Stats.md) |
| Disk | `GetDiskFreeSpaceExW` | [Stats](Stats.md) |
| Assistant token | `CoCreateGuid` for randomness | [Assistant](Assistant.md) |

## Things that differ from macOS

- **Island only.** No hardware notch, so no Notch style.
- **Screenshots** use notchbase's own region picker instead of the system tool.
- **Color picking** uses notchbase's own loupe instead of the system sampler.
- **Recordings** are `.mp4` (H.264/AAC through Media Foundation) instead of `.mov` through ScreenCaptureKit, and the pipeline is notchbase's own code.
- **No permissions** are needed for screen capture, audio capture or Bluetooth.
- **Focus** never moves to the notch on click; on macOS it does, and focus is handed back afterwards.
- **HEIC, HEIF and AVIF** are not treated as images on the shelf (macOS decodes them with `sips`).
- **Volume steps** are 2% with no fine step.

## Testing on Windows

Most tests run on Windows, including tests that talk to real Core Audio devices, Media Foundation and the shell without assuming particular hardware. Tests that touch the screen, the real audio output, Bluetooth or show windows are `#[ignore]`d; see [Building and Testing](Building-and-Testing.md#ignored-tests). Use `cargo check --target aarch64-apple-darwin` to make sure macOS code still compiles when working on Windows.
