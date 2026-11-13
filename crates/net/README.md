# notchbase-net

The network pieces the app and the installer share: HTTPS agents and downloads that are hashed as they're copied. Every network call in the workspace goes through an agent made here, so timeouts, TLS and redirects are set up in one place.

The app uses it for updates, stats, video downloads, Spotify artwork, links dropped on the notch and tools' HTTP requests. `notchbase-release` uses it to read releases from GitHub and download them, and the installer to fetch the newest release.

- `agent(timeout, user_agent, mode)` makes an HTTPS-only `ureq` agent. `Mode::Follow` follows redirects, and `Mode::Exact` follows none and hands back every status as a response instead of an error.
- `copy_hashed` copies a download while hashing it, with an optional size cap and stop flag, and returns its SHA-256. `sha256_hex`, `file_sha256` and `hex` cover everything else.

TLS comes from `ureq` with the `native-tls` feature: SChannel on Windows and Security.framework on a Mac.

Run the tests with `cargo test -p notchbase-net`. None of them touch the network.
