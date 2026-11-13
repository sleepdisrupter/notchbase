# src

The app. One executable is the app, its installer, the assistant's hook and a few commands: `launch.rs` reads the first argument to tell which, and `app/` runs the app itself.

Every folder here is one part of notchbase:

| Folder | What's in it |
| --- | --- |
| `app` | The running app: owns every service and the notch windows, and carries out what the notch asks for |
| `notch` | The notch itself, from the closed shape to the open panel |
| `pages` | The pages the open panel shows |
| `bridge` | The types the app and the notch pass each other |
| `ui` | Drawing pieces the notch and pages share, on top of `crates/paint` |
| `config` | `config.json` |
| `platform` | Everything that differs between macOS and Windows |
| `system` | Small OS watchers: power, Bluetooth, volume keys, Focus, the calendar and more |
| `poll` | Workers, polls and mailboxes the background threads use |
| `tools` | The tool host: built-in tools, plugins and portable tools |
| `media`, `audio` | What's playing, and the per-app volume mixer |
| `shelf`, `filedrag` | The file shelf, and dragging files in and out |
| `capture`, `annotate`, `record` | Screenshots and the color sampler, drawing on screenshots, screen recording |
| `find` | The file search behind the Find page |
| `webvideo` | Downloading the video on a dragged browser tab |
| `snap` | Window layouts |
| `stats` | The Stats page's numbers |
| `assistant` | Claude Code and Codex hooks |
| `update`, `installer`, `whats_new` | Updates, installing, and the notes each release carries |

Each folder has its own README, and [wiki/Architecture.md](../wiki/Architecture.md) shows how they fit together.
