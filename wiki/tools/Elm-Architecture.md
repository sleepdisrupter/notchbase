# The Elm Architecture

A notchbase tool is written the way Elm programs are: all state lives in one model, every change happens in one `update` function in response to a message, and what's on screen is a pure function of the model. notchbase runs the loop; your tool never draws, never touches a window, and never blocks waiting for the user.

## The loop

```
          ┌─────────────────────────── messages ───────────────────────────┐
          │  a click on a tile or control, a shortcut press, a timer tick,  │
          │  a reply (picked color, capture…), a settings change,           │
          │  a recording update, a result from background work             │
          ▼                                                                 │
   update(&mut self, msg) ──► Command ──► notchbase does things ────────────┘
          │                              (copy, show a notice, pick a color…)
          │ model changed
          ▼
   tile() / page() / live() ──► element trees ──► notchbase draws them
   subscriptions()          ──► what notchbase should send you next
```

1. Something happens: the user clicks a button, presses your shortcut, a timer fires, notchbase answers a request.
2. The SDK turns it into one of your messages and calls `update`.
3. `update` changes the model and returns a `Command`: requests for notchbase, follow-up messages, or background work.
4. The SDK asks your tool for its views and subscriptions again, and notchbase redraws and re-arms its listeners.

## The `Tool` trait

```rust
pub trait Tool: Sized + 'static {
    type Msg: Clone + Send + 'static;

    fn manifest() -> Manifest;
    fn init(context: &Context) -> (Self, Command<Self::Msg>);
    fn update(&mut self, msg: Self::Msg) -> Command<Self::Msg>;

    fn subscriptions(&self) -> Subscription<Self::Msg> { Subscription::none() }
    fn tile(&self) -> Option<Tile<Self::Msg>> { None }
    fn page(&self) -> Option<Element<Self::Msg>> { None }
    fn live(&self) -> Option<Live<Self::Msg>> { None }
}
```

| Method | Required | Purpose |
| --- | --- | --- |
| `manifest()` | yes | Who the tool is: id, name, description, version, page title, shortcuts, settings. Called without an instance, so it can't depend on state. |
| `init(&Context)` | yes | Build the starting model and an optional first command. |
| `update(&mut self, Msg)` | yes | Handle one message; return what should happen next. |
| `subscriptions(&self)` | no | Which outside events to receive, given the current state. |
| `tile(&self)` | no | The tile on the Tools page. `None` means no tile. |
| `page(&self)` | no | The tool's page, opened from the Tools tab. Needs `Manifest::page`. |
| `live(&self)` | no | A live activity in the closed notch. `None` means none right now. |

The model is your struct. It doesn't need to be `Send`, `Clone` or anything else; the SDK keeps it on notchbase's UI thread.

### The manifest

```rust
fn manifest() -> Manifest {
    Manifest::new("focus-timer", "Focus")
        .description("A Pomodoro-style focus timer")
        .version(env!("CARGO_PKG_VERSION"))
        .page("Focus")
        .shortcut("toggle", "Start or Pause Focus", Some("Ctrl+Alt+F"))
        .setting(SettingSpec::number("minutes", "Focus length", [5.0, 90.0], 5.0, 25.0).unit("min"))
}
```

| Builder | Meaning |
| --- | --- |
| `Manifest::new(id, name)` | `id` is the tool's permanent key (in `config.json`, for shortcuts, for plugin replacement). Use lowercase words with dashes. `name` is shown to people. |
| `.description(text)` | One line describing the tool. |
| `.version(text)` | Usually `env!("CARGO_PKG_VERSION")`. |
| `.page(title)` | The tool has a page; `title` names it (and the tile notchbase makes for it if the tool has none). See [Surfaces](Surfaces.md#page). |
| `.page_icon(Icon)` | The icon of the tile notchbase makes for a page-only tool, and of a header tab. |
| `.header_tab()` | Give the page its own tab in the header as well. See [Surfaces](Surfaces.md#page). |
| `.background()` | A tool with no tile and no page, listed under **Settings › Indicators**. |
| `.shortcut(id, title, default)` | A global shortcut. `default` is `Some("Ctrl+Shift+C")` or `None` for unbound. See [Settings and Shortcuts](Settings-and-Shortcuts.md#shortcuts). |
| `.setting(SettingSpec)` | A setting with a default. See [Settings and Shortcuts](Settings-and-Shortcuts.md#declaring-settings). |
| `.defaults()` | The declared settings' defaults as a map. The SDK uses it; you rarely need to. |

## Messages

`Msg` is usually an enum. It must be `Clone + Send + 'static`: `Clone` because a button's message is sent every time it's pressed, `Send` because background threads send messages too. Deriving `Debug` and `PartialEq` as well makes tests easier.

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Pick,
    Picked(Option<Color>),
    Configured(Settings),
}
```

Every message comes from one of these places:

| Source | How you set it up |
| --- | --- |
| A click, toggle, selection, drag or keystroke | A handler on an element: `button("Go").on_press(Msg::Go)`, `slider(v).on_change(Msg::Volume)`… See [Elements](Elements.md). |
| A click on the tile itself | `Tile::on_press(msg)` |
| A reply from notchbase | Commands that ask something: `Command::pick_color(Msg::Picked)` |
| An outside event | A subscription: `Subscription::shortcut("pick", Msg::Pick)` |
| Background work | `Command::perform(task, Msg::Done)` or `Command::spawn(|emitter| …)` |
| Your own code | `Command::message(msg)` |

## Context

`init` receives a `Context`:

| Method | Returns |
| --- | --- |
| `settings()` | `&Settings`: the tool's settings, stored values merged over the manifest defaults |
| `data_dir()` | `PathBuf` of a folder for your tool's own files: `tools/<your id>` inside notchbase's data folder. It may not exist yet, so create it before writing |
| `dirs()` | `&Dirs` with `data: String` and the user's `home`, `pictures`, `videos`, `desktop`, `downloads` and `documents` folders (each `Option<String>`) |
| `platform()` | `Platform::Windows`, `Platform::MacOs` or `Platform::Other` |
| `host_version()` | notchbase's version string |
| `time_zone()` | The user's time zone as an IANA name such as `Europe/Paris`, when notchbase knows it. A [portable tool](Plugins-and-Distribution.md#portable-tools) can't ask the system itself |
| `clock_24h()` | Whether the system shows times on a 24-hour clock, for labels you format yourself (the `timer(at).clock()` element already follows it) |

Keep what you need from it in your model; `Context` is only passed to `init`.

## Update

```rust
fn update(&mut self, msg: Msg) -> Command<Msg> {
    match msg {
        Msg::Pick => Command::pick_color(Msg::Picked),
        Msg::Picked(Some(color)) => {
            self.last = Some(color);
            Command::copy_text(color.hex()).and(Command::notify("Copied"))
        }
        Msg::Picked(None) => Command::none(),
        Msg::Configured(settings) => {
            self.format = settings.text("format").to_owned();
            Command::none()
        }
    }
}
```

- `update` runs on notchbase's UI thread. Keep it quick: no file or network I/O, no sleeping. Hand slow work to `Command::perform` or `Command::spawn` and handle the result as a message. See [Commands](Commands.md#background-work).
- Return `Command::none()` when there's nothing to do. Combine commands with `Command::batch([...])` or `a.and(b)`; they run in order.
- `Command::message(msg)` feeds another message into `update` right away, before notchbase sees any of the requests. Messages queued this way run in order.

## Views

`tile`, `page` and `live` build element trees from `&self`. They should only read the model: notchbase asks for fresh views after your tool has handled a message, not on every frame, so anything a view reads outside the model (the clock, a file) won't refresh on its own.

For anything that counts time on screen, use the [`timer`](Elements.md#timer) element. You give it a start or end time and notchbase redraws the digits itself, so a stopwatch needs no ticking subscription.

### How handlers work

Elements carry your messages and closures, but notchbase only ever sees numbers. When the SDK turns your views into nodes for notchbase, every handler (`on_press`, `on_toggle`, `on_change`…) gets a numbered slot. When the user clicks, notchbase sends back "tile, handler 3, pressed", and the SDK looks up slot 3 and calls `update` with the resulting message.

The numbers are reassigned every time views are rebuilt. An event for a slot that no longer exists, or with the wrong kind of value (a number for a button), is ignored. You never deal with the numbers yourself unless you're writing [tests](Testing.md#clicking-things).

Don't keep elements in your model. Build them fresh in the view functions.

## Subscriptions

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    let always = Subscription::batch([
        Subscription::shortcut("toggle", Msg::Toggle),
        Subscription::settings(Msg::Configured),
    ]);
    if self.running {
        always.and(Subscription::every(Duration::from_secs(1), Msg::Tick))
    } else {
        always
    }
}
```

`subscriptions` is called again after every batch of updates, so it can depend on state. Above, the one-second tick only exists while the timer runs; notchbase stops the timer as soon as the tool stops asking for it. See [Subscriptions](Subscriptions.md).

## Order of events

- notchbase delivers events in the order they happened. Each event's message is fully handled (its `update`, plus any `Command::message` follow-ups) before the next event is looked at.
- Requests in a command reach notchbase in the order you wrote them.
- A reply (`pick_color`, `capture_screen`, `save_image`, `fetch`, `read_file`, `write_file`) arrives later as its own message. Each reply is answered once.
- Messages from background threads arrive the next time notchbase calls into the tool; the thread wakes notchbase up so that happens promptly.

## Splitting a tool into parts

For larger tools, give each part its own model, message type and `update`, and wrap its messages in the parent's:

```rust
pub mod stopwatch {
    use notchbase_sdk::prelude::*;

    #[derive(Default)]
    pub struct Stopwatch {
        pub started_at: Option<u64>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum Msg {
        Start,
        Stop,
    }

    impl Stopwatch {
        pub fn update(&mut self, msg: Msg) -> Command<Msg> {
            match msg {
                Msg::Start => {
                    self.started_at = Some(now_ms());
                    Command::none()
                }
                Msg::Stop => {
                    self.started_at = None;
                    Command::notify("Stopped")
                }
            }
        }

        pub fn subscriptions(&self) -> Subscription<Msg> {
            Subscription::shortcut("stopwatch", Msg::Start)
        }

        pub fn view<Outer: Clone + 'static>(&self, wrap: impl Fn(Msg) -> Outer) -> Element<Outer> {
            match self.started_at {
                Some(at) => row![timer(at), button("Stop").on_press(wrap(Msg::Stop))].into(),
                None => button("Start").on_press(wrap(Msg::Start)).into(),
            }
        }
    }
}
```

In the parent:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Stopwatch(stopwatch::Msg),
}

fn update(&mut self, msg: Msg) -> Command<Msg> {
    match msg {
        Msg::Stopwatch(inner) => self.stopwatch.update(inner).map(Msg::Stopwatch),
    }
}

fn subscriptions(&self) -> Subscription<Msg> {
    self.stopwatch.subscriptions().map(Msg::Stopwatch)
}

fn page(&self) -> Option<Element<Msg>> {
    Some(self.stopwatch.view(Msg::Stopwatch))
}
```

`Command::map` and `Subscription::map` rewrap messages, including replies and background results. `Element::map` (see [Elements](Elements.md)) does the same for views: a child can return its own `Element<child::Msg>` for the parent to `.map(Msg::Stopwatch)`, or take a function that wraps its messages, as `view(wrap)` does above.

## Time helpers

The prelude includes three small helpers:

| Function | Does |
| --- | --- |
| `now_ms()` | The current time in Unix milliseconds, the unit `timer` and `RecordingState::Recording { since }` use |
| `unix_ms(SystemTime)` | Any `SystemTime` in Unix milliseconds |
| `clock(Duration)` | Formats like a stopwatch: `"0:42"`, `"12:05"`, `"1:02:05"` |

## Rules of thumb

- The model is the single source of truth. When you ask notchbase to save a setting, update the model at the same time instead of waiting for the change to come back.
- Keep `update` and the view functions fast and free of I/O.
- Use `timer` for clocks and countdowns, and `every` only when the model itself has to change on a schedule.
- Keep ids (tool id, shortcut ids, setting keys) in constants; they are stored in people's `config.json`, so changing them later loses their choices.
