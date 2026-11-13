# notchbase-setup

The installer people download. It doesn't carry notchbase inside it. It reads the newest GitHub release's `latest.json` for this computer's build, downloads it with the same code the app's updater uses, checks its SHA-256 and runs the downloaded app with `--install`, so one installer keeps installing the latest version.

It's drawn with notchbase's own UI kit, so it looks like the open notch, and on Windows its bottom edge dances to whatever is playing.

- `src/main.rs` starts the window, and `src/app/` is the `Setup` app: its state in `mod.rs`, the page flow and keys in `flow.rs`, titles and status in `view.rs`.
- `src/pages/` are the welcome, confirm, working and finish pages.
- `src/install/` is the options, looking up the newest release, downloading it, and running the app for the install or uninstall, with a `macos`, `windows` and `other` side.
- `src/dance.rs` is the dancing edge, and `src/window.rs`, `widgets.rs`, `theme.rs` and `brand.rs` the frame, the pieces, the layout and the name.
- `src/shot.rs` saves screenshots of the installer for the docs.

It leans on the shared crates: `notchbase-release` for releases and the install contract, `notchbase-net` for HTTPS, `notchbase-paint` for the window, and `notchbase-spectrum` with `notchbase-wasapi` for the dance. `notchbase-build` links in the Windows icon.

## Building

```sh
cargo build -p notchbase-setup
```

For a build to hand out, `build-installer.bat` writes `dist/notchbase-setup.exe` on Windows, and `./build-installer.sh` builds a universal `Install notchbase.app` for Apple silicon and Intel and packs it into `dist/notchbase-setup.dmg`. Set `NOTCHBASE_SIGN_IDENTITY` to sign it. Like the app, it installs from the repository named in `NOTCHBASE_RELEASES_REPO` at build time, or `sleepdisrupter/notchbase`.

On Windows the app installs for the current user and registers its uninstall entry. On a Mac the downloaded `.app` is unpacked and moved into Applications, and the edge stays still rather than ask for audio permission.

Run the tests with `cargo test -p notchbase-setup`. One test that reproduces a hang with a relaunched app is ignored: `cargo test -p notchbase-setup -- --ignored`. [wiki/Installing.md](../wiki/Installing.md#the-installer-setup) has the full design.
