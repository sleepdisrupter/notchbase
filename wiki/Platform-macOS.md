# Platform: macOS

macOS is notchbase's primary platform (macOS 27 is the target release; the app bundle declares macOS 14.2 as the minimum). This page covers how the app presents itself on macOS, how its windows are configured, how the hardware notch is measured, which permissions it needs, and which frameworks each feature uses. Window code is in `src/platform/macos`; feature-specific code lives in each module's `macos` folder.

Objective-C APIs are called through the `objc2` family of crates (`objc2-app-kit`, `objc2-foundation`, `objc2-core-graphics`, `objc2-screen-capture-kit`, `objc2-av-foundation`, and others). A few private or newer symbols are looked up at runtime so the app still starts on systems that lack them.

## An accessory app

`setup_event_loop` (`platform/macos/system.rs`) configures the `winit` event loop with:

- **`ActivationPolicy::Accessory`**: no Dock icon and no menu bar of its own, like a menu bar utility;
- no default menu;
- no "activate ignoring other apps" at launch, so starting notchbase does not steal focus.

The bundle's `Info.plist` also sets `LSUIElement`, which has the same effect when launched from Finder.

Other system details:

- **Data folder**: `~/Library/Application Support/notchbase` (override with `NOTCHBASE_DATA_DIR`).
- **Configuration and plugins**: the app bundle keeps `config.json` in the data folder, `~/Library/Application Support/notchbase/`, because writing into `notchbase.app` breaks the seal of its code signature. A `config.json` that an older version left in `notchbase.app/Contents/MacOS/` is moved there on the first start. A bare binary (`cargo run`) keeps `config.json` next to the executable. The `tools` folder is read from the data folder and from next to the executable (override with `NOTCHBASE_CONFIG` and `NOTCHBASE_TOOLS_DIR`). Plugins are `.dylib` files, one folder per tool.
- **Known folders**: `~/Pictures` (saved screenshots go to `~/Pictures/Screenshots` unless a tool sets another folder), `~/Movies` (recordings), `~/Desktop`, `~/Downloads`. Tools receive these folders in their `Start`.
- **Opening things**: `/usr/bin/open`.
- **Modifier symbols** for shortcuts: ⌃ ⌥ ⇧ ⌘.
- **Quit**: there is no console handler; `exit_now` exits the process after the event loop returns.

## The notch window (`platform/macos/window.rs`)

`winit` creates the window (undecorated, transparent, not resizable, no shadow, inactive, accepting the first mouse click), and `configure_notch_window` then adjusts the underlying `NSWindow`:

- **Collection behaviour**: `CanJoinAllSpaces | Stationary | FullScreenAuxiliary | IgnoresCycle`, so the notch is on every Space, does not move with Mission Control, appears over full-screen apps, and is skipped by Cmd+` window cycling.
- **Level**: three levels above the main menu level, so it sits above the menu bar, where the physical notch is.
- Clear background, not opaque, no shadow, never hidden on deactivate, cannot be hidden or moved, and no open/close animation.
- **Color space**: sRGB, so colors (and picked colors) match other apps.
- **Frame**: the 680 x 600 point canvas, centered horizontally at the top of the screen's frame (in AppKit's bottom-left origin coordinates), rounded to whole points.

The recording dim uses the same setup one level lower (so the notch stays above it), covering the whole screen. Capture overlays use the screen saver level.

`raise_window` calls `orderFrontRegardless`.

### The blur behind the panel (`platform/macos/backdrop.rs`)

Each notch window gets a `Backdrop`: an `NSVisualEffectView` (blending behind the window, always active) added to the window's frame view *below* the content view, so it sits under the GL surface and shows through wherever the GL output is translucent. Its layer is masked by a `CAShapeLayer`, and every frame `NotchWindow::paint` hands it the outline the notch wants blurred (`Notch::backdrop`: the open panel, in canvas points, flipped into the layer's bottom-up coordinates). The view's own tint would turn the glass a flat dark grey, so every frame `clear_glass` walks its layers, hides every one but the backdrop layer and sets that layer's `gaussianBlur` filter to an 8 point radius (through `filters.gaussianBlur.inputRadius`, only if a filter of that name is there), leaving a clear, light blur. The path is set inside a `CATransaction` with actions disabled so it never animates on its own, and the view is hidden while there is nothing to blur. **Smoked panes.** A plain layer-backed view sits between the blur and the content view. For every glass pane the notch painted that frame (`erase::panes`, see [see-through glass](Rendering-and-Motion.md#glass)) it holds two `CAShapeLayer`s in the pane's outline, each filled with a grey and composited with `darkenBlendMode`, the mirror of the `lightenBlendMode` layer AppKit keeps inside the effect view. A darken layer of grey `g` at opacity `a` leaves every channel of the blur that is below `g` alone and pulls what is above it toward `g`, keeping `1 - a` of the difference. Two of them (`erase::SMOKE`: 0.20 at 50%, then 0.25 at 90%, each times the pane's strength) make a soft knee: whatever is darker than the first grey shows through a button exactly as it did without them, and a white window behind it comes out near 30% grey. They work per channel, so a bright color behind a button turns to a dark grey with a trace of its hue. The layers live in a view of their own because inside the effect view's masked layer the blend does not reach the backdrop (they paint as flat grey there) and `clear_glass` would hide them. They are pooled, their paths and opacities are set with actions disabled like the mask's, and a frame whose panes are the same as the last one's touches nothing. Windows has a no-op `Backdrop`. See [Notch](Notch.md#black-into-glass-notchframers-platformbackdrop).

### Click-through, cursor and focus (`platform/macos/interaction.rs`)

- `set_click_through` uses `winit`'s `set_cursor_hittest(false)`, which makes the window ignore mouse events.
- `cursor_in_window` reads `NSEvent.mouseLocation` and converts it to the window's top-left coordinates. The app polls it, because a window that ignores the mouse gets no mouse events.
- `held_modifiers` reads `NSEvent.modifierFlags`.
- `watch_presses_elsewhere` adds an `NSEvent` global monitor for left, right and other mouse-down events and returns a `PressesElsewhere` that removes it when dropped; `take_press` says whether one was heard since it was last asked. AppKit calls a global monitor only for events delivered to other apps, and mouse events need no permission. Find holds one while it is open, so a click on the menu bar or the Dock, which moves no focus, closes it too.
- Unlike Windows, clicking the notch **does** make notchbase the active app. So when the pointer enters the notch's input zone, the app remembers the frontmost app (`remember_front_app` looks at the frontmost and the menu-bar-owning application, skipping notchbase itself), and when the pointer leaves it hands activation back (`restore_front_app`), using `yieldActivationToApplication` plus `activateFromApplication` where available (the cooperative activation API) and `activateWithOptions` otherwise. The same mechanism restores focus after a capture.

## Measuring the hardware notch (`platform/macos/notch.rs`)

`hardware_notch(monitor)` finds the `NSScreen` for the monitor (by matching `NSScreenNumber` with the display id) and:

1. requires `safeAreaInsets.top > 0` (the screen has a region at the top that content should avoid);
2. reads `auxiliaryTopLeftArea` and `auxiliaryTopRightArea`, the usable strips of the menu bar on either side of the camera;
3. computes the notch as `screen width - left width - right width` wide and `safeAreaInsets.top` tall;
4. accepts it only if both side areas are non-empty and the notch is less than half the screen's width.

A screen with a measured notch gets the **Notch** style; every other screen gets the **Island** style (see [Notch](Notch.md#two-styles)). By default notchbase goes to the screen with a hardware notch, or to the primary screen if none has one.

## Rendering

GL windows swap with vsync, which paces animation; no extra pacing is needed. The GL config asks for transparency explicitly. **Fonts** are SF Pro (the variable `SFNS.ttf` at weights 400 and 590), SF Mono (`SFNSMono.ttf` at 560), with Helvetica and Menlo as fallbacks, all loaded from `/System/Library/Fonts`. See [Rendering and Motion](Rendering-and-Motion.md).

## Permissions

macOS gates much of what notchbase does behind privacy permissions. A new install asks for them one at a time in a walkthrough in the open notch (below); after that, background features never ask by themselves, and the rest are asked for when the feature is used. Each feature degrades with a clear message when it is denied.

### The first-launch walkthrough

On the first launch of a new install, the notch opens by itself on a **Welcome** page instead of macOS firing every permission dialog at once. It takes one permission per step, in this order, and leaves out any that is already on (or, for Music and Spotify, when neither app is open):

| Step | What it says it is for | What **Allow** does |
| --- | --- | --- |
| Accessibility | volume, brightness and the media keys | macOS's own Accessibility dialog the first time, System Settings after that |
| Bluetooth | headphones and their battery | touches `IOBluetoothDevice` once, which brings up the Bluetooth dialog |
| Calendars | the next meeting | `requestFullAccessToEvents` |
| Downloads Folder | downloads as they finish | lists `~/Downloads` once, which brings up the folder dialog |
| Music and Spotify | what's playing | `AEDeterminePermissionToAutomateTarget` for each open player |
| System Audio | the dance and per-app volume | opens a process tap until macOS has an answer (macOS 14.2+ only) |
| Screen Recording | screenshots and recordings | `CGRequestScreenCaptureAccess`, then the Screen Recording pane |
| Full Disk Access | Focus, and Find in every folder | opens the Full Disk Access pane (macOS never asks for it) |

Each step has **Allow** (or **Open Settings** once only System Settings can grant it) and **Not Now**. While the page is up notchbase checks twice a second, and once a second it also starts a copy of itself with `--permission-standings`, which prints every permission's answer and exits (`system/permissions/probe.rs`). That is needed because macOS keeps the first answer cached inside a running app for some permissions (seen with the Downloads folder, Screen Recording and Full Disk Access), while a fresh process always gets the current one. A step that turns on shows a green check and moves on by itself. A dot row shows where you are. **Skip** on the first page passes every step. The walkthrough ends on **You're all set**.

The panel stays open while you are in System Settings. If it is closed, the next open lands on the walkthrough again until it is finished. If macOS relaunches notchbase (Screen Recording's **Quit & Reopen**), it picks up at the first step not yet answered, without the hello.

Rules behind it (`src/system/permissions`):

- While `permissions.walkthrough` is `"pending"`, nothing in the background asks for anything: the volume and media keys, the Bluetooth indicator, the Calendar tool, the Downloads tool, Music and Spotify, and the Focus and Find Full Disk Access notices all wait for their permission instead of triggering a dialog.
- Every step the walkthrough puts to you (allowed or **Not Now**) is listed in `permissions.asked`. Background features never ask for those again; **Settings → Permissions** runs the same steps for everything still off, at any time.
- Features you start yourself still ask the moment you use them (a screenshot, a recording, dragging a window to the notch, an app's volume in the mixer).
- Copies that were set up before the walkthrough existed (a `config.json` older than version 6) never see it, and keep asking as they always did.

Windows has no permission dialogs at launch, so there the walkthrough is only the hello and **You're all set**.

| Permission | Needed for | How notchbase asks / checks |
| --- | --- | --- |
| **Screen Recording** | region screenshots, recording | `CGPreflightScreenCaptureAccess`; the first time `CGRequestScreenCaptureAccess`, later it opens the Screen Recording pane of System Settings and shows "Allow Screen Recording for notchbase in System Settings, then reopen it" (`platform/macos/capture.rs`) |
| **System Audio Recording** | per-app volume, music levels | TCC preflight for `kTCCServiceAudioCapture`; the mixer shows "Allow System Audio Recording for notchbase in System Settings" when denied. `NSAudioCaptureUsageDescription` |
| **Accessibility** | volume, brightness and media keys; reading other apps' menus to make room for them | `AXIsProcessTrusted`, prompting once with `AXIsProcessTrustedWithOptions` (for the keys; the menu reader never prompts) |
| **Full Disk Access** | the Focus tool | reading `~/Library/DoNotDisturb` fails until it's granted; notchbase asks once with a notch message and a Notifications entry that opens the pane, then skips Focus (see [System tools](System-Tools.md#focus)) |
| **Microphone** | recording with the microphone | ScreenCaptureKit's microphone capture. `NSMicrophoneUsageDescription` |
| **Bluetooth** | device indicator | IOBluetooth. `NSBluetoothAlwaysUsageDescription` |
| **Downloads folder** | the Downloads tool | macOS asks the first time the folder is listed. `NSDownloadsFolderUsageDescription`. Denied, no downloads are shown |
| **Automation** (Apple Events) | now playing in Music and Spotify | AppleScript via `osascript`. `NSAppleEventsUsageDescription` |
| **Keychain** | Claude usage | `/usr/bin/security find-generic-password`; if access is denied, notchbase stops asking until a manual refresh |

The usage description strings live in `macos/Info.plist`. Permissions are recorded against the app's bundle identity (`app.notchbase`), which is one reason to run notchbase from the bundle (below) rather than as a bare binary.

## Bundling (`scripts/bundle-macos.sh`)

```bash
./scripts/bundle-macos.sh
open target/release/notchbase.app
```

The script:

1. builds in release mode with `MACOSX_DEPLOYMENT_TARGET=14.2`;
2. creates `target/release/notchbase.app` with `Contents/MacOS/notchbase` and `Contents/Info.plist` (bundle id `app.notchbase`, `LSUIElement`, `LSMinimumSystemVersion` 14.2, high-resolution capable, and the usage descriptions above);
3. code-signs the bundle with the identity in `NOTCHBASE_SIGN_IDENTITY`. If that is not set it uses the `notchbase Local Signing` certificate when your keychain has it, and signs ad hoc (`-`) otherwise.

The script deletes the old bundle before building the new one, and with it any `tools` folder that was inside `Contents/MacOS/`. Keep plugins in the data folder's `tools`, or point `NOTCHBASE_TOOLS_DIR` elsewhere, if you rebuild often.

### Signing and permissions

macOS ties every privacy permission it grants (screen recording, input monitoring, microphone and the rest) to the app's code-signing identity. An ad hoc signature has no identity beyond the hash of that one build, so each new build is a stranger: System Settings still shows the old grants switched on, they no longer match, and the app keeps asking. `tccutil reset All app.notchbase` clears the stale grants.

A certificate gives the app a lasting identity. Signed with one, its designated requirement is `identifier "app.notchbase" and certificate leaf = H"<the certificate's SHA-1>"`, which stays the same from build to build, so permissions survive rebuilds and updates for as long as every build is signed with that same certificate. Releases are signed with the self-signed `notchbase Local Signing` certificate, whose SHA-1 is pinned in `macos/signing-certificate.sha1`. A self-signed certificate does nothing for Gatekeeper: opening a copy downloaded with a browser without a warning still needs a Developer ID and notarization.

macOS ties privacy grants to the app's code signature, and an ad hoc signature changes with every build, so a rebuilt bundle may be asked for permissions again. Signing with a stable identity (for example a local development certificate) avoids that.

## Feature by feature

| Feature | Frameworks and APIs | Page |
| --- | --- | --- |
| Per-app volume | Core Audio process taps (`CATapDescription`, `AudioHardwareCreateProcessTap`), private aggregate devices, IO procs; `NSRunningApplication` for names and icons | [Sound Mixer](Sound-Mixer.md) |
| Device volume and default devices | Core Audio hardware properties | [Sound Mixer](Sound-Mixer.md) |
| Now playing | AppleScript to Music and Spotify, distributed notifications, `ureq` for Spotify artwork | [Media and Music](Media-and-Music.md) |
| Music levels | a global Core Audio process tap | [Media and Music](Media-and-Music.md) |
| Volume and media keys | Quartz event tap on `NSSystemDefined` events | [Volume and Media Keys](Volume-and-Media-Keys.md) |
| Brightness keys | the same event tap; the private DisplayServices framework (`DisplayServicesGetBrightness`, `DisplayServicesSetBrightness`), loaded with `dlopen` | [Volume and Media Keys](Volume-and-Media-Keys.md#brightness-keys) |
| Color picker | `NSColorSampler` | [Screen Capture](Screen-Capture.md) |
| Region screenshots | notchbase's picker overlays, then `/usr/sbin/screencapture -x -R` for the chosen rectangle | [Screen Capture](Screen-Capture.md) |
| Drops from browsers and apps | the window delegate's dragging methods, `NSFilePromiseReceiver`, the drag pasteboard | [Shelf](Shelf.md#dragging-things-in) |
| Screen recording | ScreenCaptureKit `SCStream` + `SCRecordingOutput` | [Screen Recording](Screen-Recording.md) |
| Shelf images | `image` crate, plus `/usr/bin/sips` for HEIC, HEIF and AVIF | [Shelf](Shelf.md) |
| Shelf video | AVFoundation `AVAssetExportSession`, `AVAssetImageGenerator` | [Shelf](Shelf.md) |
| Shelf thumbnails, reveal, drag-out | Quick Look `QLThumbnailGenerator`, `NSWorkspace`, `NSDraggingSession`, the drag pasteboard | [Shelf](Shelf.md) |
| Bluetooth | IOBluetooth `IOBluetoothDevice` | [Bluetooth](Bluetooth.md) |
| Battery, keep awake | IOKit power sources (`IOPSCopyPowerSourcesInfo`) and power assertions (`IOPMAssertionCreateWithName`); while kept awake, `kPMSetClamshellSleepState` on `IOPMrootDomain` so the lid doesn't sleep the Mac (not public) | [System tools](System-Tools.md) |
| Camera and microphone use | CoreMediaIO device properties; Core Audio process objects | [System tools](System-Tools.md#camera--microphone) |
| Screen lock | `CGSessionCopyCurrentDictionary` | [System tools](System-Tools.md#the-lock-screen) |
| The notch on the lock screen | private SkyLight spaces, loaded with `dlopen` | [Notch](Notch.md#the-lock-screen) |
| Focus | the Focus database in `~/Library/DoNotDisturb/DB` | [System tools](System-Tools.md#focus) |
| Making room for menus | Accessibility: `AXMenuBar` of the frontmost app | [Notch](Notch.md#making-room-for-menus-notchmovesdodgers) |
| Haptics | `NSHapticFeedbackManager` | [Notch](Notch.md#motion) |
| GPU usage, disk | IOKit `IOAccelerator` statistics, `CFURL` volume capacity keys | [Stats](Stats.md) |
| Claude credentials | Keychain via `/usr/bin/security` | [Stats](Stats.md) |

## Working on macOS code from Windows

The macOS code must keep compiling even when you work on Windows. Cross-check it without a Mac:

```bash
rustup target add aarch64-apple-darwin
cargo check --target aarch64-apple-darwin --all-targets
cargo clippy --target aarch64-apple-darwin --all-targets -- -D warnings
```

`check` and `clippy` type-check everything without linking, so they need no Apple SDK. Running and linking still requires a Mac. See [Building and Testing](Building-and-Testing.md#cross-checking-the-macos-target).
