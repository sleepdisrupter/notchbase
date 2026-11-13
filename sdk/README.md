# notchbase-sdk

The SDK every notchbase tool is written with. It's Elm-style: a tool is a model and its messages. It describes its tile, its page and its live activity as plain data, and asks the host to do things by returning commands. The built-in tools and third-party plugins use exactly the same API.

To build a tool outside this repository, start with the tools kit in [KIT.md](KIT.md).

Most tools only need `use notchbase_sdk::prelude::*;`. The main pieces:

- `Tool` is the trait you implement: `manifest`, `init` and `update`, and if you want them `subscriptions`, `tile`, `page` and `live`. `Context` is what `init` gets, and `Settings` your saved settings.
- `Element` builds a view (`row`, `column`, `text`, `button`, `toggle`, `slider`, `list`, `canvas`, `image` and more), and `Tile`, `Live` and `Notice` are the places a tool shows up. `draw` paints inside a `canvas`.
- `Command` is what `update` returns, from `none` and `batch` to host requests like `notify`, `copy_text`, `capture_screen`, `fetch` or `write_file`. `Emitter` sends messages back from a `Command::spawn` task.
- `Subscription` is what a tool listens to: timers, shortcuts, settings, media, the notch, the calendar, power, downloads and more.
- `export!(MyTool)` exports the plugin, as a native library or as WebAssembly for a portable build.
- `testing::Harness` runs a tool without the app, so you can send it messages and events and check what it did.

The host side lives here too: `protocol` is the data that goes between a tool and the app, `Runtime`, `boxed`, `Driver` and `Waker` drive a tool, and `ffi` is the plugin ABI.

The SDK is plain Rust on `serde` and builds for Windows, macOS, Linux and `wasm32-unknown-unknown`. A `.wasm` build is portable and runs sandboxed on every notchbase. A native build is a `.dll`, `.dylib` or `.so` for one system, and needs `crate-type = ["cdylib", "rlib"]`. Anything that differs per system, like screen capture or calendars, stays in the host, and tools reach it only through commands and subscriptions.

Run the tests with `cargo test -p notchbase-sdk`. Tools test themselves with `testing::Harness`, see [Testing tools](../wiki/tools/Testing.md).

More in the wiki: [Tools overview](../wiki/tools/Overview.md), [Getting started](../wiki/tools/Getting-Started.md), [The Elm architecture](../wiki/tools/Elm-Architecture.md), [Elements](../wiki/tools/Elements.md), [Surfaces](../wiki/tools/Surfaces.md), [Commands](../wiki/tools/Commands.md), [Subscriptions](../wiki/tools/Subscriptions.md), [Plugins and distribution](../wiki/tools/Plugins-and-Distribution.md) and [Protocol and ABI](../wiki/tools/Protocol-and-ABI.md).
