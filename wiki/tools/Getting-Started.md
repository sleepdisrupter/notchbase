# Getting Started

This page builds **Tally**, a tool that counts things. It has a tile on the Tools page showing the count, a Reset button, and a global shortcut that adds one from anywhere. By the end you will have it running in notchbase both as a plugin and as a bundled tool.

## What you need

- Rust 1.88 or newer (`rustup update` brings an older one up to date)
- The notchbase SDK, the crate `notchbase-sdk`. Either unzip `notchbase-sdk.zip` (attached to the [latest release](https://github.com/sleepdisrupter/notchbase/releases/latest); it unpacks to a `notchbase-tools` folder with the SDK inside; see [Plugins and Distribution](Plugins-and-Distribution.md#the-sdk-kit)) or use a checkout of the notchbase repository, where it lives under [`sdk/`](../../sdk)
- A notchbase to try it in: either an installed copy, or a checkout you run with `cargo run`

## 1. Create the crate

```sh
cargo new --lib tally
cd tally
```

## 2. Cargo.toml

```toml
[package]
name = "tally"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
notchbase-sdk = { path = "../notchbase/sdk" }
```

- `cdylib` builds the dynamic library notchbase loads as a plugin.
- `rlib` lets other Rust crates link it, which is what notchbase does when the tool is bundled. It also lets integration tests in `tests/` use it. Unit tests inside `src/` work with either.
- Point `path` at the SDK: `../notchbase-sdk` when the crate sits in the unzipped `notchbase-tools` folder, or your notchbase checkout's `sdk` folder. A git dependency works too: `notchbase-sdk = { git = "https://github.com/sleepdisrupter/notchbase" }`.

## 3. src/lib.rs

```rust
use notchbase_sdk::prelude::*;

const BUMP: &str = "bump";

pub struct Tally {
    count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Bump,
    Reset,
}

impl Tool for Tally {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("tally", "Tally")
            .description("Count anything with a shortcut")
            .version(env!("CARGO_PKG_VERSION"))
            .shortcut(BUMP, "Add One", Some("Ctrl+Alt+T"))
    }

    fn init(_context: &Context) -> (Self, Command<Msg>) {
        (Self { count: 0 }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Bump => self.count += 1,
            Msg::Reset => self.count = 0,
        }
        Command::none()
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::shortcut(BUMP, Msg::Bump)
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let accessory = row![text(self.count.to_string()).value(), button("Reset").plain().on_press(Msg::Reset)];
        Some(Tile::new("Tally").icon(Icon::Plus).shortcut(BUMP).on_press(Msg::Bump).accessory(accessory))
    }
}

notchbase_sdk::export!(Tally);
```

What each part does:

- **`manifest()`** identifies the tool. The `id` (`"tally"`) is its permanent name: it keys the tool's settings and shortcuts in `config.json` and decides which tool a plugin replaces, so pick one and keep it. The `name` is what people see. `.shortcut(BUMP, "Add One", Some("Ctrl+Alt+T"))` declares a global shortcut with a default binding.
- **`init`** builds the starting model. It receives a [`Context`](Elm-Architecture.md#context) with the tool's settings, folders and platform.
- **`update`** handles each message and returns a `Command`. Tally has nothing to ask of notchbase, so it returns `Command::none()`.
- **`subscriptions`** says Tally wants to hear its `bump` shortcut, and turns each press into `Msg::Bump`.
- **`tile`** describes the tile. Clicking anywhere on it sends `Msg::Bump`; the accessory row on the right shows the count and a Reset button; `.shortcut(BUMP)` puts the shortcut's keycaps along the bottom, where people can click them to rebind.
- **`export!`** makes the crate loadable as a plugin. It is harmless when the tool is bundled.

## 4. Test it

Tools are plain Rust, so you can drive them in a unit test without notchbase running. Add this to the end of `src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Event, Start};
    use notchbase_sdk::{Driver, Runtime, Waker};

    use super::*;

    #[test]
    fn the_shortcut_counts_up() {
        let mut runtime = Runtime::<Tally>::new(Waker::noop());
        runtime.start(Start::default());
        runtime.handle(vec![Event::Shortcut { id: BUMP.into() }]);
        runtime.handle(vec![Event::Shortcut { id: BUMP.into() }]);
        assert_eq!(runtime.tool().unwrap().count, 2);
    }
}
```

```sh
cargo test
```

[Testing](Testing.md) shows how to click buttons, answer replies and check what a tool asked notchbase to do.

## 5. Run it as a plugin

Build the library:

```sh
cargo build --release
```

Cargo names the file after the crate, with `-` turned into `_`:

| Platform | File |
| --- | --- |
| Windows | `target\release\tally.dll` |
| macOS | `target/release/libtally.dylib` |

Copy it into a folder of its own inside your tools folder. The folder name is up to you; using the tool id keeps things tidy.

**Windows** (PowerShell):

```powershell
New-Item -ItemType Directory -Force "$env:APPDATA\notchbase\tools\tally"
Copy-Item target\release\tally.dll "$env:APPDATA\notchbase\tools\tally\"
```

**macOS**:

```sh
mkdir -p ~/Library/Application\ Support/notchbase/tools/tally
cp target/release/libtally.dylib ~/Library/Application\ Support/notchbase/tools/tally/
```

About a second later notchbase says "Reloaded tally" and the tile is on the Tools tab. Press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>T</kbd> anywhere and the count goes up. Change the code, build, copy again, and it reloads again: you never need to restart notchbase.

> Keep the library's file name as Cargo wrote it. notchbase finds the tool's entry point from the file name (`tally.dll` → `notchbase_tool_tally`), so a renamed file won't load.

If you would rather keep plugins somewhere else, set the `NOTCHBASE_TOOLS_DIR` environment variable to that folder before starting notchbase. See [Plugins and Distribution](Plugins-and-Distribution.md) for the details, including what happens when a plugin fails to load.

The repository also has a complete reference plugin, `tools/focus-timer`. From a notchbase checkout, `cargo build -p focus-timer --release` builds `target/release/focus_timer.dll` (`libfocus_timer.dylib` on macOS); copy it into a `focus-timer` folder in your tools folder the same way. See [The reference plugin](Plugins-and-Distribution.md#the-reference-plugin).

## 6. Or bundle it into notchbase

Bundled tools are compiled into the notchbase executable. To bundle Tally:

1. Move the crate into the notchbase repository as `tools/tally`. Everything under `tools/` is already a workspace member.
2. Change its SDK dependency to the in-repo path:

   ```toml
   [dependencies]
   notchbase-sdk = { path = "../../sdk" }
   ```

3. Add it to the root `Cargo.toml` of notchbase:

   ```toml
   [dependencies]
   tally = { path = "tools/tally" }
   ```

4. Register it in the host's list of bundled tools, currently `all()` in `src/tools/bundled.rs`. Each entry is a constructor such as `boxed::<color_picker::ColorPicker>`; add `boxed::<tally::Tally>` and grow the array's length by one:

   ```rust
   pub fn all() -> [Construct; 12] {
       [
           boxed::<color_picker::ColorPicker>,
           boxed::<::screenshot::Screenshot>,
           // … the other bundled tools, unchanged …
           boxed::<focus_status::FocusStatus>,
           boxed::<tally::Tally>,
       ]
   }
   ```

5. `cargo run`.

The tool's code doesn't change at all between the two ways of shipping.

## 7. Change the shortcut

Click the keycaps at the bottom of the Tally tile and press a new combination, or click the × to leave it unbound. The choice is saved to `config.json`:

```json
{
  "tools": {
    "tally": {
      "enabled": true,
      "shortcuts": { "bump": "Ctrl+Shift+Space" },
      "settings": {}
    }
  }
}
```

You can edit that file by hand too; notchbase picks up changes within about a second. See [Configuration](../Configuration.md).

## Next steps

- Give the tool a page of its own: [Surfaces](Surfaces.md#page)
- Save the count across restarts with a hidden `data` setting: [Settings and Shortcuts](Settings-and-Shortcuts.md#saving-state-with-data-settings)
- See a bigger tool with a page, a list and a text field: [Example: Snippets](Example-Snippets.md)
- A complete plugin with a live activity, a folder setting, notch state and tests: `tools/notes` in the repository
- Let an AI write tools for you: the **notchbase skill** (`notchbase-skill.zip`, attached to the [latest release](https://github.com/sleepdisrupter/notchbase/releases/latest)) teaches Claude or ChatGPT this whole SDK, and all of notchbase besides
- Browse what you can put on screen: [Elements](Elements.md)
