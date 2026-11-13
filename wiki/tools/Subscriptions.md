# Subscriptions

A subscription tells notchbase which outside events your tool wants, and how to turn each into a message. Return them from `Tool::subscriptions`:

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    Subscription::batch([
        Subscription::shortcut("pick", Msg::Pick),
        Subscription::settings(Msg::Configured),
    ])
}
```

`subscriptions` is called again after every batch of updates, so what you subscribe to can depend on state. After each call notchbase learns what your tool wants (which timers, whether it cares about media, recordings and so on) and only sends those events, so subscribe only while you need something.

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    let always = Subscription::shortcut("toggle", Msg::Toggle);
    if self.running {
        always.and(Subscription::every(Duration::from_millis(250), Msg::Tick))
    } else {
        always
    }
}
```

## At a glance

| Subscription | Message comes with | When |
| --- | --- | --- |
| [`every(interval, msg)`](#every) | nothing | On a timer |
| [`shortcut(id, msg)`](#shortcut) | nothing | One of your global shortcuts is pressed |
| [`settings(f)`](#settings) | `Settings` | Your settings change |
| [`recording(f)`](#recording) | `RecordingUpdate` | A screen recording changes state, is saved or fails |
| [`audio_inputs(f)`](#audio_inputs) | `Vec<AudioInput>` | The list of microphones changes |
| [`media(f)`](#media) | `Option<NowPlaying>` | What's playing changes |
| [`bluetooth(f)`](#bluetooth) | `DeviceChange` | A Bluetooth device connects, disconnects or updates |
| [`volume(f)`](#volume) | `VolumeLevel` | The system volume or mute changes |
| [`notch(f)`](#notch) | `NotchState` | The notch changes shape or state: opens, closes, peeks, hugs the top, changes page |
| [`calendar(f)`](#calendar) | `CalendarFeed` | Events in the system calendar change, or access to it does |
| [`power(f)`](#power) | `Power` | The battery level changes, or the charger goes in or out |
| [`screen_lock(f)`](#screen_lock) | `bool` | The screen locks or unlocks |
| [`sensors(f)`](#sensors) | `Sensors` | The camera or microphone starts or stops being used, or the microphone is muted |
| [`downloads(f)`](#downloads) | `DownloadChange` | A browser download moves, finishes or is cancelled |
| [`focus(f)`](#focus) | `Focus` | A Focus or Do Not Disturb turns on, off, or changes |

Combine them with:

| Method | Effect |
| --- | --- |
| `Subscription::none()` | Nothing (also `Default`) |
| `Subscription::batch(iterable)` | All of them |
| `a.and(b)` | Both |
| `.map(f)` | Wrap every message with `f`, to embed a part of your tool that has its own message type |

`batch` takes any iterator, so optional subscriptions chain in naturally:

```rust
let listening = [Subscription::shortcut(TOGGLE, Msg::Toggle), Subscription::settings(Msg::Configured)];
let ticking = self.running.then(|| Subscription::every(TICK, Msg::Tick));
Subscription::batch(listening.into_iter().chain(ticking))
```

Subscriptions that take a function (`settings`, `recording`…) usually take a message constructor: `Subscription::recording(Msg::Recording)` where `Msg::Recording(RecordingUpdate)`.

## every

```rust
Subscription::every(Duration::from_secs(1), Msg::Tick)
```

Sends `msg` every `interval`. Intervals shorter than 16 ms are raised to 16 ms. If notchbase falls behind (the computer slept, say), the timer skips ahead to the next due time instead of sending a burst of catch-up ticks. The first tick comes one interval after you subscribe.

Two subscriptions with the same interval share one timer. To display a clock, use the [`timer`](Elements.md#timer) element instead; it needs no ticks.

## shortcut

```rust
Subscription::shortcut("pick", Msg::Pick)
```

Sends `msg` when the shortcut with this id is pressed, wherever the user is. The id must be one declared with `Manifest::shortcut`; notchbase registers the key combination from the manifest and the user's `config.json`, not from the subscription. See [Settings and Shortcuts](Settings-and-Shortcuts.md#shortcuts).

## settings

```rust
Subscription::settings(Msg::Configured)

Msg::Configured(settings) => {
    self.format = Format::named(settings.text("format"));
    Command::none()
}
```

Sends your tool's full, current `Settings` whenever they change: after your own `Command::save_setting`, after the user changes them in the Settings page, or after someone edits `config.json` by hand. Values missing from the file are filled in from your manifest's defaults. See [Settings and Shortcuts](Settings-and-Shortcuts.md#reading-settings).

## recording

```rust
Subscription::recording(Msg::Recording)

fn follow(&mut self, update: RecordingUpdate) -> Command<Msg> {
    match update {
        RecordingUpdate::State { state } => {
            self.state = state;
            Command::none()
        }
        RecordingUpdate::Saved { path, seconds, note } => Command::shelve([path]),
        RecordingUpdate::Failed { error } => Command::notify(error),
    }
}
```

Screen recording progress, after [`Command::start_recording`](Commands.md#start_recording).

| `RecordingUpdate` | Meaning |
| --- | --- |
| `State { state }` | The recording moved to a new `RecordingState` |
| `Saved { path, seconds, note }` | The file at `path` is ready; it's `seconds` long. `note`, when present, is something notchbase wants the user to know about the result |
| `Failed { error }` | The recording couldn't start or couldn't be saved |

| `RecordingState` | Meaning |
| --- | --- |
| `Idle` | Nothing is happening |
| `Choosing` | The user is choosing the area to record |
| `Starting` | Recording is about to begin |
| `Recording { since }` | Recording since `since` (Unix milliseconds; pass it to `timer`) |
| `Saving` | Writing the file |

## audio_inputs

```rust
Subscription::audio_inputs(Msg::Inputs)
```

The available microphones, whenever the list changes.

| `AudioInput` field | Type | Meaning |
| --- | --- | --- |
| `id` | `String` | Stable identifier; pass it as `RecordingOptions::microphone_id` |
| `name` | `String` | Name to show |
| `is_default` | `bool` | The system's default input |

## media

```rust
Subscription::media(Msg::Playing)
```

What's playing, or `None` when nothing is.

| `NowPlaying` field | Type | Meaning |
| --- | --- | --- |
| `title` | `String` | Track title |
| `artist` | `String` | Artist |
| `playing` | `bool` | `false` when paused |
| `position` | `f64` | Seconds into the track |
| `duration` | `f64` | Track length in seconds |

Control playback with [`Command::media`](Commands.md#media).

## bluetooth

```rust
Subscription::bluetooth(Msg::Device)

fn device(&mut self, change: DeviceChange) -> Command<Msg> {
    match change {
        DeviceChange::Connected { device } => match device.battery {
            Some(level) => Command::notify(format!("{} connected, {level}%", device.name)),
            None => Command::notify(format!("{} connected", device.name)),
        },
        DeviceChange::Disconnected { .. } | DeviceChange::Updated { .. } => Command::none(),
    }
}
```

`DeviceChange` is `Connected`, `Disconnected` or `Updated` (for example a new battery reading), each with a `device`:

| `Device` field | Type | Meaning |
| --- | --- | --- |
| `id` | `String` | Stable identifier |
| `name` | `String` | Name to show |
| `kind` | `String` | What sort of device: `headphones`, `speaker`, `keyboard`, `mouse`, `controller`, `phone`, `computer`, `watch` or `other` |
| `battery` | `Option<u8>` | Battery percentage, when the device reports one |

`Device` itself is in `notchbase_sdk::protocol` if you need to name the type.

## volume

```rust
Subscription::volume(Msg::Volume)
```

The system output volume, whenever it or the mute state changes. `VolumeLevel { volume: f32, muted: bool }`, with `volume` from `0.0` to `1.0`.

## calendar

```rust
Subscription::calendar(Msg::Calendar)

fn calendar(&mut self, feed: CalendarFeed) -> Command<Msg> {
    match feed {
        CalendarFeed::Events { events, .. } => self.events = events,
        CalendarFeed::Waiting | CalendarFeed::Denied | CalendarFeed::Unavailable => self.events.clear(),
    }
    Command::none()
}
```

The events in Apple Calendar on macOS: every calendar in every account added under System Settings, Internet Accounts (iCloud, Google, Exchange and the rest). notchbase starts reading only once a tool subscribes, and stops when none does. The first time, macOS asks whether notchbase may see your calendars.

| `CalendarFeed` | Meaning |
| --- | --- |
| `Events { events, calendars }` | Access is allowed. `events` covers from 36 hours ago to 60 hours ahead, so today and tomorrow in any time zone, plus any range asked for with `calendar_between`, with repeating events already expanded into occurrences |
| `Waiting` | macOS is asking the user right now |
| `Denied` | The user said no. They can change it in System Settings, Privacy & Security, Calendars |
| `Unavailable` | There is no system calendar to read (Windows) |

A new feed arrives when anything changes, checked every 30 seconds and right after a [`save_event` or `delete_event`](Commands.md#save_event-and-delete_event).

To see further out, for a month view, subscribe with `Subscription::calendar_between(from, to, Msg::Calendar)` (Unix milliseconds) instead. The feed then also covers that range, up to 400 days; when several tools ask, it covers them all. Change the range in `subscriptions` as the view moves and a fresh feed follows straight away.

| `CalendarEvent` field | Type | Meaning |
| --- | --- | --- |
| `id` | `String` | Identifies this occurrence; pass it to `save_event` or `delete_event` |
| `title` | `String` | May be empty |
| `start`, `end` | `u64` | Unix milliseconds |
| `all_day` | `bool` | An all-day event, running from local midnight to midnight |
| `location`, `notes`, `url` | `String` | Empty when not set |
| `calendar`, `calendar_id` | `String` | The calendar's name and identifier |
| `color` | `Option<Color>` | The calendar's color |
| `declined` | `bool` | You were invited and said no |
| `editable` | `bool` | The calendar allows changes |

| `CalendarInfo` field | Type | Meaning |
| --- | --- | --- |
| `id` | `String` | Pass it as `EventDraft::calendar_id` |
| `title` | `String` | The calendar's name |
| `account` | `String` | The account it belongs to, such as iCloud or an email address |
| `color` | `Option<Color>` | Its color |
| `writable` | `bool` | New events can be added to it |

## power

```rust
Subscription::power(Msg::Power)
```

The computer's own battery. You get the current reading when you subscribe and a new one whenever it changes.

| `Power` field | Type | Meaning |
| --- | --- | --- |
| `level` | `Option<u8>` | Battery percentage; `None` on a computer without a battery |
| `plugged` | `bool` | The charger is connected |
| `charging` | `bool` | The battery is charging; a full battery can be plugged in without charging |

## screen_lock

```rust
Subscription::screen_lock(Msg::Locked)
```

`true` when the screen locks and `false` when it unlocks. The first message is the current state, so compare it with the last one before treating it as news.

## sensors

```rust
Subscription::sensors(Msg::Sensors)
```

| `Sensors` field | Type | Meaning |
| --- | --- | --- |
| `camera` | `bool` | A camera is on |
| `microphone` | `bool` | An app is listening to a microphone |
| `apps` | `Vec<String>` | The apps known to be using them. macOS only names the apps using the microphone |
| `muted` | `bool` | The default input device is muted. Change it with [`Command::mute_microphone`](Commands.md#mute_microphone) |

## downloads

```rust
Subscription::downloads(Msg::Download)

fn download(&mut self, change: DownloadChange) -> Command<Msg> {
    match change {
        DownloadChange::Progress { download } => self.follow(download),
        DownloadChange::Finished { path, .. } => Command::shelve([path]),
        DownloadChange::Cancelled { download } => self.forget(&download.id),
    }
}
```

Browser downloads in the Downloads folder. `Progress` arrives about once a second while a file grows, and is also the first you hear of a download.

| `Download` field | Type | Meaning |
| --- | --- | --- |
| `id` | `String` | The unfinished file's name; the same for every change to one download |
| `name` | `String` | The name the finished file will have |
| `received` | `u64` | Bytes on disk so far |
| `total` | `Option<u64>` | The full size, when the browser says (Safari does) |

`download.share()` is `received / total` from `0.0` to `1.0`, or `None` without a total. See [System tools](../System-Tools.md#downloads) for how downloads are found.

## focus

```rust
Subscription::focus(Msg::Focus)
```

`Focus { active: bool, name: Option<String> }`: whether a Focus is on and its name ("Work", "Do Not Disturb") when known. The first message is the current state. On a Mac this needs Full Disk Access for notchbase; without it the subscription stays silent. Use it to hold back notices that can wait.

## notch

```rust
Subscription::notch(Msg::Notch)

Msg::Notch(state) => {
    self.visible = state.open && state.page.as_deref() == Some("focus-timer");
    Command::none()
}
```

Everything the notch is doing, updated as it moves. A new `NotchState` arrives whenever any field changes, including every step of an animation, so the halfway points of opening, closing and hugging all reach you.

| Field | Type | Meaning |
| --- | --- | --- |
| `open` | `bool` | A page is showing |
| `page` | `Option<String>` | Which page: `media`, `tools`, `shelf`, `sound`, `stats`, `agents`, `settings`, `notifications`, `library`, `find`, `whats-new` (the page of a release's notes), or a tool id |
| `phase` | `NotchPhase` | `Resting`, `Peeking` (hovered, about to open), `Open`, `Notice` (showing a notice), `Dropping` (files dragged over it) or `Asking` (an assistant question) |
| `style` | `NotchStyle` | `Notch` on a Mac with a camera housing, `Island` on screens without one (a floating pill) |
| `expansion` | `f32`, 0 to 1 | How far open: 0 is closed, 1 is the full panel, and values between are the notch opening or closing |
| `hug` | `f32`, 0 to 1 | How tightly it hugs the top edge. A resting song or live activity docks flat against the top (1) and lifts off as the pointer comes near |
| `float` | `f32`, 0 to 1 | On an island: 0 is tucked into the top edge as a small tab, 1 is a full floating capsule. Always 0 on a notch |
| `width`, `height` | `f32` | The shape's current size in points |
| `notch` | `Option<[f32; 2]>` | The camera housing's size in points on a notched Mac, `None` on an island |
| `held` | `bool` | Something is holding it open (typing, recording a shortcut, or a tool's [`hold_open`](Commands.md#hold_open-and-release)) |
| `settled` | `bool` | Nothing is moving. `false` during every animation |
| `activities` | `Vec<String>` | Live activities on show, main one first (at most 3): tool ids, `assistant` or `update` |
| `music` | `bool` | A song is showing in the notch |

Helpers read the common questions directly:

| Method | True when |
| --- | --- |
| `state.hugged()` | It's flat against the top edge |
| `state.floating()` | It's a free-floating island capsule, not hugging anything |
| `state.opening()` | It's part way open (opening or closing) |
| `state.fully_open()` | The panel is all the way out |
| `state.resting()` | It's at rest and nothing is moving |

Values are rounded (fractions to hundredths, sizes to half points) so a tool isn't woken for changes nobody could see. Subscribe only while you need it; while subscribed, your tool hears every frame of an animation.

Uses: do work only while your page is visible, mark things read once seen, draw a live activity differently when the notch is hugged, or wait for `settled` before showing something.
