# update

Updates from the notchbase server. A thread checks for a newer release three seconds after start and every three minutes after that, and downloads one when asked. `install.rs` swaps the running copy for the new one and starts it.

Versions, the release list and checked downloads come from `crates/release`. The app's side, installing when the computer is idle or when you ask, is in `src/app/updates.rs`. [wiki/Updates.md](../../wiki/Updates.md) covers the server, the platforms and publishing a release.

Run the tests with `cargo test update::`.
