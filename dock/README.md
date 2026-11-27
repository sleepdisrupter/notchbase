# notchbase dock

A Dock for macOS in the notchbase style. It is a separate app that ships inside `notchbase.app` (in
`Contents/Helpers`, signed with notchbase's certificate) and is off until someone turns it on, from the one-time
"Meet notchbase dock" notice or **Settings › Dock** in notchbase. From then on notchbase starts it whenever notchbase
starts. Every setting below can be changed on that page. `scripts/bundle-dock-macos.sh` still builds it on its own in
`target/release` for working on it.

The body is one calm slab of black liquid with the floating island's grey rim. Only the icon under the pointer reacts:
it lifts and grows a little on a soft rise in the liquid, and its neighbours step aside just enough to keep a gap. The
tooltip slides from icon to icon instead of popping again, and an icon sinks slightly while it's pressed. Clicking an
app gives its icon a quick squash and stretch, and a droplet jumps off the body on a thin neck and snaps back in. While
an app launches its dot breathes, and it settles solid once the app is running. New apps' slots grow out like a
droplet, and a quitting app's slot pinches off and melts back in.

## What it does

- Finder first, then your pinned apps, then running apps you haven't pinned, a separator, your Dock's folders (the
  Downloads stack) and the Trash.
- The first run copies the pinned apps, folders, icon size and magnification from the system Dock. From then on the dock
  keeps its own list in `dock.json` and never writes the system Dock's list.
- Click an icon to open or switch to the app (a new window if it has none, like the Dock). Click a folder to see its
  newest items, and click the Trash to open it.
- Right-click, Control-click or hold an icon for its menu:
  - Apps: Open, Keep in Dock / Remove from Dock, Show in Finder, Hide, Quit.
  - Claude, ChatGPT and Codex also show their plan's usage under the buttons: the 5-hour and weekly limits (only the
    weekly one on a plan without a 5-hour limit), each with a bar in the app's colour and when it resets. The numbers
    come from `crates/usage`, the same readers as notchbase's Stats page: Claude's from the Claude app's history file,
    or Anthropic's usage endpoint with Claude Code's sign-in; ChatGPT's from the logs the Codex CLI writes, so they're
    the plan's Codex limits. Reading starts when you rest on the icon.
  - Folders: Open, Show in Finder, Remove from Dock.
  - The Trash: Open, Empty Trash (asks twice).
- Drag an app to reorder it. Pull it up and away from the dock to remove it.
- Drop files on an app to open them with it, or on the Trash to trash them. Drop an `.app` on the dock to keep it there.
- Running dots update live as apps launch and quit, from NSWorkspace notifications.
- Apps' badges (unread counts and the like) show as red pills on their icons' top right, like the Dock's. They're read
  every second off the hidden system Dock through Accessibility (`AXStatusLabel`), so they need that permission too
  (`badges.rs`, `titlebar::dock_badges`).
- Rest on a running app's icon for about a third of a second and its open windows show side by side above it, with
  their titles. Click one to bring it to the front (a minimized one is restored), or use its close button. Move along
  the dock and the panel glides to the next app. It goes a moment after the pointer leaves the icon and the panel.
  Window pictures need Screen Recording: macOS asks once to allow "notchbase dock" to record the screen (System
  Settings > Privacy & Security > Screen & System Audio Recording). Without it the panel shows titles and the app's
  icon. Raising a single window uses Accessibility, the same permission `avoid_windows` asks for.
- When Mission Control, App Exposé, Launchpad or the system Dock come up, the dock fades out in place (about 200ms,
  settling 3pt) and fades back when they go.
- **Position** (`side` in `dock.json`: `bottom`, `left` or `right`): the dock stands along the bottom of the screen or
  down either side, centred. Everything works the same there: the hover lift, the dots and download bars, dragging the
  separator to size it (sideways), menus, window previews and stacks (beside it, upright), minimizing and restoring
  (the window flies sideways into its tile), coming out from that edge over a full-screen app, and keeping zoomed,
  filled and double-clicked windows clear of it. Stacks open as a grid on a side. `side.rs` turns the dock's own
  layout (drawn as if along the bottom) onto its window; what has to be read stays upright.

## The system Dock

While it runs, the dock hides the system Dock the way uBar does:

1. It saves your `com.apple.dock` `autohide` and `autohide-delay` to `saved-dock.json`.
2. It sets `autohide` to true and `autohide-delay` to 1,000,000 seconds, then restarts the Dock.

Your settings come back when the dock quits: a normal quit, `--quit`, Ctrl-C, SIGTERM, SIGHUP or a panic. If the dock is
killed outright, the next start (or `--restore-dock`) restores from the saved file before hiding anything again, so
your own values are never overwritten. If the file is lost while the Dock is still hidden, `--restore-dock` notices the
long delay and shows the Dock again.

```bash
notchbase-dock --quit           # quit the running dock; the system Dock comes back
notchbase-dock --restore-dock   # the same, and also repairs a Dock left hidden by a crash
```

If all else fails:

```bash
defaults delete com.apple.dock autohide-delay; defaults write com.apple.dock autohide -bool false; killall Dock
```

## Files

Everything lives in `~/Library/Application Support/notchbase dock/` (or `$NOTCHBASE_DOCK_DIR`):

| File | What |
| --- | --- |
| `dock.json` | Pinned apps, folders, `tile_size`, `large_size`, `magnification`, `launch_effect`: `"pop"` (the default, all inside the dock), `"window"` (black liquid flows from the icon into the app's window) or `"off"`, `show_names` (name tooltips, off by default), `avoid_windows` (keeps zoomed and tiled windows above the dock, on by default) and `window_previews` (on by default) |
| `saved-dock.json` | Your Dock settings while they're hidden. It exists only while the dock runs or after a crash |
| `dock.pid` | The running dock's pid |

## Starting and stopping it from notchbase (later)

The interface for notchbase is:

- **Start:** launch `notchbase dock.app` (or the `notchbase-dock` binary). A second start sees the live pid in
  `dock.pid`, prints "already running" and exits 0.
- **Is it running?** `notchbase-dock --status` exits 0 with the pid, or 1. You can also read `dock.pid` and check the
  process with `kill(pid, 0)`.
- **Stop:** `notchbase-dock --quit`, or send SIGTERM to the pid in `dock.pid`. Either way the dock restores the system
  Dock and removes `dock.pid` before it exits. `--quit` waits up to five seconds for that.
- **Repair:** `notchbase-dock --restore-dock` is safe to run at any time, for example from notchbase's own quit or
  uninstall.

## Building

```bash
cargo run -p notchbase-dock                    # run from the repo
scripts/bundle-dock-macos.sh                   # target/release/notchbase dock.app
NOTCHBASE_DOCK_KEEP_SYSTEM_DOCK=1 cargo run -p notchbase-dock   # try it without hiding the system Dock
```

Headless pictures, with nothing shown and the system Dock left alone:

```bash
notchbase-dock --snapshot out.png --scene hover
```

The scenes are `rest`, `hover`, `tooltip-ooze`, `menu`, `menu-claude`, `menu-chatgpt` (add `-light` to either), `menu-ooze`, `stack`, `previews`, `previews-ooze`, `swell`, `arrive`, `leave` and
`pop:<seconds>`.

## How it's built

- `dock.rs`: tiles, the pointer, menus, stacks, drags and drops. It turns everything into liquid blobs, then paints the
  icons, dots and labels on top.
- `liquid.rs`: the body. It's a signed distance field of continuous-corner boxes, melted together with a smooth minimum
  and shaded solid black, with a faint wet sheen and the notch's grey keyline.
- `pop.rs`: the `"window"` launch effect. It's drawn with the same liquid as the body, in a click-through window just
  under the dock.
- `layout.rs`: lays the row out, widening only the hovered icon's slot by as much as it needs.
- `tiles.rs`: which tiles show, in what order, and how they come and go.
- `real_dock.rs`, `lifecycle.rs`: hiding and restoring the system Dock, the pid file, signals and the panic hook.
- `windows.rs`: an app's windows for the previews: listing them, capturing pictures on a thread of their own, and raising
  or closing one through Accessibility.
- `avoid.rs`: keeps zoomed, filled and tiled windows clear of the dock, on whichever edge it stands.
- `side.rs`: which edge the dock stands on, and the map from its layout to its window.
- `glass.rs`: the blur behind the dark glass, masked to the liquid's outline.
- `flight.rs`: minimizing and restoring; the liquid under the window is slices that follow its bent sides exactly
  (`Blob::slice` in `liquid.rs`), between rounded ends.
- `mac.rs`: AppKit (running apps, icons, opening, the Trash, the pointer, windows).

Springs and fonts come from `crates/paint`, the same ones the notch uses. Frames are drawn only while something moves,
paced by the display (120 Hz on ProMotion).

Known limits: the system Dock stays along the bottom while it's hidden, so in Mission Control (where macOS shows it
anyway) a dock on a side doesn't cover it.

## Windows

The dock ran on Windows too until notchbase 0.33, which made it macOS only. notchbase on Windows no longer carries
it: on its first start after the update it asks a dock an older version left running to quit (the dock puts the
taskbar back on its way out), puts the taskbar back itself if one was stopped with it hidden, and deletes the copies
older versions wrote to `%LOCALAPPDATA%\notchbase\dock` (`system::dock::retire`).
