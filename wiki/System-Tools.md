# System tools

Bundled tools show what the computer itself is doing: the charger going in and out, the camera or microphone being used, a browser download, a Focus, and a switch that keeps the computer awake. The notch follows the screen lock itself. Each is an ordinary [tool](tools/Overview.md) fed by a small service in the app, so every one can be turned off under **Settings › Indicators** (Keep Awake, which has a tile, is under **Settings › Tools**), and plugins can subscribe to the same feeds.

| Tool | Id | Shows | Fed by |
| --- | --- | --- | --- |
| Battery | `battery` | notices: charger in or out, low battery, fully charged | `src/system/power` |
| Camera & Microphone | `camera-mic` | a live activity while either is in use, with a mute button | `src/system/sensors` |
| Downloads | `downloads` | a live activity per browser download, then a banner and the file on the Shelf | `src/system/downloads` |
| Keep Awake | `keep-awake` | a tile, a shortcut and a cup while it is on | `src/system/awake` |
| Focus | `focus-status` | notices: a Focus or Do Not Disturb turning on or off | `src/system/focus` |

The services only run while a tool asks for them: `app/system/mod.rs` starts each one when `Tools::wants_power`, `wants_screen_lock`, `wants_sensors` or `wants_downloads` turns true and drops it when the last subscriber goes, so a tool that is switched off costs nothing. They are skipped while a [preview](Previews.md) is running.

## Polling (`src/poll`)

`Poll<T>` is the shape the power and lock services share: a named thread that reads a value on an interval and publishes it only when it differs from the last one. `Poll::start(thread, every, open)` runs `open` on the new thread (so a reader may hold state that isn't `Send`) and `take()` hands the app the newest unseen reading. Dropping it stops the thread within 100 ms.

## Battery

`power::Battery` polls every 2 seconds on the `notchbase-power` thread and publishes a `Power { level, plugged, charging }`; `level` is `None` on a computer without a battery.

- **macOS** (`system/power/macos.rs`): IOKit's `IOPSCopyPowerSourcesInfo` and `IOPSCopyPowerSourcesList`, taking the first source whose type is `InternalBattery`. The level is current capacity over max capacity, `plugged` is the `AC Power` state.
- **Windows** (`system/power/windows.rs`): `GetSystemPowerStatus`. A battery flag of 128 (none) or 255 (unknown) means no level.

The tool (`tools/battery`) turns readings into notices. The first reading only sets the scene, so nothing shows at launch.

| Change | Notice |
| --- | --- |
| charger plugged in | a green bolt, "Charging" (or "Plugged In" when it isn't charging), the level and a ring |
| charger unplugged | a battery, "On Battery", the level and a ring |
| 20% and 10% on battery | "Low Battery" in orange, then red and shaking, for 6 seconds, once per threshold per discharge |
| 100% while plugged in | "Fully Charged", once per charge |

On a hardware notch the words give way to the icon; the level and ring stay. The ring is green, orange at 20% or below and red at 10% or below, like a [Bluetooth device's](Bluetooth.md#the-notch-indicator-notchactivitydevicers). Settings: `charger`, `low`, `full`, all on by default.

## Camera & Microphone

`sensors::SensorWatch` polls once a second on the `notchbase-sensors` thread and publishes `Sensors { camera, microphone, apps, muted }`.

- **macOS** (`system/sensors/macos`): the camera is on when any CoreMediaIO device reports `kCMIODevicePropertyDeviceIsRunningSomewhere` (`camera.rs`); macOS does not say which app. The microphone is in use when a Core Audio process object reports `kAudioProcessPropertyIsRunningInput` (`audio/macos/recording.rs`); each is traced to its responsible app, and processes that aren't apps (speech daemons) and notchbase itself are left out. `muted` is the default input device's mute. Neither needs a privacy permission.
- **Windows** (`system/sensors/windows`): the capability consent store in the registry (`HKCU\…\CapabilityAccessManager\ConsentStore\webcam` and `\microphone`). An app whose `LastUsedTimeStop` is 0 is using the device right now; packaged apps are named after their package, desktop apps after their executable (`system/sensors/names.rs`). `muted` is the default capture endpoint's `IAudioEndpointVolume` mute.

The tool (`tools/camera-mic`) shows a live activity while either is in use: a green dot for the camera and an orange dot for the microphone, like the system's own indicators, and the app's name (or "Camera", "Microphone"). Beside a hardware notch the name gives way to a camera and a microphone glyph. Hovering its [ear](Notch.md#several-activities-notchlivesidesrs) shows a microphone button that mutes and unmutes the input device (`Command::mute_microphone`, applied on the sensors thread) and a × that hides the indicator until both are off again. Settings: `camera`, `microphone`.

## Downloads

`downloads::Downloads` lists the Downloads folder once a second on the `notchbase-downloads` thread.

- `scan.rs` splits the folder into **unfinished** files, by the suffix a browser gives them (`.crdownload`, `.download`, `.part`, `.opdownload`, `.partial`), and everything else. A Safari `.download` bundle counts the file inside it, and on macOS its `Info.plist` gives the total size (`system/downloads/macos.rs`).
- On macOS a browser also publishes each download's progress for Finder, and `macos/subscription.rs` subscribes to the folder to hear it (`NSProgress.addSubscriberForFileURL`, whose blocks arrive on the main thread; the watcher subscribes after its first listing, because the first subscription in a process takes about half a second; the name and kind are read from the progress's `userInfo`, because `fileURL` and `fileOperationKind` stay empty on such a proxy until `userInfo` has been read). That is where the total size of a Chrome, Firefox or Zen download comes from. Chrome publishes the `.crdownload` file itself; Firefox publishes the name the file will land under (`movie.mp4`) while it writes `movie.Xy3kQ9ab.mp4.part`, eight random characters after the first dot, so `macos/published.rs` matches the two and the download is shown under its real name. A total smaller than what is already on disk is not a count of bytes and is left out. On Windows nothing says how big a download will be.
- `tracker.rs` turns listings into outcomes. Unfinished files that were already there at launch stay quiet until they grow. A file that grows is `Progress`; one that stops growing for 20 seconds is dropped until it moves again; one that disappears is `Finished` if the file it was becoming is there, and lost otherwise.
- `watch.rs` publishes `DownloadChange::Progress`, `Finished { path }` or `Cancelled`. Firefox names its `.part` files at random, so for those the newest file to land in the last 10 seconds is taken as the finished one.

The tool (`tools/downloads`) shows the first download as a live activity: a blue ring that fills when the total is known and a download glyph when it is not, beside the amount so far ("12.4 MB"), and "+1" when more are running. A finished download becomes a banner with its name and size and is added to the [Shelf](Shelf.md). When the file is a `.zip`, the activity then asks **Unzip?** for 30 seconds; hovering it shows **Unzip** and ×. Unzip runs `ditto -x -k` on a Mac and the built-in `tar -xf` on Windows (on a background thread, `tools/downloads/src/unzip.rs`) into a hidden staging folder next to the zip, then moves the result out like Archive Utility does: a zip holding a single item gives that item, anything else a folder named after the zip, numbered ("fonts 2") if the name is taken, with `__MACOSX` dropped. The zip itself stays. The result goes on the Shelf with an "Unzipped" banner, or a "Couldn't unzip" message if the archive is broken. Settings: `shelve`, `announce`, `offer_unzip`.

macOS asks once for access to the Downloads folder (`NSDownloadsFolderUsageDescription`); without it nothing is shown. A download that starts and finishes inside one second is never seen.

## The lock screen

The screen lock isn't a tool any more: the notch itself follows it (see [Notch](Notch.md#the-lock-screen)). `session::ScreenLock` polls five times a second on the `notchbase-session` thread while `notch.lock_screen` is on or a tool subscribes to `Subscription::screen_lock`.

- **macOS**: `CGSessionCopyCurrentDictionary`, whose `CGSSessionScreenIsLocked` key is present while the screen is locked.
- **Windows**: `WTSQuerySessionInformationW(WTSSessionInfoEx)`, whose session flags read `WTS_SESSIONSTATE_LOCK`.

Each change goes to every notch (`Notch::set_locked`) and to tools that subscribe. Locking also stops a shortcut being rebound, lets go of a tool's text field and cancels a capture.

## Keep Awake

`Command::keep_awake(true)` asks the host to hold the computer and its display awake; `false` lets go. `awake::Awake` keeps one assertion for as long as any tool is asking and releases it when the tools are reloaded.

- **macOS**: two IOKit power assertions, `PreventUserIdleDisplaySleep` and `PreventUserIdleSystemSleep`, which show in `pmset -g assertions` as "notchbase is keeping this Mac awake", and the lid switch below.
- **Windows**: `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)` on the main thread.

**The lid is a hinge.** A power assertion stops the idle timer, not the lid: a MacBook still starts to sleep the moment it is closed, which turns the display off, locks the screen and makes players pause. So for as long as the Mac is kept awake, `system/awake/macos.rs` (`Lid`) also tells the kernel's power root not to sleep when the lid closes, and tells it to sleep again when it lets go. Closing the lid is then meant to change nothing but what the hardware does by itself: the built-in display goes dark and the built-in microphone is cut. It is the switch `powerd` itself uses, `kPMSetClamshellSleepState` (selector 12) on a connection to `IOPMrootDomain`. It needs no password, and it is not a public API, so a macOS update could take it away.

- **`powerd` owns that switch too.** It writes it only when its own answer changes (an external display with the charger in, a system assertion that applies with the lid closed, a sleep that began), so most of the time it leaves ours alone, but each of those ends with `powerd` switching lid sleep back on. `Awake::renew` sets it again once a second (called every frame from `app/system/mod.rs`).
- **What renewing can't cover.** If `powerd` switches lid sleep back on while the lid is closed, the kernel starts to sleep in the same instant, about 4 ms in the log, far faster than the once-a-second renewal. Seen on 2026-10-09: the charger goes in with the lid shut, Apple's PowerChime briefly holds a `DisplayWake` assertion that applies with the lid closed; when its chime ends `powerd` re-evaluates (`EvaluateClamshell. Disable : 0`) and the Mac sleeps. A plug or unplug of the charger is the common trigger; the clamshell toggle alone cannot win this race.
- **The floor underneath, for a charger change.** The one switch nothing overrides is `pmset -a disablesleep 1`, which turns off all sleep, system-wide, until it is set back to `0`. It needs an administrator. When **Keep awake when the lid is closed** is on (`awake.hold_lid_closed`, default on; the switch is in Keep Awake's own settings, Settings › Tools › Keep Awake, on a Mac), the first time anything holds the Mac awake notchbase asks for an administrator through macOS's own authorization dialog and turns the switch on. The dialog shows notchbase's name and icon because the request comes from inside the process (`system/awake/macos/floor.rs`, `AuthorizationExecuteWithPrivileges`, deprecated since 10.7 but the only way a self-signed app without a paid Developer ID can run one command as root). It turns it back off when nothing holds the Mac awake any more. No standing privilege is kept: the authorization is created, used for the one command, and freed.
  - The prompt runs on its own thread so the notch never freezes, and reports back through `UserEvent::LidFloor`. Turning the switch **off** also needs root, so that asks too; a cancelled prompt is not asked again until the awake state changes (`System::sync_lid_floor`, debounced on `lid_tried`).
  - Turn the setting off and Keep Awake falls back to the clamshell toggle alone: no password, but a charger change can still sleep the Mac.
- **After a crash, or a force-quit while on.** Neither the clamshell toggle nor `disablesleep` is undone when the app goes away. notchbase clears the clamshell toggle when it lets go and once when it starts; `disablesleep` it only clears by turning Keep Awake off (which prompts). So if notchbase is force-quit or crashes while holding the Mac awake with the lid closed, the Mac stays awake until Keep Awake is toggled off again or `pmset -a disablesleep 0` is run by hand.
- **Heat.** Nothing watches the battery or the temperature: a closed Mac that is kept awake in a bag gets hot.

Windows decides what closing the lid does in its power plan, which notchbase does not change.

The tool (`tools/keep-awake`) is a tile on the Tools tab: press it to turn it on, press again to turn it off. `length` (**Stay awake for**) makes one press last 15 minutes to 4 hours instead of until you turn it off; a timed session counts down on the tile and ends with a quiet "Sleep allowed" notice. While it is on a cup sits in the notch (`in_notch`), with a × on its ear. The `toggle` shortcut has no default key.

## Focus

`focus::FocusWatch` polls every 2 seconds on the `notchbase-focus` thread while a tool subscribes to `Subscription::focus`.

- **macOS** (`system/focus/macos.rs`, `system/focus/parse.rs`): there is no public API for the current Focus, so notchbase reads the files Focus keeps, `~/Library/DoNotDisturb/DB/Assertions.json` (a Focus that is on has an assertion record naming its mode) and `ModeConfigurations.json` (the mode's name, such as "Work"). macOS protects that folder: it can only be read with **Full Disk Access**. A Focus switched on by a schedule rather than by hand isn't recorded there and isn't seen.
- **Windows** (`system/focus/windows.rs`): the quiet-hours profile behind Do Not Disturb, read with `NtQueryWnfStateData`. It is not an official API.

**Asking for Full Disk Access.** When the files can't be read, the app asks once: a message in the notch ("Allow Full Disk Access for notchbase to show your Focus") and a **Show your Focus** entry on the Notifications page whose **Open Settings** button opens Privacy & Security › Full Disk Access (`Action::AllowFullDiskAccess`). It remembers that it asked (`permissions.full_disk_asked` in `config.json`) and never asks again; without the permission Focus is simply skipped and no Focus events are sent. Granting it later works: macOS asks you to reopen notchbase, and from then on Focus is seen.

The **Focus** tool (`tools/focus-status`) shows a purple moon and the Focus's name when one turns on, and "Focus off" when it ends (setting `announce`). While a Focus is on, Battery holds its charger and fully charged notices and Downloads lands files on the Shelf without a banner; low battery warnings still show.

## Tests

- `system/power/windows.rs`, `system/sensors/names.rs`, `system/downloads/scan.rs` and `system/downloads/tracker.rs` are tested with synthetic readings, folders under the temp directory and synthetic clocks; `system/downloads/watch.rs` runs the real thread against a temp folder.
- Each tool has harness tests beside it (`tools/*/src/tests.rs`).
- `src/tools/tests.rs` checks that feeds reach the tools and come back as notices, live activities and requests.
- `notch/tests/system_tools.rs` paints every notice and live activity headlessly on an island and on two MacBook notches, and holds the live activities to 64 points beside the camera.
- Ignored tests read this computer's real battery, lock state, camera and microphone use, and hold a real keep-awake assertion; run them with `cargo test -- --ignored` (see [Building and Testing](Building-and-Testing.md#ignored-tests)).
