# Tools

Everything you can do from the notch's **Tools** page is a *tool*: Pick Color, Screenshot, Record Screen and [Alerts](../Alerts.md) are tools, and so is anything you write yourself. Tools are small Rust crates built on the `notchbase-sdk` crate (in [`sdk/`](../../sdk)). The same SDK powers the tools that ship with notchbase and the ones you install as plugins.

This page is the map. If you want to write a tool right away, jump to [Getting Started](Getting-Started.md).

## Where a tool shows up

A tool can appear in up to five places. Each is optional.

```
 closed notch                                   open notch
 ┌──────────────────────────────────┐          ┌──────────────────────────────────────────┐
 │ ●  ▀▀▀▀ camera ▀▀▀▀   0:42       │  live    │  Media  Tools  Shelf  Sound  Stats  Focus │ tabs
 └──────────────────────────────────┘          │    (◉)       (◉)       (●)       (◷)     │ tile icons
 ┌──────────────────────────────────┐          │   Color   Screenshot  Record    Timer    │
 │ ◐  ▀▀▀▀ camera ▀▀▀▀   #3A7BFF    │  notice  │ ┌──────────────────────────────────────┐ │
 └──────────────────────────────────┘          │ │ ● Record Screen        [Mic][Desktop]│ │ the card:
 ┌──────────────────────────────────┐          │ │ Ctrl Alt R           Microphone ▾   │ │ the tile of
 │    Couldn't reach the server     │  notify  │ └──────────────────────────────────────┘ │ the icon you
 └──────────────────────────────────┘          │                                          │ linger on
                                               └──────────────────────────────────────────┘
```

| Surface | Where | How you provide it |
| --- | --- | --- |
| Tile | The tool's icon and title in the **Tools** tab's launcher, with a card that opens under it | `fn tile(&self) -> Option<Tile<Msg>>` |
| Page | A full view in the open notch, opened from the Tools tab | `fn page(&self) -> Option<Element<Msg>>` plus `Manifest::page("Title")` |
| Live activity | Around the camera housing (or in the island) while the notch is closed, for as long as you return it | `fn live(&self) -> Option<Live<Msg>>` |
| Notice | A short pop-up in the closed notch, with elements on both sides and, if you want, a button | `Command::notice(Notice::new(leading, trailing))`, or `Command::notice_with_controls(notice, button)` |
| Message | A short line of plain text in the closed notch | `Command::notify("text")` |

[Surfaces](Surfaces.md) covers each one in detail.

## How a tool is written

Tools follow the Elm architecture. You write:

- a **model**: a plain struct holding your tool's state
- a **message** type: an enum of everything that can happen
- `update`: changes the model in response to a message and returns a `Command` asking notchbase to do things (copy text, show a notice, pick a color, run background work…)
- `subscriptions`: what outside events you care about (timers, your shortcuts, settings changes, recording updates…)
- views: `tile`, `page` and `live` describe what to show, as a tree of elements

```rust
use notchbase_sdk::prelude::*;

pub struct Hello;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Wave,
}

impl Tool for Hello {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("hello", "Hello")
    }

    fn init(_context: &Context) -> (Self, Command<Msg>) {
        (Hello, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Wave => Command::notify("Hello from the notch"),
        }
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        Some(Tile::new("Hello").icon(Icon::Star).on_press(Msg::Wave))
    }
}

notchbase_sdk::export!(Hello);
```

notchbase owns the look. Glass cards, fonts, colors, spacing and animation all come from the host, so every tool looks native on both macOS and Windows and on both notched and notchless displays. A tool picks *what* to show (a button, a slider, a timer) and a few semantic choices (a text style such as `headline` or `caption`, a palette tone such as `Tone::Red`), and notchbase decides *how* it looks. See [Elements](Elements.md).

## What a tool can ask notchbase to do

Through [Commands](Commands.md) a tool can show notices and messages, copy text or images, pick a color from the screen, capture a screen region, open a capture to draw on, start and stop screen recordings, save images, add files to the Shelf, reveal or open files and links, open its page, close the notch, control media playback, save its settings, and run work on a background thread.

Through [Subscriptions](Subscriptions.md) it can hear about timers, its keyboard shortcuts, settings changes, recording progress, microphones, what's playing, Bluetooth devices, volume changes and whether the notch is open.

## Bundled tools and plugins

A tool crate can be shipped two ways, with no code changes:

- **Bundled**: the crate lives in the notchbase repository under `tools/` and is compiled into the notchbase executable. The built-in tools (Pick Color, Screenshot, Record Screen and Alerts) work this way.
- **Plugin**: the crate is compiled to a dynamic library (`.dll` on Windows, `.dylib` on macOS) and dropped into a folder under `tools/` next to the notchbase executable. notchbase loads it at startup. `tools/focus-timer` in the repository is the reference plugin.

A plugin whose manifest `id` matches a bundled tool replaces it. See [Plugins and Distribution](Plugins-and-Distribution.md).

## Settings and shortcuts

Tools declare their settings and global keyboard shortcuts in their manifest. Users change them from the notch (each tool's own settings page, behind the cog beside it in Settings › Tools, and the shortcut field on each tile) or by editing `config.json`, which notchbase reloads while running. See [Settings and Shortcuts](Settings-and-Shortcuts.md) and [Configuration](../Configuration.md).

## The SDK at a glance

| Item | What it is |
| --- | --- |
| `notchbase_sdk::prelude::*` | Everything a tool normally needs: `Tool`, `Command`, `Subscription`, `Tile`, `Live`, `Notice`, `Settings`, `Manifest`, `SettingSpec`, every element builder, the `row!` macro, styles and data types |
| `notchbase_sdk::column` | The `column!` macro. Import it by name next to the prelude (see [Elements](Elements.md#rows-and-columns)) |
| `notchbase_sdk::element` | Element builders and their types (`Button`, `Slider`, `Row`…) |
| `notchbase_sdk::protocol` | The serializable wire types: `Node`, `Event`, `Request`, `Reply`, `Manifest`, `Views`, `Effects`… |
| `notchbase_sdk::ffi` | The C ABI used by plugins and the `export!` macro |
| `Runtime`, `Driver`, `Waker` | The machinery that drives a tool. You use them directly in tests |

## All pages

1. [Getting Started](Getting-Started.md): a complete tool, from `cargo new` to seeing it in the notch
2. [The Elm Architecture](Elm-Architecture.md): model, messages, update, views, and how they fit together
3. [Elements](Elements.md): every element and layout, with their builder methods
4. [Surfaces](Surfaces.md): tiles, pages, live activities, notices and messages
5. [Commands](Commands.md): everything a tool can ask notchbase to do
6. [Subscriptions](Subscriptions.md): everything a tool can listen to
7. [Settings and Shortcuts](Settings-and-Shortcuts.md): declaring, reading and saving settings; global shortcuts
8. [Testing](Testing.md): unit testing tools without running notchbase
9. [Plugins and Distribution](Plugins-and-Distribution.md): building, installing and sharing a tool
10. [Protocol and ABI](Protocol-and-ABI.md): the JSON messages and the C entry points underneath
11. [Built-in Tools](Built-in-Tools.md): a walkthrough of Pick Color, Screenshot and Record Screen, with pointers to Alerts and the Focus plugin
12. [Example: Snippets](Example-Snippets.md): a complete tool with a page, a list, a text field and saved data
13. [Configuration](../Configuration.md): every key in `config.json`
