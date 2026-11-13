# tools

Every tool in the notch: the ones built in and the plugins from the tools folders. Tools are written with the SDK in `sdk/`, and the built-in ones live in `tools/` at the repository root.

- `mod.rs` is the registry: loading tools, their order, and their views.
- `bundled.rs` lists the tools built into notchbase.
- `plugin.rs` loads native plugins through the C ABI the SDK exports, and `wasm.rs` runs portable tools in wasmi.
- `host.rs` runs one tool: its events, its timers, and catching its panics so a failing tool stops alone.
- `bindings.rs` and `shortcuts/` are tool shortcuts, as global hotkeys and as the Settings page shows them.
- `outside.rs` is what tools can reach outside notchbase: HTTP requests and text files.
- `system.rs` is the system feeds tools can ask for, `images.rs` the pictures they get, and `watch.rs` notices a rebuilt plugin.

The app's side, feeding tools and carrying out their requests, is in `src/app/tools`. [wiki/Architecture.md](../../wiki/Architecture.md#tools) covers the host and [wiki/tools/Overview.md](../../wiki/tools/Overview.md) writing a tool.

Run the tests with `cargo test tools::`. They load the real built-in tools.
