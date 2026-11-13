# Volume and media keys

With the **Volume indicator** setting on (`volume.indicator` in `config.json`, on by default), notchbase replaces the system's volume popup (and, while music is playing, the media popup) with the notch. It intercepts the volume and media keys, applies the change itself, and shows the level in the notch. It also notices volume changes made anywhere else (the menu bar, the taskbar flyout, another app) and shows those too. `src/system/volume` does the key handling and watching; the indicator is drawn by the notch.

On macOS the same service also takes the **brightness keys** when the **Brightness indicator** setting is on (`brightness.indicator`, on by default); see [Brightness keys](#brightness-keys).

## The service (`system/volume/watch.rs`)

`Volume::start(enabled)` spawns the `notchbase-volume` thread. The app turns it on or off with `set_enabled` (the volume indicator) and `set_brightness_enabled` (the brightness indicator) when a setting changes (both are always off during [previews](Previews.md)). While either is on, the thread loops roughly every 30 ms:

1. **Install the key tap** if there is none, retrying every 3 seconds if installation fails (for example until macOS Accessibility access is granted).
2. **Collect key presses** from the tap. Volume and brightness presses are applied right away; media key presses are queued for the app.
3. **Apply volume presses**: read the current output level, compute each press's change (`system/volume/steps.rs`), write it to the output device, and publish the resulting level.
4. **Watch for outside changes** (`system/volume/detector.rs`): read the output level and publish it if it moved by more than 0.002 or the mute state flipped since the last reading **on the same device**. Switching the default device does not count as a change, so plugging in headphones does not flash the indicator.

Steps 3 and 4 are skipped while the volume indicator is off. While both indicators are off (or the brightness indicator is the only one on and no display can be adjusted), the tap is removed and the thread only checks the settings every 250 ms.

The tap only takes the keys somebody claimed (`system/volume/tap.rs`): volume keys while the volume indicator is on, media keys while it is on and the app has claimed them, brightness keys while the brightness indicator is on and a display can be adjusted. Everything else passes through to the system.

On the main thread, each frame the app:

- tells the service whether to **claim the media keys**: only when something is playing or a player session exists, so with nothing to control the keys keep their normal system behaviour;
- drains queued media keys and turns them into media controls (see [Media and Music](Media-and-Music.md)), which also make the notch react instantly;
- takes a brightness level a key press published, if any, and calls `Notch::show_brightness(level)` on every notch;
- takes the latest published level and calls `Notch::show_volume(volume, muted)` on every notch, unless the user just moved the mixer's Output slider (on the Media page, or the Sound page) (the indicator is quiet for `volume.quiet_after_mixer_ms` after that, 1.2 seconds by default; see [Configuration](Configuration.md)).

## Steps (`system/volume/steps.rs`)

| Platform | Normal step | Fine step |
| --- | --- | --- |
| macOS | 1/16 (like the system) | 1/64, with Option + Shift held |
| Windows | 1/50 (2%, like the system) | 1/50 |

A step rounds the current level to the step grid first, so the volume always lands on clean values. Volume up and down also unmute. Mute toggles. Brightness steps on the macOS grid: 1/16, or 1/64 with Option + Shift.

## Windows (`system/volume/windows`)

**Keys** (`keys.rs`). A dedicated thread (`notchbase keys`) installs a low-level keyboard hook (`WH_KEYBOARD_LL`) and runs a message loop. The hook recognises `VK_VOLUME_UP`, `VK_VOLUME_DOWN`, `VK_VOLUME_MUTE`, `VK_MEDIA_PLAY_PAUSE`, `VK_MEDIA_NEXT_TRACK` and `VK_MEDIA_PREV_TRACK`. When it takes a key it returns a non-zero result so the key never reaches the system, which is what keeps the Windows volume flyout from appearing.

- The hook first calls the next hook in the chain. If another program already consumed the key, notchbase leaves it alone.
- Media keys are only taken while the app has claimed them.
- Holding a volume key repeats (every repeat is a step); holding a media key only counts the first press.

**Output** (`output.rs`). The default render endpoint's `IAudioEndpointVolume` is used to read and set the master volume and mute. The default device is re-checked every 500 ms, so a device switch is followed. The thread joins a COM apartment.

## macOS (`system/volume/macos`)

**Keys** (`keys.rs`). The volume and media keys arrive as `NSSystemDefined` events (subtype 8, "aux control buttons") rather than ordinary key events. A dedicated thread (`notchbase volume keys`) creates a **Quartz event tap** (`CGEvent::tap_create`) at the session level for that event type, adds it to its own run loop and runs it.

- The callback decodes the key code, down/up state and repeat flag from the event's `data1` and reads the modifier flags for fine steps.
- Taken events are swallowed by returning null; everything else is passed through.
- If macOS disables the tap because the callback was too slow or by user input, it is re-enabled immediately.
- Volume and brightness keys repeat while held; media keys count once. Fast-forward and rewind keys act as next and previous.

**Permission** (`trust.rs`). Event taps need **Accessibility** access. `may_watch_keys` checks `AXIsProcessTrusted`; the first time it is not trusted it asks macOS to show the Accessibility prompt (`AXIsProcessTrustedWithOptions` with the prompt option). Until access is granted, installation fails and is retried every 3 seconds, and the system popups keep working as usual.

**Output** (`output.rs`). The default output device's volume and mute are read and written through Core Audio, shared with the [Sound Mixer](Sound-Mixer.md) (`audio::master`).

## Brightness keys

macOS only (`brightness::SUPPORTED`); the Settings row is hidden elsewhere. Windows never delivers the brightness keys to a keyboard hook, so there is nothing to take there.

**Keys.** The brightness keys are `NSSystemDefined` aux control buttons too (codes 2 and 3), so the same event tap takes them. A press is swallowed, which keeps the system's brightness popup away, and the service steps the level itself.

**Display** (`system/brightness/macos.rs`). `Display` reads and sets the level through the private DisplayServices framework (`DisplayServicesGetBrightness`, `DisplayServicesSetBrightness`), loaded with `dlopen` so a macOS without it only loses the feature. It adjusts the built-in display, or failing that the first active display `DisplayServicesCanChangeBrightness` accepts (an Apple display), and only one whose level can be read, and looks again every 500 ms so closing the lid or plugging a display in is followed. With no adjustable display the keys aren't claimed and keep their system behaviour.

**Only key presses show.** Unlike the volume, brightness changed anywhere else (Control Center, System Settings, automatic brightness following the room) is not watched: the level drifts by itself all day and the indicator would keep popping up.

## Other platforms

`system/volume/other.rs` never installs a tap and never reads a level, so the indicator simply never appears. `system/brightness/other.rs` (everything but macOS) has no display to adjust.

## The indicator

`Notch::show_volume` shows the level as a short activity (`notices.volume_ms`, 1.6 seconds by default): a capsule with a speaker glyph, a liquid meter and the percentage on an island, or the glyph and value in the lanes beside the camera with the meter below on a hardware notch. Presses while it is visible slide the meter to the new value instead of restarting the activity. When a song is playing, the volume instead appears as a strip under the song in the music layout. `Notch::show_brightness` shows the display brightness in the same capsule or strip with a sun glyph, for `notices.brightness_ms`. See [Notch](Notch.md#volume-indicator).

Every volume level change the service publishes is also broadcast to tools that subscribe to volume (`app/tools/feeds.rs`), including while the indicator is quieted after using the mixer. Brightness isn't broadcast to tools. See [tools/Subscriptions.md](tools/Subscriptions.md).

## Tests

`system/volume/steps.rs`, `system/volume/detector.rs` and `system/volume/tap.rs` test stepping on both platforms' grids (brightness included), unmuting, change detection on the same device (and that a change notchbase just made is not reported twice), and which keys are claimed. `system/volume/windows/keys.rs` tests the key map, leaving keys another hook consumed, and repeat handling. On Windows an ignored test reads the real output device and briefly installs the keyboard hook; run it deliberately (see [Building and Testing](Building-and-Testing.md#ignored-tests)). On macOS an ignored test in `system/brightness/macos.rs` reads the real display brightness and writes the same level back.
