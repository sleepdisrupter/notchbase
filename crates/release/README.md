# notchbase-release

Versions, releases and the install contract. The app's updater and the installer find, download and install a release with this same code, and both sides of an install agree here on its flags, paths and error format.

The app uses it in `src/update` and `src/installer`, and `notchbase-setup` to look up, download and install the newest release. It goes through `notchbase-net` for HTTPS.

- `Version` reads and orders `major.minor.patch` versions, and `Release` is one published build: its version, URL, SHA-256 and size.
- `newest` picks the newest release after a given version, and `platform()` names this computer's builds, like `windows-x86_64` or `macos-aarch64`. An Intel build running under Rosetta asks for `aarch64`.
- `Server` reads the `latest.json` attached to the newest GitHub release (`Manifest`, with a `Build` per platform) and gives this platform's build, or nothing when there is none yet. It fetches `https://github.com/<repo>/releases/latest/download/latest.json` as a plain download, never through the rate-limited GitHub API. `Server::built_in` uses `REPO` (`sleepdisrupter/notchbase`), or the repository named in `NOTCHBASE_RELEASES_REPO` at build time so a fork updates from its own releases. `download` fetches a build and checks its SHA-256 before it replaces anything.
- `install` is the contract: the `--install` and `--uninstall` flags, the installer's `Choices`, where the app goes on each system, unpacking a Mac bundle, the Windows uninstall entry, and the `notchbase: ` prefix on errors so the installer can show them.

The registry parts only exist on Windows. The Mac bundle helpers build everywhere so their tests run everywhere. [wiki/Updates.md](../../wiki/Updates.md) and [wiki/Installing.md](../../wiki/Installing.md) show how it all fits together.

Run the tests with `cargo test -p notchbase-release`. Reading the real Windows registry is an ignored test: `cargo test -p notchbase-release -- --ignored`.
