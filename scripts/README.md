# scripts

Packaging, bundling and releasing.

| Script | What it does |
| --- | --- |
| `bundle-macos.sh` | Builds the Mac `notchbase.app` in release mode, with notchbase dock inside it in `Contents/Helpers` |
| `bundle-dock-macos.sh` | Builds `notchbase dock.app` on its own in `target/release`, for working on the dock |
| `build-sdk.sh`, `build-sdk.ps1` | Packs the SDK, the Notes example and the tool docs into `Distributable/notchbase-sdk.zip` |
| `build-skill.sh`, `build-skill.ps1` | Packs the notchbase skill and the whole wiki into `Distributable/notchbase-skill.zip` |
| `release-github.sh` | Builds the Mac release (both architectures, the installer and both zips), writes `latest.json` and publishes it as a GitHub release. `--dry-run` builds without uploading |
| `release-github.ps1`, `release-github.bat` | Adds the Windows build and `notchbase-setup.exe` to that release and its `latest.json`. `-DryRun` builds without uploading |
| `prune-target.sh` | Clears old build output from `target/`. `--dry-run` only lists it |

The release scripts need the [GitHub CLI](https://cli.github.com), logged in. They release to `NOTCHBASE_RELEASES_REPO`, or to the GitHub repository of the `origin` remote, and build that repository into the app so it updates from there. [wiki/Updates.md](../wiki/Updates.md#releases-on-github) describes a release and [releasing a fork](../wiki/Updates.md#releasing-a-fork), and [wiki/Building-and-Testing.md](../wiki/Building-and-Testing.md) covers bundling.

`Distributable/` is build output and isn't committed: every release attaches both zips. They are built from `sdk/`, `skill/`, the wiki and `tools/notes`.
