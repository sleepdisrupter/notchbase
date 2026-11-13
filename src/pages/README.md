# pages

The pages the open panel shows. Each one works out its height for the panel and draws itself into a `PageCtx` (in `mod.rs`).

There's a folder or file per page: `media` (with `player`), `tools`, `tool.rs` for a tool's own page, `shelf`, `mixer`, `stats`, `agents`, `find`, `library`, `notifications.rs`, `settings`, `welcome` and `whats_new`. `harness.rs` is for tests.

Pages only read what the app gives them through `PanelData` and send back `Action`s, so they never touch a service. To add one, follow [Writing a new page](../../wiki/Pages.md#writing-a-new-page). [wiki/Pages.md](../../wiki/Pages.md) describes every page.

Run the tests with `cargo test pages::`.
