# config

Settings, kept in `config.json`.

- `model.rs` is the shape of the file. Anything missing takes its default, so an older or partial file still loads.
- `store.rs` reads and writes it, notices when you edit it by hand, and never writes over a file with a mistake in it.
- `migrate.rs` brings old settings forward, from `settings.txt` and every format change since.
- `hotkey.rs` reads shortcuts the way the file writes them, like `Ctrl+Shift+C`.
- The other files are sections of the model: `annotate` and `updates`.

Anywhere in the app, `config::current()` gives the latest settings. To add a setting, put it in `model.rs` with a default, and react to changes in `src/app/config.rs` if reading it again isn't enough. [wiki/Configuration.md](../../wiki/Configuration.md) describes every key.

Run the tests with `cargo test config::`.
