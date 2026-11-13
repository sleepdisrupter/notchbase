# Commands

`update` returns a `Command<Msg>`: a list of things for the SDK and notchbase to do after the model has changed. Commands are values; nothing happens until you return them.

```rust
fn update(&mut self, msg: Msg) -> Command<Msg> {
    match msg {
        Msg::Copy => Command::copy_text(self.hex.clone()).and(Command::notify("Copied")),
        Msg::Pick => Command::pick_color(Msg::Picked),
        Msg::Picked(Some(color)) => Command::copy_text(color.hex()),
        Msg::Picked(None) => Command::none(),
    }
}
```

## At a glance

| Command | Does | Answers with |
| --- | --- | --- |
| [`none()`](#combining-commands) | Nothing | |
| [`batch(commands)`](#combining-commands), [`a.and(b)`](#combining-commands) | Several commands, in order | |
| [`message(msg)`](#combining-commands) | Run `update` again with `msg`, right away | |
| [`map(f)`](#map) | Rewrap messages into a parent type | |
| [`notify(text)`](#notify) | Show a text message in the closed notch | |
| [`notice(Notice)`](#notice) | Show a notice in the closed notch | |
| [`notice_with_controls(Notice, controls)`](#notice_with_controls) | Show a notice with a button (or other controls) that can be pressed | your message, when a control is pressed |
| [`dismiss_notice()`](#dismiss_notice) | Remove your notice | |
| [`copy_text(text)`](#copy_text) | Put text on the clipboard | |
| [`copy_image(ImageId)`](#copy_image) | Put a captured image on the clipboard | |
| [`annotate(ImageId)`](#annotate) | Open a captured image in a window to draw on | |
| [`pick_color(f)`](#pick_color) | Let the user pick a color on screen | `Option<Color>` |
| [`capture_screen(f)`](#capture_screen) | Let the user capture a region of the screen | `Option<Capture>` |
| [`save_image(image, folder, prefix, format, f)`](#save_image) | Save a captured image to a file | `Result<String, String>` |
| [`cancel_capture()`](#cancel_capture) | Cancel a pick, capture or area choice in progress | |
| [`start_recording(options)`](#start_recording) | Start a screen recording | via [`Subscription::recording`](Subscriptions.md#recording) |
| [`stop_recording()`](#stop_recording) | Stop the recording | via [`Subscription::recording`](Subscriptions.md#recording) |
| [`save_setting(key, value)`](#save_setting) | Save one of your settings to `config.json` | via [`Subscription::settings`](Subscriptions.md#settings) |
| [`fetch(HttpRequest, f)`](#fetch) | Make an HTTP request | `Result<HttpResponse, String>` |
| [`read_file(path, f)`](#read_file-and-write_file) | Read a text file | `Result<String, String>` |
| [`write_file(path, text, f)`](#read_file-and-write_file) | Write a text file | `Result<String, String>` |
| [`shelve(paths)`](#shelve) | Add files to the Shelf | |
| [`reveal(path)`](#reveal) | Show a file in Finder or File Explorer | |
| [`open(target)`](#open) | Open a file, folder or URL | |
| [`open_page()`](#open_page) | Open the notch on your page | |
| [`open_settings()`](#open_settings) | Open the notch on your tool's settings | |
| [`collapse()`](#collapse) | Close the notch | |
| [`hold_open()`](#hold_open-and-release) | Keep the notch open until you release it | |
| [`release()`](#hold_open-and-release) | Let the notch close again | |
| [`media(MediaControl)`](#media) | Play/pause, next or previous track | |
| [`chime()`](#chime) | Play the system alarm sound | |
| [`mute_microphone(muted)`](#mute_microphone) | Mute or unmute the default microphone | via [`Subscription::sensors`](Subscriptions.md#sensors) |
| [`keep_awake(awake)`](#keep_awake) | Keep the computer and its display from sleeping | |
| [`save_event(EventDraft)`](#save_event-and-delete_event) | Add or change an event in Apple Calendar | via [`Subscription::calendar`](Subscriptions.md#calendar) |
| [`delete_event(id)`](#save_event-and-delete_event) | Delete an event from Apple Calendar | via [`Subscription::calendar`](Subscriptions.md#calendar) |
| [`log(text)`](#log) | Write a line to notchbase's log | |
| [`perform(task, f)`](#perform) | Run a function on a background thread | its result, through `f` |
| [`spawn(task)`](#spawn) | Run a background thread that sends messages | any number of messages |
| [`request(Request)`](#request) | Send a raw protocol request | |

`Command<Msg>` also implements `Default` (the same as `none()`).

## Combining commands

```rust
Command::none()
Command::batch([Command::copy_image(capture.image), Command::notice(notice)])
Command::copy_text(hex).and(Command::notify("Copied"))
Command::message(Msg::Refresh)
```

- `batch` takes anything iterable, so `Command::batch(vec_of_commands)` works when you build the list conditionally.
- Effects run in the order you list them.
- `message(msg)` runs `update(msg)` immediately after the current update, before notchbase sees any requests. Use it to reuse a branch of `update` rather than to loop.

### map

```rust
Msg::Stopwatch(inner) => self.stopwatch.update(inner).map(Msg::Stopwatch),
```

`map` wraps every message the command can produce (follow-up messages, replies and background results) with a function, so a part of your tool with its own message type can be embedded in a parent. See [Splitting a tool into parts](Elm-Architecture.md#splitting-a-tool-into-parts).

## Feedback in the notch

### notify

```rust
Command::notify("Couldn't reach the server")
```

Shows a line of text in the closed notch for `notices.message_ms` ([Configuration](../Configuration.md#notices)).

### notice

```rust
Command::notice(Notice::new(swatch(color), text(color.hex()).value()).pop())
```

Shows a notice. See [Surfaces](Surfaces.md#notice) for everything a `Notice` can do.

### notice_with_controls

```rust
let annotate = button("Annotate").on_press(Msg::Annotate(capture.image));
Command::notice_with_controls(notice, annotate)
```

`notice_with_controls(notice, controls)` shows the notice with one more element, `controls`, which is the only part of a notice that takes presses. `controls` is anything that converts into an `Element<Msg>`: usually one `button`. A press runs `update` with the handler's message, like a press on a tile or page.

- The controls belong to that notice. They keep working until your tool sends its next notice; after a plain `Command::notice`, a press on the old controls is ignored.
- A press doesn't remove the notice. Add `Command::dismiss_notice()` to your answer when it should go.
- `map` wraps the controls' messages along with everything else, so a part of your tool can send a notice with controls of its own message type.

See [Surfaces](Surfaces.md#controls) for where the controls sit and how the notch treats the pointer over them.

### dismiss_notice

```rust
Command::dismiss_notice()
```

Removes the notice your tool is showing, if any, at once: it does not wait for the notice's time to run out, and a pointer resting on the notice's controls does not keep it. Pair it with `Notice::sticky()`, which otherwise stays until the user clicks the notch.

## Clipboard

### copy_text

```rust
Command::copy_text(color.hex())
```

### copy_image

```rust
Command::copy_image(capture.image)
```

Copies an image you got from `capture_screen`. notchbase keeps the full pixels of only the most recent few captures, so copy or save a capture when you receive it rather than much later.

### annotate

```rust
Msg::Annotate(image) => Command::dismiss_notice().and(Command::annotate(image)),
```

Opens an image you got from `capture_screen` in notchbase's annotator: a window of its own where the user draws on the screenshot with a brush and writes on it with a text tool. Every change they make is copied to the clipboard, the annotated picture replacing whatever was there. What they add goes to the clipboard and nowhere else. There is no reply, and the capture itself is not changed: `copy_image` and `save_image` with the same `ImageId` still give the original, and a file you already saved stays as captured.

Sending it again for the image that's already open brings that window forward; another image replaces it. Like `copy_image`, it only works while notchbase still holds the full pixels, so offer it soon after the capture: otherwise the notch says "That image is no longer available". The built-in Screenshot tool sends it from the **Annotate** button on its notice. See [Annotate](../Annotate.md) for the window itself.

## Screen

These commands ask the user to do something on screen and answer with a message. You pass a function (usually a message constructor) that turns the answer into a message.

### pick_color

```rust
Msg::Pick => Command::pick_color(Msg::Picked),
Msg::Picked(Some(color)) => Command::copy_text(color.hex()),
Msg::Picked(None) => Command::none(),
```

Shows notchbase's color picker. The answer is `Some(Color)`, or `None` if the user cancelled.

### capture_screen

```rust
Msg::Capture => Command::capture_screen(Msg::Captured),
Msg::Captured(Some(capture)) => Command::batch([
    Command::copy_image(capture.image),
    Command::save_image(capture.image, self.folder.as_deref(), "Shot", ImageFormat::Png, Msg::Saved),
]),
Msg::Captured(None) => Command::none(),
```

Lets the user drag out a region of the screen. The answer is `None` if they cancelled, or a `Capture`:

| Field | Type | Meaning |
| --- | --- | --- |
| `image` | `ImageId` | The captured image. Use it with `copy_image`, `save_image` and the [`image`](Elements.md#image) element |
| `width`, `height` | `u32` | Size in pixels |
| `on_clipboard` | `bool` | `true` if the image is already on the clipboard, so you don't need to copy it again |

### save_image

```rust
Command::save_image(capture.image, self.folder.as_deref(), "Screenshot", ImageFormat::Png, Msg::Saved)
```

`save_image(image, folder: Option<&Path>, prefix, format, f)` writes a captured image to a new file whose name starts with `prefix`. `folder: None` uses notchbase's default folder. `format` is `ImageFormat::Png` or `ImageFormat::Jpeg`. The answer is `Ok(path)` with the saved file's path, or `Err(message)`.

```rust
Msg::Saved(Ok(path)) => Command::shelve([path]),
Msg::Saved(Err(error)) => Command::notify(error),
```

### cancel_capture

```rust
Msg::Pick if self.picking => {
    self.picking = false;
    Command::cancel_capture()
}
```

Cancels a color pick, region capture or recording-area choice that's in progress. The built-in tools send it when their shortcut is pressed a second time.

## Screen recording

### start_recording

```rust
Command::start_recording(RecordingOptions {
    microphone: true,
    desktop_audio: true,
    dim_outside: true,
    ..RecordingOptions::default()
})
```

Asks notchbase to record the screen. The user first chooses the area. Progress arrives through [`Subscription::recording`](Subscriptions.md#recording): the state moves through `Choosing`, `Starting`, `Recording { since }` and `Saving`, and ends with `Saved { path, seconds, note }` or `Failed { error }`.

| `RecordingOptions` field | Type | Meaning |
| --- | --- | --- |
| `microphone` | `bool` | Record the microphone |
| `desktop_audio` | `bool` | Record the computer's sound |
| `microphone_id` | `Option<String>` | Which microphone, by [`AudioInput::id`](Subscriptions.md#audio_inputs); `None` is the default input |
| `folder` | `Option<String>` | Where to save; `None` is notchbase's default |
| `max_fps` | `Option<u32>` | Frame rate limit; `None` lets notchbase choose |
| `dim_outside` | `bool` | Shade the screen outside the recorded area |

`RecordingOptions::default()` records no audio and leaves `dim_outside` on, as the built-in recorder does. Set `dim_outside: false` to record without the shade.

### stop_recording

```rust
Command::stop_recording()
```

Stops the recording in progress; the file is saved and reported through `Subscription::recording`.

## Settings

### save_setting

```rust
Msg::Remember(on) => {
    self.remember = on;
    Command::save_setting("remember", on)
}
```

Saves one of your tool's settings to `config.json` (under `tools.<id>.settings.<key>`). The value can be anything that converts into a JSON `Value`: `bool`, numbers, `&str`, `String`, `Vec`s of those, or a `Value`. notchbase then sends your tool the updated settings through [`Subscription::settings`](Subscriptions.md#settings). See [Settings and Shortcuts](Settings-and-Shortcuts.md).

## Files

### shelve

```rust
Command::shelve([path])
```

Adds files to the **Shelf** tab, where they can be dragged out, converted and shared. Takes any iterable of paths (`&str`, `String`, `PathBuf`…).

### reveal

```rust
Command::reveal("C:/Users/me/Videos/clip.mp4")
```

Shows the file in Finder or File Explorer.

### open

```rust
Command::open("https://example.com")
```

Opens a file, folder or URL with its default app.

## Network and files

These are done by notchbase, in the background, and answer with a message. They work the same in [portable and native tools](Plugins-and-Distribution.md#portable-tools); in a portable tool they are the only way out.

### fetch

```rust
Command::fetch(HttpRequest::get("https://api.example.com/items").header("Accept", "application/json"), Msg::Loaded)
```

```rust
Msg::Loaded(Ok(response)) if response.ok() => match response.json::<Vec<Item>>() { … },
Msg::Loaded(Ok(response)) => Command::notify(format!("The server said {}", response.status)),
Msg::Loaded(Err(problem)) => Command::notify(problem),
```

| Item | Meaning |
| --- | --- |
| `HttpRequest::get(url)`, `::post(url, body)`, `::new(method, url)` | The request |
| `.header(name, value)`, `.body(text)`, `.json(&value)`, `.timeout(duration)` | Refine it. `json` also sets the content type. The timeout is 30 seconds unless you say, and five minutes at most |
| `HttpResponse { status, body }` | What came back, whatever the status. `.ok()` is true for 200 to 299, `.json::<T>()` parses the body |
| `Err(String)` | It never arrived: no connection, a timeout, a bad address, or an answer over 16 MB |

Bodies are text. Add `serde = { version = "1", features = ["derive"] }` for your own types.

### read_file and write_file

```rust
Command::read_file("list.json", Msg::Read)
Command::write_file("~/Documents/Notes/today.md", text, Msg::Written)
```

Both answer with `Result<String, String>`: the text that was read, or the full path that was written (its folders are made if missing), or what went wrong. A plain name or relative path is inside the tool's own data folder (`context.data_dir()`), a path starting `~/` is in the user's home folder, and an absolute path is used as it is.

## The notch

### open_page

```rust
Msg::Open => Command::open_page(),
```

Opens the notch on your tool's page. Your manifest needs `.page(title)`.

### open_settings

```rust
Msg::SetUp => Command::open_settings(),
```

Opens the notch on your tool's own settings page (the one behind its cog in Settings › Tools). Use it for a "set this up first" button, such as the Calendar's when no calendar link has been added yet.

### collapse

```rust
Command::collapse()
```

Closes the open notch, for example after an action that needs the screen.

### hold_open and release

```rust
Msg::KeepOpen(true) => Command::hold_open(),
Msg::KeepOpen(false) => Command::release(),
```

`hold_open` opens the notch (if it isn't already) and keeps it open when the pointer leaves, the way it stays open while you type in a text field. It stays held until your tool sends `release`, or until tools reload. Several tools can hold it at once; it closes only when every one has released it. Combine it with `open_page` to hold your own page on screen: `Command::open_page().and(Command::hold_open())`.

Hold the notch only while the user is doing something that needs it, such as a live transcript they are reading, and always offer a way to let go. [`NotchState::held`](Subscriptions.md#notch) tells you whether anything is holding it.

## Media

### media

```rust
Command::media(MediaControl::PlayPause)
Command::media(MediaControl::Next)
Command::media(MediaControl::Previous)
```

Controls whatever is playing. Pair it with [`Subscription::media`](Subscriptions.md#media) to know what that is.

## Sound

### chime

```rust
Command::chime()
```

Plays the system's alarm sound once, without waiting for it: `Alarm01.wav` from the Windows media folder (or the notification sound if that's missing), or Glass on macOS. Pair it with a sticky, shaking [notice](#notice) for something that must not be missed, like a finished timer.

### mute_microphone

```rust
Command::mute_microphone(true)
```

Mutes or unmutes the system's default input device, for every app. A microphone without a mute switch of its own ignores it; [`Subscription::sensors`](Subscriptions.md#sensors) reports what actually happened.

## Power

### keep_awake

```rust
Command::keep_awake(true)
```

Keeps the computer and its display awake until you send `keep_awake(false)`. notchbase holds one assertion for as long as any tool is asking, and lets go when your tool is switched off or reloaded.

## Calendar

### save_event and delete_event

```rust
Command::save_event(EventDraft {
    id: None,
    calendar_id: work.id.clone(),
    title: "Dentist".into(),
    start,
    end: start + 30 * 60_000,
    ..EventDraft::default()
})
Command::delete_event(meeting.id.clone())
```

Writes to Apple Calendar on macOS. `id: None` adds a new event; the `id` of a [`CalendarEvent`](Subscriptions.md#calendar) changes that one occurrence of it (a repeating event keeps its other occurrences). `calendar_id` is one of the calendars in the feed whose `writable` is true. `start` and `end` are Unix milliseconds. `location`, `notes` and `url` (the event's link) are `Option`s: `None` leaves them as they are, `Some("")` clears them.

There is no reply. Once the change is saved, a fresh [`Subscription::calendar`](Subscriptions.md#calendar) feed arrives with it in place, usually within a fraction of a second. If Apple Calendar refuses (the event is gone, or access was taken away), notchbase shows the reason in the notch. Anywhere else than a Mac there is nothing to write to, and notchbase says so.

## Diagnostics

### log

```rust
Command::log(format!("synced {} items", count))
```

Writes a line to notchbase's log output. Useful while developing; users don't see it.

## Background work

`update` must return quickly, so anything slow (files, network, heavy computation) goes to a background thread. Each of these commands starts a new thread, named `notchbase-tool-<id>`, and feeds its results back to `update` on the UI thread.

### perform

```rust
Msg::Load => Command::perform(
    || std::fs::read_to_string("notes.txt").map_err(|err| err.to_string()),
    Msg::Loaded,
),
Msg::Loaded(Ok(text)) => {
    self.notes = text;
    Command::none()
}
```

`perform(task, f)` runs `task` on a background thread and sends `f(result)` back as a message. Messages have to be `Clone`, so convert errors that aren't (like `std::io::Error`) to strings first.

### spawn

```rust
Msg::Import => Command::spawn(|emitter| {
    for step in 1..=10 {
        std::thread::sleep(Duration::from_millis(200));
        emitter.send(Msg::Progress(step as f32 / 10.0));
    }
    emitter.send(Msg::Imported);
}),
```

`spawn(task)` runs `task` with an `Emitter<Msg>`, which can send any number of messages. `Emitter` is `Clone` and can be moved to other threads. `emitter.map(f)` turns it into an `Emitter` of another message type, wrapping each message with `f` before it's sent; that lets a helper written with its own message type report through yours:

```rust
Msg::Serve => Command::spawn(|emitter| server::run(emitter.map(Msg::Server))),
```

A long-lived `spawn` like this is the way to run a server or a watcher: the Alerts tool runs its local webhook this way (see [Alerts](../Alerts.md)).

Things to know about background work:

- There's no cancellation. If a result can go stale (the user started a newer request), keep a counter in the model, send it along, and ignore results that don't match.
- A panic on a background thread ends that thread only; no message arrives.
- For work on a schedule, prefer a single [`every`](Subscriptions.md#every) subscription that triggers `perform`, or one long-lived `spawn` loop, over starting threads in a tight loop.

## request

```rust
use notchbase_sdk::protocol::Request;

Command::request(Request::Collapse)
```

Sends any [protocol request](Protocol-and-ABI.md#requests) as is. Every request has a typed constructor above; prefer those. Requests that expect an answer (`PickColor`, `CaptureScreen`, `SaveImage`, `Fetch`, `ReadFile`, `WriteFile`) carry a reply id that only the typed constructors register, so an answer to a hand-built one never reaches your tool.

## How answers find their way back

Commands that ask a question (`pick_color`, `capture_screen`, `save_image`, `fetch`, `read_file`, `write_file`) remember your function under a fresh reply id and send the request with that id. When notchbase answers, the SDK calls the function and runs `update` with the result. Each question is answered once; a second answer with the same id is ignored. See [Protocol and ABI](Protocol-and-ABI.md#replies) for what that looks like on the wire.
