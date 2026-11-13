# Sound mixer

The mixer, on the Media page under now playing (one row with the Output slider shows there; the chevron at its end opens the device pickers, Input and Apps) and on the Sound page when `layout.tabs` lists `sound`, lets you set the volume of each app separately, mute apps, change the output and input volume, and pick the default output and input devices. `src/audio` is the engine behind it. It has one portable front end, a driver that decides when to do work, and one backend per platform: **WASAPI audio sessions** on Windows and **Core Audio process taps** on macOS.

The UI is described in [Pages](Pages.md#sound-pagesmixer). Its rows are drawn with the shared UI kit in the `notchbase-paint` crate (`crates/paint/src`, re-exported as `ui::*`): the glass slider from `ui::controls`, the liquid level meter from `ui::meter` and the device-form glyphs from `ui::glyph`. The audio engine itself stays in the app; only the WASAPI *capture* code that the recorder, the music visualizer and the installer share moved to `notchbase-wasapi` (see [Screen Recording](Screen-Recording.md#audio)). The mixer's Windows backend keeps its own COM apartment guard in `src/audio/windows/com.rs`.

## The model

`src/audio/mod.rs` defines what the UI sees:

- `AudioApp { id, name, volume, muted, peak, icon }` is one row: an app, or the output or input device. `volume` and `peak` are 0..1.
- `AppId` is a `u32`. Two ids are reserved: `MASTER` for the default output row and `INPUT` for the default input row. App ids are an FNV-1a hash of the app's **key** (lowercased), nudged out of the reserved range (`audio/keys.rs`). The key is the process image path on Windows and the owning bundle id on macOS, so an app keeps the same id across refreshes and restarts. Processes without a readable path or bundle get a `pid:<n>` key.
- `AudioDevice { id, name, kind, is_default, form }` is an entry in a device picker. `form` (speakers, headphones, display, microphone, other) picks the glyph.

### Grouping and sorting

Apps often play audio from several sessions or helper processes (a browser's tabs, an app's helper processes). `audio/grouping.rs` folds members with the same key into one row: the row's volume is the loudest member's, it is muted only if every member is muted, and its peak is the highest peak. Changes are applied to every member.

`audio/sorting.rs` orders rows: normal apps before system sounds, apps heard in the last 8 seconds before quiet ones (`audio/activity.rs`), then by name. Devices are sorted by name.

## `Mixer` and the driver

`audio::Mixer` (`audio/mixer.rs`) is what the app owns. It keeps the latest `Snapshot` (apps, master, input, device lists, a supported flag and an optional reason) and exposes simple methods: `apps()`, `master()`, `input()`, `devices(kind)`, `set_volume(id, v)`, `set_muted(id, m)`, `set_default_device(kind, id)`, `refresh()`, `poll()`.

Setters are **optimistic**: the snapshot is updated immediately so the slider does not snap back, and a numbered command is sent to the driver. A snapshot from the backend is only accepted if its sequence number shows it already includes the last command, so a stale reading can never undo what the user just did.

The driver (`audio/driver`) decides when the backend does work:

- **Demand.** The app calls `refresh()` / `poll()` only while the Media, Sound or Tools page is visible (the Media and Sound pages show the mixer; the Record Screen tile lists microphones; the app feeds the mixer's input devices to tools that subscribe to audio inputs). The driver stays active for 1.5 seconds after the last demand. When nobody is looking, it does nothing.
- **Cadence** (`driver/cadence.rs`). While active, it updates level meters every 33 ms and re-enumerates apps and devices once a second.
- **Meters** (`driver/meters.rs`). Raw peaks are smoothed with a fast attack (25 ms) and a gentle release (300 ms), with a small silence floor, so the level meters look alive but not jittery.
- **Publishing** (`driver/publisher.rs`, `driver/snapshot.rs`). A new snapshot is only published when something **visibly** differs (volume by more than 0.0001, peak by more than 0.006, names, icons, devices, or a new command), which keeps repaints to a minimum.

There are two driver implementations with the same API:

- **Windows: `Worker`** runs the backend on the `notchbase-audio` thread, which joins a COM multithreaded apartment. Commands arrive over a channel. The worker sleeps until woken by a command or demand, and publishes snapshots into a mutex the UI takes from. A panic in the worker publishes an "unsupported" snapshot with the reason "Volume control stopped unexpectedly" instead of crashing the app.
- **macOS: `Inline`** runs the backend on the main thread, called from the frame loop, because Core Audio's object model and the AppKit calls used for app names and icons are simplest to drive from one thread. `maintain()` refreshes once a second while any tap is active, so taps follow apps that start new processes even when the page is closed.

## Windows: WASAPI audio sessions

`src/audio/windows` uses the Windows Core Audio APIs through the `windows` crate.

**Devices** (`endpoints.rs`, `default_endpoint.rs`, `device_labels.rs`)

- `IMMDeviceEnumerator` lists active render and capture endpoints and finds the default console device for each.
- The default output and input rows use `IAudioEndpointVolume` for the master volume and mute and `IAudioMeterInformation` for the peak.
- Names come from `PKEY_Device_FriendlyName`, and the form factor from `PKEY_AudioEndpoint_FormFactor` (headphones, headset and handset map to headphones, display to display, and so on). Labels are cached per device id.
- **Changing the default device** uses the undocumented but long-stable `IPolicyConfig` COM interface (`policy_config.rs`), calling `SetDefaultEndpoint` for the console, multimedia and communications roles. The vtable is declared by hand, and a test pins the method to slot 13.

**Apps** (`sessions.rs`, `session.rs`, `processes.rs`, `app_labels.rs`, `app_names.rs`, `icons.rs`)

- On each refresh, `IAudioSessionManager2` enumerates the audio sessions on the default output device. Expired sessions and notchbase's own sessions are skipped.
- Each session is keyed by its process's full image path (`QueryFullProcessImageNameW`, lowercased). The audio engine itself (`audiodg.exe`) is not an app. The **System Sounds** session gets its own row, sorted last, with the icon the session provides.
- Volume and mute use `ISimpleAudioVolume` on every session in the group; peaks use each session's `IAudioMeterInformation`. Changes pass a notchbase event-context GUID so they can be told apart from other apps' changes.
- **Friendly names** come from the executable's version resource `FileDescription`, cleaned up by `friendly_app_name`: taglines after " - " are dropped, dotted names keep their meaningful last segment ("PowerToys.Peek.UI" becomes "Peek"), vendor prefixes and architecture suffixes are removed ("Microsoft Teams" becomes "Teams"), and very long descriptions fall back to the capitalised file stem.
- **Icons** are extracted with `SHDefExtractIconW` at 48 px and converted from BGRA plus mask into an RGBA image. Session icon paths such as `@%SystemRoot%\System32\AudioSrv.Dll,-203` are parsed and environment-expanded (`icon_location.rs`).

Everything runs on the worker thread inside a COM apartment. If COM cannot be joined, the backend reports "Windows audio services aren't available".

## macOS: Core Audio process taps

macOS has no per-app volume API. Since macOS 14.2, Core Audio can create **process taps**: a tap captures the audio of chosen processes and can mute it at the source. notchbase uses a tap per app whose volume is not at 100%, and plays the tapped audio back itself at the chosen gain. `src/audio/macos` and `src/audio/core_audio` implement this.

**Listing apps** (`processes.rs`, `apps.rs`, `app_labels.rs`, `app_icon.rs`)

- The system's audio process object list gives, for each process, its pid, bundle id and whether it is currently producing output.
- Each process is attributed to the app responsible for it (`responsibility_get_pid_responsible_for_pid`, looked up dynamically), and helper bundle ids are folded into their parent (`com.example.App.helper.Renderer` becomes `com.example.App`), so a browser and all its helpers are one row.
- An app is listed if it is producing output now, has an active tap, or was heard recently. Names and icons come from `NSRunningApplication`.

**Changing an app's volume** (`backend.rs`, `taps`)

1. The desired mix (volume, muted) is recorded for the app.
2. If the app already has a tap, only its gain changes (an atomic the audio callback reads).
3. Otherwise, if the mix is anything other than 100% and unmuted, a tap is created:
   - a `CATapDescription` for a **stereo mixdown** of the app's process objects, marked private, with the mute behaviour "muted when tapped" (so the app is silenced at the source while notchbase plays it), and, where supported, following the app's bundle ids with process restore enabled so new helper processes are covered;
   - `AudioHardwareCreateProcessTap` creates the tap;
   - a **private aggregate device** is created with the current default output as its main sub-device and the tap attached (auto-started, drift compensated);
   - an IO proc on the aggregate renders the tapped input to the output at the desired gain (`core_audio/render.rs`), ramping the gain across each buffer to avoid clicks, down-mixing to mono or spreading channels as needed, and measuring the peak for the meter.
4. When an app returns to 100% and unmuted, its tap and aggregate are destroyed, so the app plays through the system normally again.

Gain is `volume²` (0 when muted, `core_audio/gain.rs`), which feels more even across the slider than a linear gain.

Taps are kept in sync on every refresh: taps for apps that disappeared are closed; a tap is recreated if the app's processes or the output device changed. If creating a tap fails, the row snaps back to 100% and the app is not retried for 5 seconds. Taps require a float32 output device.

**Loading the API.** `AudioHardwareCreateProcessTap`, `AudioHardwareDestroyProcessTap` and the `CATapDescription` class are looked up at runtime (`taps/api.rs`). On systems older than macOS 14.2 they are missing and the page shows "Per-app volume needs macOS 14.2 or later".

**Permission.** Taps need the **System Audio Recording** permission. The backend checks it with the TCC preflight function for `kTCCServiceAudioCapture` (loaded from the private TCC framework) and, if denied, shows "Allow System Audio Recording for notchbase in System Settings" and closes all taps. The bundle's `Info.plist` provides `NSAudioCaptureUsageDescription`. See [Platform: macOS](Platform-macOS.md).

**Devices** (`devices.rs`, `endpoint.rs`, `master.rs`, `property.rs`, `selectors.rs`, `streams.rs`)

- Devices are read from the Core Audio hardware object: name, UID, transport type and stream count per direction. notchbase's own mixer aggregates (UIDs starting with `app.notchbase.mixer.`) are hidden.
- The form comes from the transport type (`core_audio/transport.rs`): built-in speakers, or headphones when the built-in output's data source is the headphone jack; Bluetooth as headphones; HDMI and DisplayPort as displays; AirPlay as speakers.
- Master and input volume use the device's virtual main volume, or the main volume element, or, failing both, the average of the left and right channels. Mute uses the device mute property.
- Setting the default output also sets the default **system** output (alerts and sound effects), so the two do not drift apart.

`audio/macos/master.rs` exposes the default output's volume and mute to the [volume key handler](Volume-and-Media-Keys.md), and `taps/listener.rs` provides the global tap that [media levels](Media-and-Music.md#capturing-the-output) listen to.

## Other platforms

`audio/other.rs` reports "Volume control isn't available on this system" and returns empty lists.

## Tests

- `audio/keys.rs`, `grouping.rs`, `sorting.rs`, `activity.rs`, `labels.rs` and `per_kind.rs` test the portable logic.
- `driver/meters.rs`, `driver/snapshot.rs` and the worker test smoothing, change detection and command sequencing.
- `core_audio` is compiled for tests on every platform, so the tap render path, gain curve, bundle id folding and transport mapping are tested on Windows too.
- The Windows backend has tests that enumerate the real devices and sessions without assuming any particular hardware, plus pure tests for name cleanup, icon location parsing, form factors and the `IPolicyConfig` vtable layout.
