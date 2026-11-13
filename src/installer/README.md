# installer

`--install` and `--uninstall`: copying the app to where it lives, registering it, keeping settings across an uninstall, and handing over to the copy that was just put in place.

- `location.rs` knows where the app is and where it goes. A new copy is swapped in with the old one set aside.
- `handoff.rs` starts the new copy, which waits for the old one to exit, and stops any others that are running.
- `macos.rs` puts the bundle in Applications and adds a launch agent for opening at login.
- `windows/` installs for this user only, with a Start menu shortcut, an entry in the Apps list and a Run value for login.
- `other.rs` stands in on other systems, where installing isn't offered.

The installer people download is a separate program in `setup/`, which uses the same install contract from `crates/release`. See [wiki/Installing.md](../../wiki/Installing.md).

Run the tests with `cargo test installer::`.
