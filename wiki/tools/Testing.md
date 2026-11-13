# Testing

A tool is plain Rust with no window or platform code, so you can test it with `cargo test` and no notchbase running. The SDK's `Runtime` drives a tool exactly the way notchbase does: you feed it events and look at what it asks for, what its views contain, and what its model holds.

## The quick way: Harness

`notchbase_sdk::testing::Harness` drives a tool the way a person would: type into fields by their placeholder, click buttons by their label, flip toggles, press shortcuts, and read what's on screen. Most tool tests need nothing else.

```rust
#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::Request;
    use notchbase_sdk::testing::Harness;

    use super::*;

    #[test]
    fn adding_a_snippet() {
        let mut snippets = Harness::<Snippets>::start();
        snippets.type_text("New snippet", "hello");
        snippets.click("Add");
        assert_eq!(snippets.model().saved, ["hello"]);
        assert!(snippets.shows("hello"));
        assert!(matches!(snippets.requests(), [Request::SaveSetting { .. }]));
    }
}
```

| Method | Does |
| --- | --- |
| `Harness::<T>::start()` | Starts the tool with default settings |
| `Harness::<T>::start_with([("key", value), …])` | Starts it with some settings set |
| `model()` | Your tool's model, to assert on |
| `settings()` | The current merged settings |
| `requests()` | What the last step asked notchbase to do |
| `saved(key)` | The value the last step saved for `key`, if it saved one |
| `click(label)` / `click_nth(label, n)` | Clicks a button, round button, chip or checkbox by its label, searching the page, the tile and the live activity. Panics with the list of clickable labels if there's none |
| `click_icon(Icon, n)` | Clicks the `n`th button or chip that carries that icon, labelled or not |
| `click_notice(label)` | Clicks a control, by its label, on the last notice the tool sent with [`notice_with_controls`](Commands.md#notice_with_controls), however many steps ago that was. Panics with the list of that notice's controls if there's none with that label, which is also what happens once a plain notice has replaced it |
| `press_tile()` | Presses the tile |
| `type_text(placeholder, text)` | Types into the text field with that placeholder |
| `submit(placeholder)` | Presses Enter in that field |
| `set_toggle(n, on)` | Flips the `n`th toggle |
| `select(option)` | Picks an option of a segmented control or dropdown by its label |
| `slide(n, value)` | Moves the `n`th slider |
| `pointer(n, pointer)` | Sends a `Pointer` to the `n`th [canvas](Elements.md#canvas): a `Scroll` goes to its `on_scroll`, anything else to its `on_pointer` |
| `pan(n, by)` / `tap_pan(n, at)` | Drags the `n`th [pan](Elements.md#pan) by that many points, or clicks it that far from its child's left edge |
| `scroll_calendar(steps)` | Scrolls the first [week](Elements.md#week) or [month](Elements.md#month) on screen that has `on_scroll` |
| `pick_color(index)` | Clicks a circle in the first [palette](Elements.md#palette) on screen |
| `tap_week_day(year, month, day)` / `tap_week_event(id)` | Taps a day's column, or an event's block, in the first [week](Elements.md#week) on screen |
| `pick_day(day)` / `pick_date(year, month, day)` | Clicks a day of the first [month](Elements.md#month) that can be picked, or any date on it (such as one from the next month in its last row) |
| `shortcut(id)` | Presses a shortcut |
| `tick(interval)` | Fires a timer subscription |
| `change_settings([(key, value), …])` | Changes settings as if the user had |
| `notch(NotchState)`, `media(…)`, `volume(v, muted)`, `calendar(CalendarFeed)`, `power(Power)`, `screen_lock(locked)`, `sensors(Sensors)`, `download(DownloadChange)`, `focus(Focus)`, `event(Event)` | Sends what notchbase would |
| `answer(Reply)` | Answers the most recent question: `Reply::Color` for `pick_color`, `Reply::Capture` for `capture_screen`, `Reply::Saved` for `save_image` and `write_file`, `Reply::Fetched` for `fetch`, `Reply::Read` for `read_file` |
| `wait_for(\|model\| done)` | Lets background work (`perform`, `spawn`) finish, up to 5 seconds, until `done` is true |
| `texts()` / `shows(text)` | The text elements, the labels of buttons, round buttons and chips, a week's event titles and canvas text, on the tile's accessory and footer, the page and the live activity / whether any contains `text`. The tile's title, checkbox labels, segmented and dropdown options, what a text field holds, keycaps, pickers and timers are not among them: read those from `tile()`, `page()` or the model |
| `noticed()` | Every text in the notices the last step showed, the labels of their controls included |
| `wants()` | What the tool is subscribed to after the last step, such as its timers in `every_ms` |
| `tile()`, `page()`, `live()` | The raw views, for anything more detailed |

Every step returns the requests it produced, so `assert_eq!(tool.shortcut("open"), [Request::OpenPage])` reads naturally. `tools/notes/src/tests.rs` in the repository is a full example.

A notice's controls aren't part of any view, so `click` doesn't find them; `click_notice` does. Here `Msg::Shot(image)` makes the tool show a "Copied" notice with an Annotate button:

```rust
#[test]
fn the_notice_offers_to_annotate() {
    let mut tool = Harness::<Shooter>::start();
    tool.send(Msg::Shot(ImageId(7)));
    assert_eq!(tool.noticed(), ["Copied", "2560×1600", "Annotate"]);
    let answered = tool.click_notice("Annotate").to_vec();
    assert_eq!(answered, [Request::DismissNotice, Request::Annotate { image: ImageId(7) }]);
}
```

`send(msg)` runs one of your messages through `update`, as `Runtime::send` does.

The rest of this page shows the layer underneath, `Runtime`, for the rare test that needs it.

## Setup

```rust
#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Event, Node, Reply, Request, Start, Surface, UiValue, Values};
    use notchbase_sdk::{Driver, Runtime, Waker};

    use super::*;

    fn started() -> Runtime<Picker> {
        let mut runtime = Runtime::<Picker>::new(Waker::noop());
        runtime.start(Start::default());
        runtime
    }
}
```

`start`, `handle` and `views` come from the `Driver` trait, so import it. `Waker::noop()` is a waker that does nothing, which is all you need unless you test [background work](#background-work).

Unit tests in `src/` work whether your crate is a `cdylib`, an `rlib` or both. Integration tests in `tests/` need the `rlib`.

## What you can look at

| Call | Gives you |
| --- | --- |
| `runtime.start(start)` | `Effects` from `init` |
| `runtime.handle(events)` | `Effects` from handling those events |
| `runtime.send(msg)` | `Effects` from running one of your messages through `update` |
| `runtime.views()` | `Views { tile, page, live }` as nodes |
| `runtime.tool()` | `Option<&YourTool>`: the model, after `start` |
| `runtime.settings()` | The current merged `Settings`, including values the tool has just saved with `save_setting` |

`Effects` has three fields:

- `requests`: what the tool asked notchbase to do, in order (`Request::CopyText { .. }`, `Request::Notice { .. }`…)
- `wants`: what its subscriptions need (`every_ms`, `settings`, `media`…)
- `revision`: a number that grows whenever the model may have changed

## Starting with settings

```rust
fn started_with(format: &str) -> Runtime<ColorPicker> {
    let mut settings = Values::new();
    settings.insert("format".into(), format.into());
    let mut runtime = Runtime::new(Waker::noop());
    runtime.start(Start { settings, ..Start::default() });
    runtime
}
```

Values you leave out take the manifest's defaults, as they would in notchbase.

## Checking requests

```rust
#[test]
fn a_picked_color_comes_back_through_the_reply() {
    let mut runtime = started();
    let effects = runtime.send(Msg::Pick);
    let [Request::PickColor { reply }] = effects.requests[..] else { panic!("{:?}", effects.requests) };
    let color = Color::rgb(1, 2, 3);
    runtime.handle(vec![Event::Reply { id: reply, reply: Reply::Color { color: Some(color) } }]);
    assert_eq!(runtime.tool().unwrap().last, Some(color));
}
```

Commands that ask a question produce a request with a `reply` id. Answer it with `Event::Reply` and the matching `Reply`: `Reply::Color { color }` (`pick_color`), `Reply::Capture { capture }` (`capture_screen`), `Reply::Saved { result }` (`save_image`, `write_file`), `Reply::Fetched { result }` (`fetch`, with `Ok(HttpResponse { status, body })` or `Err(text)`) or `Reply::Read { result }` (`read_file`).

Slice patterns (`let [Request::PickColor { reply }] = effects.requests[..] else { … }`) check the exact list of requests and pull out fields in one step. `matches!` is handy for a single request:

```rust
assert!(matches!(&effects.requests[..], [Request::SaveSetting { key, .. }] if key == "loud"));
```

## Subscriptions

Send the events notchbase would send:

```rust
#[test]
fn ticks_and_shortcuts() {
    let mut runtime = started();
    runtime.handle(vec![Event::Tick { every_ms: 1000 }, Event::Tick { every_ms: 250 }]);
    let effects = runtime.handle(vec![Event::Shortcut { id: "pick".into() }]);
    assert_eq!(runtime.tool().unwrap().ticks, 1);
    assert!(matches!(effects.requests[..], [Request::PickColor { .. }]));
}
```

A tick only reaches subscriptions with that exact interval, so the 250 ms tick above is ignored. Other events: `Event::Settings { values }`, `Event::Recording { update }`, `Event::AudioInputs { inputs }`, `Event::Media { now_playing }`, `Event::Bluetooth { change }`, `Event::Volume { level }`, `Event::Notch { state }`, `Event::Calendar { feed }`, `Event::Power { power }`, `Event::ScreenLock { locked }`, `Event::Sensors { sensors }`, `Event::Download { change }`, `Event::Focus { focus }`.

Check what a tool listens for with `effects.wants`:

```rust
#[test]
fn start_reports_wants() {
    let mut runtime = Runtime::<Picker>::new(Waker::noop());
    let effects = runtime.start(Start::default());
    assert!(effects.requests.is_empty());
    assert_eq!(effects.wants.every_ms, vec![1000]);
    assert!(effects.wants.settings);
    assert!(!effects.wants.media);
}
```

## Settings changes

```rust
#[test]
fn settings_arrive_merged_over_the_defaults() {
    let mut runtime = started();
    let mut values = Values::new();
    values.insert("loud".into(), true.into());
    runtime.handle(vec![Event::Settings { values }]);
    assert!(runtime.tool().unwrap().loud);
    assert!(runtime.settings().bool("loud"));
}
```

This is also how you test that hand edits to `config.json` reach your tool.

## Clicking things

Views come back as `Node` trees with handler numbers in them. Take the number from the node and send an `Event::Ui` for it:

```rust
#[test]
fn pressing_the_tile_and_its_toggle() {
    let mut runtime = started();
    let tile = runtime.views().tile.expect("a tile");
    let press = tile.on_press.expect("a pressable tile");
    let effects = runtime.handle(vec![Event::Ui { surface: Surface::Tile, handler: press, value: UiValue::Press }]);
    assert!(matches!(effects.requests[..], [Request::PickColor { .. }]));

    let Some(Node::Row { children, .. }) = tile.accessory else { panic!("an accessory row") };
    let Node::Toggle { on_toggle: Some(toggle), .. } = children[0] else { panic!("a toggle") };
    let effects =
        runtime.handle(vec![Event::Ui { surface: Surface::Tile, handler: toggle, value: UiValue::Bool(true) }]);
    assert!(matches!(&effects.requests[..], [Request::SaveSetting { key, .. }] if key == "loud"));
}
```

| Control | `UiValue` to send |
| --- | --- |
| Button, tile, text input's `on_submit` | `UiValue::Press` |
| Toggle, checkbox, chip | `UiValue::Bool(new_state)` |
| Dropdown, segmented, palette, a day of a month or week | `UiValue::Index(i)` |
| Slider, a pan's `on_pan` and `on_tap`, a month's or week's `on_scroll` | `UiValue::Number(x)` |
| Text input's `on_change`, a week's `on_event` (the event's id) | `UiValue::Text(whole_text)` |
| Picker | `UiValue::Picked { column, index }` |
| Canvas | `UiValue::Pointer(pointer)` |

The `Harness` methods (`pick_day`, `tap_week_day`, `pick_color`, `pan`, `pointer`, …) build these values for you, which is the reason to prefer them.

Use `Surface::Page` for handlers from `views().page` and `Surface::Live` for `views().live`.

A notice's [controls](Surfaces.md#controls) use `Surface::Notice`. Their handler numbers come from the `Request::Notice` itself, not from `views()`, and stay good until the tool's next notice:

```rust
let Request::Notice { notice } = &requests[0] else { panic!("{:?}", requests) };
let Some(Node::Button { on_press: Some(handler), .. }) = notice.controls else { panic!("{notice:?}") };
let pressed = Event::Ui { surface: Surface::Notice, handler, value: UiValue::Press };
let answered = runtime.handle(vec![pressed]).requests;
assert_eq!(answered, [Request::DismissNotice, Request::Annotate { image: ImageId(7) }]);
```

Call `views()` before sending UI events, and take the handler numbers from the most recent call: the numbers are assigned when views are built and reassigned each time.

### Finding nodes

`Node::any(&test)` searches a whole tree, and `Node::children()` lists a node's direct children:

```rust
let tile = runtime.views().tile.unwrap();
let accessory = tile.accessory.unwrap();
assert!(accessory.any(&|node| matches!(node, Node::Toggle { on: false, .. })));
```

## Background work

`Command::perform` and `Command::spawn` run on real threads. Give the runtime a waker that records when a result is ready, wait for it, then call `handle` with no events to let the tool take the result:

```rust
#[test]
fn background_work_lands_on_the_next_handle() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = wakes.clone();
    let mut runtime = Runtime::<Picker>::new(Waker::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.start(Start::default());
    runtime.send(Msg::Load);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while wakes.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    runtime.handle(Vec::new());
    assert_eq!(runtime.tool().unwrap().loaded, Some(42));
}
```

(with `use std::sync::Arc;`, `use std::sync::atomic::{AtomicUsize, Ordering};` and `use std::time::Duration;`)

Usually it's simpler to keep the slow part in a plain function, test that directly, and test the tool by sending the result message yourself: `runtime.send(Msg::Loaded(42))`.

## Controlling time

A tool that reads the clock (`now_ms()`) is hard to test at a specific moment. Make the clock a type parameter with the real clock as its default:

```rust
use std::marker::PhantomData;
use std::time::Duration;

use notchbase_sdk::prelude::*;

pub trait Clock: 'static {
    fn now() -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now() -> u64 {
        now_ms()
    }
}

pub struct Egg<C: Clock = SystemClock> {
    pub ready_at: Option<u64>,
    pub done: bool,
    clock: PhantomData<C>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Start,
    Tick,
}

impl<C: Clock> Tool for Egg<C> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("egg", "Egg Timer")
    }

    fn init(_context: &Context) -> (Self, Command<Msg>) {
        (Self { ready_at: None, done: false, clock: PhantomData }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Start => self.ready_at = Some(C::now() + 6 * 60_000),
            Msg::Tick => self.done = self.ready_at.is_some_and(|at| C::now() >= at),
        }
        Command::none()
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::every(Duration::from_secs(1), Msg::Tick)
    }
}

notchbase_sdk::export!(Egg);
```

The tool calls `C::now()` wherever it would call `now_ms()`. `export!(Egg)` uses the default, so the real tool reads the real clock. Tests plug in a clock they control:

```rust
thread_local! {
    static NOW: Cell<u64> = const { Cell::new(0) };
}

struct FakeClock;

impl Clock for FakeClock {
    fn now() -> u64 {
        NOW.with(Cell::get)
    }
}

#[test]
fn the_egg_is_done_after_six_minutes() {
    NOW.with(|now| now.set(1_000));
    let mut runtime = Runtime::<Egg<FakeClock>>::new(Waker::noop());
    runtime.start(Start::default());
    runtime.send(Msg::Start);
    NOW.with(|now| now.set(1_000 + 6 * 60_000));
    runtime.handle(vec![Event::Tick { every_ms: 1000 }]);
    assert!(runtime.tool().unwrap().done);
}
```

The Focus tool in `tools/focus-timer` is tested this way.

## Testing the plugin boundary

To check that your tool works through the C ABI, as a plugin, call the table `export!` would hand to notchbase:

```rust
#[test]
fn the_plugin_speaks_json_through_the_c_abi() {
    use notchbase_sdk::ffi::{ABI_VERSION, Buffer, HostWaker, Plugin};
    use notchbase_sdk::protocol::{Answer, Call};

    unsafe extern "C" fn wake(_: *const std::ffi::c_void) {}

    fn call(plugin: &Plugin, instance: *mut std::ffi::c_void, call: &Call) -> Answer {
        let request = serde_json::to_vec(call).unwrap();
        let buffer: Buffer = unsafe { (plugin.call)(instance, request.as_ptr(), request.len()) };
        let answer = serde_json::from_slice(buffer.as_slice()).unwrap();
        unsafe { (plugin.release)(buffer) };
        answer
    }

    let plugin = Plugin::of::<Picker>();
    assert_eq!(plugin.abi, ABI_VERSION);
    let instance = unsafe { (plugin.create)(HostWaker { context: std::ptr::null(), wake }) };
    let Answer::Manifest { manifest } = call(&plugin, instance, &Call::Manifest) else { panic!() };
    assert_eq!(manifest.id, "picker");
    let start = Call::Start { start: Box::new(Start::default()) };
    assert!(matches!(call(&plugin, instance, &start), Answer::Effects { .. }));
    let Answer::Views { views } = call(&plugin, instance, &Call::Views) else { panic!() };
    assert_eq!(views.tile.unwrap().title, "Picker");
    unsafe { (plugin.destroy)(instance) };
}
```

This needs `serde_json` as a dev-dependency. For JSON-level tests without the C functions, `notchbase_sdk::ffi::serve(&mut runtime, bytes)` answers one JSON call on a `Runtime`.

## Tips

- Derive `Debug` and `PartialEq` on your message type, so you can compare and print messages.
- `send` before `start` does nothing: there's no model yet.
- Keep your shortcut ids and setting keys in constants, and use the same constants in tests.
- The built-in tools' tests are good examples: `tools/color-picker/src/lib.rs`, `tools/screenshot/src/lib.rs` and `tools/screen-recorder/src/tests.rs`.
