# platform

Everything that differs between macOS and Windows. `macos/` and `windows/` each implement the same calls, `mod.rs` picks one as `imp`, and `other.rs` stands in on other systems so the app still builds and its tests run on Linux.

What's in here: window styles for the notch and the overlays, click-through and keyboard focus, which screens get a notch and in which style (`placement.rs`), where notchbase keeps its files (`paths.rs`), one running copy per data folder (`instance.rs`), the local time, shortcut labels, the alert chime, trackpad haptics and screen capture.

Platform code elsewhere follows the same pattern, a `macos`, a `windows` and an `other` with identical signatures. [wiki/Platform-macOS.md](../../wiki/Platform-macOS.md) and [wiki/Platform-Windows.md](../../wiki/Platform-Windows.md) list every system API the app relies on.

Run the tests with `cargo test platform::`, and check the other system with `cargo clippy --target ...` as in the root README.
