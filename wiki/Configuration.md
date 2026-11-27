# Configuration

Everything notchbase remembers lives in one file, `config.json`: behavior and timing of the notch, which tabs appear and in what order, and every tool's settings and shortcuts. You can change most of it from the notch itself; this page is for editing the file directly.

## Where it is

`config.json` sits next to the notchbase executable:

| Setup | File |
| --- | --- |
| Windows | `config.json` beside `notchbase.exe` |
| macOS app | `~/Library/Application Support/notchbase/config.json` (a `config.json` left inside `notchbase.app/Contents/MacOS/` by an older version moves there on the first start) |
| A checkout run with `cargo run` | `target/debug/config.json` (`target/release/config.json` with `--release`) |

To keep it somewhere else, set `NOTCHBASE_CONFIG` to the full path of the file before starting notchbase. See [Environment variables](#environment-variables).

## How it behaves

- **First run.** If the file doesn't exist, notchbase creates it with every default filled in, and with `updates.seen` set to the version that is running, so a new copy has no changes to announce. If an older notchbase left a `settings.txt` in the data folder, its choices are carried over first (see [Upgrading from settings.txt](#upgrading-from-settingstxt)).
- **Tools fill themselves in.** Whenever notchbase finds a tool that the file doesn't know yet, it adds an entry under `tools` with every setting and shortcut at its default, and adds the tool to `layout.tools`. Tools are never added to `layout.tabs` on their own; list a tool that has a header tab there yourself to place it (ids of tools without a header tab are removed). Settings or shortcuts that a tool gains in a later version are added the same way. Values already in the file are never overwritten.
- **Partial files are fine.** Anything missing takes its default, so a file containing only `{ "notch": { "hover_delay_ms": 150 } }` is valid.
- **Unknown keys are ignored.** notchbase rewrites the whole file when it saves a change (for example when you rebind a shortcut in the notch), and keys it doesn't know aren't written back. JSON has no comments, so don't rely on extra keys as notes.
- **Find it from the notch.** Settings › Tools shows the path of `config.json` and the tools folder, each with an Open button.
- **Edits apply while running.** notchbase checks the file about once a second and applies what changed: tool settings reach tools through [`Subscription::settings`](tools/Subscriptions.md#settings), changed shortcuts are registered again, tabs are rearranged, timings take effect. Changing `layout.tools` reorders the tools without restarting them. Changing any tool's `enabled` restarts all tools, which begin again from their initial state.
- **Mistakes never overwrite the file.** If the file isn't valid JSON, or a value has the wrong type (`"hover_delay_ms": "fast"`), notchbase shows the problem, with its line number, as a message in the notch, and keeps going: with the last good settings if it was already running, with the defaults if it's just starting. Changes you make in the notch still apply, but notchbase won't save anything to the file until you fix it ("config.json has a mistake, so changes aren't saved until it's fixed").

## The default file

This is the file a first run writes on Windows with the built-in tools. Plugins you've installed get entries of their own. On macOS, shortcuts are written with `Option` in place of `Alt` (`Ctrl+Option+S`), and the calendar's `source` starts as `"apple"`. `updates.seen` is the version that wrote the file, 0.14.0 here.

```json
{
  "version": 4,
  "displays": {
    "all": false
  },
  "notch": {
    "open_on_hover": true,
    "wake_at_edge": true,
    "flee_from_keys": true,
    "haptics": true,
    "dodge_menus": true,
    "lock_screen": true,
    "hover_delay_ms": 120,
    "close_delay_ms": 220,
    "start_page": "tools",
    "collapse_before_capture_ms": 260
  },
  "notices": {
    "message_ms": 3500,
    "volume_ms": 1600,
    "brightness_ms": 1600,
    "device_connected_ms": 3200,
    "device_disconnected_ms": 2200,
    "shelf_ms": 4000
  },
  "music": {
    "in_notch": true,
    "dance": true,
    "open_to_player": true,
    "reveal_ms": 4000,
    "hover_reveal_ms": 300
  },
  "volume": {
    "indicator": true,
    "quiet_after_mixer_ms": 1200
  },
  "brightness": {
    "indicator": true
  },
  "bluetooth": {
    "indicator": true,
    "battery": true,
    "poll_ms": 1500
  },
  "media": {
    "poll_ms": 800
  },
  "stats": {
    "refresh_ms": 1000
  },
  "assistant": {
    "enabled": true,
    "provider": "claude",
    "questions_in_notch": true,
    "progress_in_notch": true
  },
  "updates": {
    "automatic": true,
    "seen": "0.14.0"
  },
  "annotate": {
    "color": "#EC545A",
    "size": 5.0
  },
  "find": {
    "enabled": true,
    "excluded": [],
    "volumes": true,
    "max_entries": 0,
    "learn": true,
    "recents": true
  },
  "layout": {
    "tabs": [
      "media",
      "tools",
      "shelf",
      "agents",
      "stats"
    ],
    "tools": [
      "color-picker",
      "screenshot",
      "screen-recorder",
      "timer",
      "keep-awake",
      "alerts",
      "calendar",
      "battery",
      "camera-mic",
      "downloads"
    ]
  },
  "tools": {
    "alerts": {
      "enabled": true,
      "shortcuts": {},
      "settings": {
        "enabled": true,
        "port": 47832,
        "token": ""
      }
    },
    "calendar": {
      "enabled": true,
      "shortcuts": {},
      "settings": {
        "source": "ical",
        "link": "",
        "heads_up": 10.0,
        "nudge": true,
        "all_day": true,
        "setup_skipped": false,
        "view": "week",
        "week_start": "auto"
      }
    },
    "color-picker": {
      "enabled": true,
      "shortcuts": {
        "pick": "Ctrl+Shift+C"
      },
      "settings": {
        "format": "hex"
      }
    },
    "screen-recorder": {
      "enabled": true,
      "shortcuts": {
        "record": "Ctrl+Alt+R"
      },
      "settings": {
        "microphone": false,
        "desktop_audio": false,
        "microphone_id": "",
        "folder": "",
        "frame_rate": "auto",
        "dim_outside": true
      }
    },
    "screenshot": {
      "enabled": true,
      "shortcuts": {
        "capture": "Ctrl+Alt+S"
      },
      "settings": {
        "copy": true,
        "save": false,
        "folder": "",
        "format": "png"
      }
    },
    "timer": {
      "enabled": true,
      "shortcuts": {
        "toggle": "Ctrl+Alt+T"
      },
      "settings": {
        "last_ms": 300000
      }
    }
  }
}
```

All durations are in milliseconds.

## Reference

### version

| Key | Default | Meaning |
| --- | --- | --- |
| `version` | `7` | The format of this file. Leave it as it is: notchbase uses it to upgrade older files once |

### displays

| Key | Default | Meaning |
| --- | --- | --- |
| `displays.all` | `false` | Show an island on every other display too, not only the main one |

### notch

| Key | Default | Meaning |
| --- | --- | --- |
| `notch.open_on_hover` | `true` | Open the notch when the pointer rests on it. Off, it opens on a click instead: resting on the drawn-out island lets it take the click, and moving away without clicking hands clicks back to what's underneath |
| `notch.wake_at_edge` | `true` | An island (and a docked song or activity) ignores the pointer until it reaches the thin strip above the browser tabs: the top 7 points of the screen within 16 points of it, or the top 4 points anywhere near it. Tab close and new-tab buttons under it never wake it. Once awake it follows the pointer until it moves away. On a Mac's real notch it instead means the notch only opens while the pointer is on the notch itself, not beside or just below it. **Settings › Notch › Wake at the top edge** |
| `notch.haptics` | `true` | A light trackpad tap as the panel or an ear opens, as switches flip and as a picker wheel rolls (Mac). **Settings › Notch › Haptics** |
| `notch.dodge_menus` | `true` | On a MacBook, slide the notch's left side away when the frontmost app's menus reach it, and back when they don't (needs Accessibility). **Settings › Notch › Make room for menus**. See [Notch](Notch.md#making-room-for-menus-notchmovesdodgers) |
| `notch.lock_screen` | `true` | Keep the notch on the Mac's lock screen (with nothing in it clickable) and show a padlock that opens as you unlock. Off takes effect at the next start. **Settings › Notch › Show on the lock screen**. See [Notch](Notch.md#the-lock-screen) |
| `permissions.full_disk_asked` | `false` | Set once notchbase has asked for Full Disk Access to see your Focus, so it never asks again |
| `permissions.find_disk_asked` | `false` | Set once Find has asked for Full Disk Access (one notch message and one Notifications entry), so it never asks again |
| `permissions.find_access_hidden` | `false` | Set when the "See every file" row at the top of Find was dismissed |
| `permissions.walkthrough` | `"pending"` | `"done"` once the first-launch walkthrough is finished. While `"pending"` the notch opens on it and nothing in the background asks for a permission. Configs older than version 6 are upgraded to `"done"` (see [Platform: macOS](Platform-macOS.md#the-first-launch-walkthrough)) |
| `permissions.asked` | `[]` | The permissions the walkthrough has put to you (`accessibility`, `bluetooth`, `calendar`, `downloads`, `automation`, `system_audio`, `screen_recording`, `full_disk`). Background features never ask for these again; **Settings → Permissions** still can |
| `notch.flee_from_keys` | `true` | Holding any key, or starting a mouse press, with the pointer near the notch makes it slide out from under the pointer. Off, it stays where it is for both. **Settings › Notch › Move away for keys and clicks** |
| `notch.hover_delay_ms` | `120` | How long the pointer rests on a peek before the notch starts straining outward; the notch opens 380 ms after that. A config.json from before version 2 that still had the old default of 300 moves to 120 once; any other value is kept. The pointer has to stay within 12 points for the whole hold |
| `notch.close_delay_ms` | `220` | How long after the pointer leaves before the notch closes |
| `notch.start_page` | `"tools"` | The tab the notch opens on: a tab's key (`media`, `tools`, `shelf`, `sound`, `stats`, or a tool with a header tab such as `calendar`). If that tab isn't shown, it opens on Tools, or else the first tab; `sound` without a Sound tab opens Media, which holds the mixer |
| `notch.collapse_before_capture_ms` | `260` | When a color pick, screenshot or recording starts while the notch is open, notchbase closes it first so it isn't in the picture, waiting up to this long |

### notices

How long things shown in the closed notch stay up.

| Key | Default | Meaning |
| --- | --- | --- |
| `notices.message_ms` | `3500` | Text messages, including those tools send with `Command::notify` |
| `notices.volume_ms` | `1600` | The volume indicator |
| `notices.brightness_ms` | `1600` | The brightness indicator |
| `notices.device_connected_ms` | `3200` | A Bluetooth device connecting |
| `notices.device_disconnected_ms` | `2200` | A Bluetooth device disconnecting |
| `notices.shelf_ms` | `4000` | The result message on the Shelf after an action (converting, compressing…) |

Tool notices set their own duration in code (see [Surfaces](tools/Surfaces.md#notice)).

### music

| Key | Default | Meaning |
| --- | --- | --- |
| `music.in_notch` | `true` | Show what's playing in the closed notch |
| `music.dance` | `true` | Let the notch's edge move with the music while something plays |
| `music.open_to_player` | `true` | Opening the notch while music plays shows the Media tab, unless the shelf holds something: then the Shelf tab opens first |
| `music.reveal_ms` | `4000` | How long the closed notch shows a track when it starts or changes |
| `music.hover_reveal_ms` | `300` | How long the track stays up after the pointer leaves the closed notch; while the pointer is near it stays. A config.json from before version 7 that still had the old default of 1500 moves to 300 once; any other value is kept |

### volume

| Key | Default | Meaning |
| --- | --- | --- |
| `volume.indicator` | `true` | Show volume changes in the notch, in place of the system's volume and media pop-ups |
| `volume.quiet_after_mixer_ms` | `1200` | After you change the main volume with the mixer's Output slider (on the Media or Sound tab), the indicator stays quiet this long, so it doesn't pop up while you drag |

### brightness

| Key | Default | Meaning |
| --- | --- | --- |
| `brightness.indicator` | `true` | macOS only. Show brightness key presses in the notch, in place of the system's brightness pop-up. **Settings › Popups › Brightness indicator**. Elsewhere it does nothing |

### bluetooth

| Key | Default | Meaning |
| --- | --- | --- |
| `bluetooth.indicator` | `true` | Show a notice when a Bluetooth device connects or disconnects |
| `bluetooth.battery` | `true` | Show devices' battery levels when they report them |
| `bluetooth.poll_ms` | `1500` | How often devices are checked. Values under 250 are treated as 250 |

### media

| Key | Default | Meaning |
| --- | --- | --- |
| `media.poll_ms` | `800` | How often what's playing is checked. Values under 100 are treated as 100 |

### stats

| Key | Default | Meaning |
| --- | --- | --- |
| `stats.refresh_ms` | `1000` | How often the Stats tab refreshes. Values under 250 are treated as 250 |

### assistant

notchbase can show usage for, and answer prompts from, a coding assistant running on your computer.

| Key | Default | Meaning |
| --- | --- | --- |
| `assistant.enabled` | `true` | Turn the assistant features on or off |
| `assistant.provider` | `"claude"` | `"claude"` for Claude Code, or `"chatgpt"` for Codex |
| `assistant.questions_in_notch` | `true` | Show the assistant's prompts and questions in the notch, so you can answer them there |
| `assistant.progress_in_notch` | `true` | Show a small live activity while the assistant works, and a "done" notice when a turn finishes |

### updates

| Key | Default | Meaning |
| --- | --- | --- |
| `updates.automatic` | `true` | Check GitHub for new releases and install them when the notch is idle. See [Updates](Updates.md) |
| `updates.seen` | `""` (a first run writes the running version) | The version whose [What's New](Updates.md#whats-new) notes you have seen. notchbase writes it itself, when you press **View Changes** or **Done**, leave the page for a tab, Settings or Find, or have had the page up for a moment, so it is something it remembers for you. Empty or missing means never seen, which is how a file from before 0.14.0 reads, and such a copy shows the notes once. Set it to an older version, or `""`, to be shown them again: the page then opens by itself the next time you open the notch |

### awake

| Key | Default | What it does |
| --- | --- | --- |
| `awake.force` | `false` | Keep the computer awake all the time. Only settable here: the Keep Awake tool shows "Always awake" and cannot turn it off, and it holds through restarts and updates |
| `awake.hold_lid_closed` | `true` | macOS: while something keeps the Mac awake, also keep it awake with the lid closed (asks for an administrator once). Shown as **Keep awake when the lid is closed** in Keep Awake's settings, but saved here, not under `tools.keep-awake` |

A Keep Awake session and a running Timer are also remembered across restarts and updates.

### snap

| Key | Default | What it does |
| --- | --- | --- |
| `snap.enabled` | `true` | Drag a window to the notch to lay it out; hold Shift over a layout to send it to the next display instead. "Fill the free space" gives it the biggest empty area left on the screen. The top row lays windows side by side (halves, thirds); the row under it stacks them (top and bottom halves under the left and right halves, three stacked thirds under the thirds) |
| `snap.gap` | `8` | Space between windows and screen edges, in points |

The layout you pick for each app is learned in `snap-habits.json` in the data folder; the notch marks your usual one.

### videos

| Key | Default | Meaning |
| --- | --- | --- |
| `videos.enabled` | `true` | Drag a browser tab, link or thumbnail onto the notch to download the video on that page: the left half for the video, the right for only its sound. See [Video downloads](Video-Downloads.md) |
| `videos.quality` | `4320` | The tallest picture to fetch, in lines (144 to 4320). The default is no limit in practice; set `1080` or `720` for smaller files. Only codecs this computer plays are fetched, so above 1080p YouTube needs a Mac whose chip decodes AV1 |
| `videos.audio` | `"m4a"` | The format of a sound-only download. `m4a` keeps the site's AAC untouched; `mp3` re-encodes it |
| `videos.sign_in` | `true` | When a site refuses a download to a stranger (Instagram, Vimeo and Facebook often do), try once more signed in as the browser the tab came from, never on YouTube. Off: such videos fail with "wants you signed in" |

### annotate

The brush of the [annotator](Annotate.md), the window the Screenshot notice's **Annotate** button opens. Its text tool writes in the same colour, and the tool itself is not remembered: a window always opens on the brush. notchbase writes both keys itself whenever you pick another colour or size there, so this section is mostly something it remembers for you.

| Key | Default | Meaning |
| --- | --- | --- |
| `annotate.color` | `"#EC545A"` | The brush colour, as six hex digits. Written as upper-case `#RRGGBB`; read with or without the `#`, in any case. Anything else counts as the default red. The window offers five colours (`#EC545A`, `#F2C14E`, `#3FC48A`, `#F4F3F0`, `#0E0E0F`); another colour set here draws in that colour, with no swatch marked |
| `annotate.size` | `5.0` | The brush width in points on screen, from 2 to 32. The value is tidied both when the file is read and when it is written: one outside the range is pulled to the nearest end, and a number too large to be a size at all (such as `1e39`) counts as the default 5, so it cannot stop the file from being saved again. Written in half-point steps when you change the brush. The text tool's letters follow it too, at `10 + 2 x size` points on screen |

An edit here is used by the next annotator window that opens; one that is already open keeps its brush.

### find

| Key | Default | Meaning |
| --- | --- | --- |
| `find.enabled` | `true` | Index every file and folder on local volumes and search them from the Find button. Off stops the scan, the watcher and every write; the page shows "Find is off". The same switch as **Settings › Find › Find files** |
| `find.excluded` | `[]` | Left out of the index. An entry with a separator or starting with `~` is a path (`"~/Movies/Raw"`, `"D:\\Backups"`); a bare word is a folder name left out wherever it appears (`"node_modules"`). Adding one removes what was indexed and learned below it at once |
| `find.volumes` | `true` | Also index other local mounted volumes (never network shares). Fixed drives on Windows |
| `find.max_entries` | `0` | `0` sizes the index from memory (about 6.7 million entries on 16 GB). A number overrides it, up to 50 million. Past the limit Find drops caches first, then build folders, then other volumes, then system folders, never your own files |
| `find.learn` | `true` | Learn what you open for what you type, to rank and suggest. Off deletes `usage.bin` and stops learning. **Settings › Find › What Find learned › Clear** empties it without turning it off |
| `find.recents` | `true` | macOS: read the files you opened recently from Spotlight to start learning before your first search |
| `find.hidden` | `false` | List hidden files and everything that is not yours (system, build and cache files, the contents of hidden folders and packages) together with your own. Off, Find lists a hidden file only when nothing of yours matches, when its name is typed with its leading dot, or when you picked it before; what is not yours only for a path or once you picked it |
| `find.pins` | `[]` | What is pinned under the resting bar of Find, at most four, in order. Each entry has `path`, `name` (the name it showed when it was pinned), `app` and `folder`. Changed from the right-click menu of a result and by dragging the pins |
| `find.system_key` | `true` | Open Find with the key of the system's own search: ⌘Space on macOS (needs Accessibility access), a lone Windows key on Windows. Spotlight or the Start menu no longer opens on that key. The same switch as **Settings › Find › Replace Spotlight** / **Replace the Start menu** |

Edits here apply while notchbase runs. See [Find](Find.md).

### dock

macOS only (Windows had it until 0.33; updating there quits an old dock and puts the taskbar back). [notchbase dock](../dock/README.md) is off for everyone until it's turned on. Until then notchbase never starts it and the macOS Dock is left as it is. The dock's own settings live in its `dock.json`, which **Settings › Dock** edits (see [Pages](Pages.md#dock-pagessettingsdockrs)).

| Key | Default | Meaning |
| --- | --- | --- |
| `dock.enabled` | `false` | Set by the **notchbase dock** switch in Settings › Dock. While `true`, notchbase starts the dock when it starts: the copy inside `notchbase.app/Contents/Helpers`, or, for a bare build, `notchbase dock.app` wherever macOS knows it. Off quits it, and the macOS Dock comes back. A copy that finds the dock already running without this key (it was started by hand before the setting existed) sets it to `true` and keeps it running |
| `dock.introduced` | `false` | Set once the one-time "Meet notchbase dock" notice has shown, on either answer (**Turn On** opens Settings › Dock; **Not Now** says where to find it), or as soon as the switch is used. A file from a version without the notice has no such key, so an update shows it once, after the first-launch walkthrough and once the notch is quiet. Set it back to `false` to see it again |

### layout

| Key | Default | Meaning |
| --- | --- | --- |
| `layout.tabs` | `["media", "tools", "shelf", "agents", "stats"]` | The tabs of the open notch, in order, including tool tabs such as `calendar` |
| `layout.tools` | every tool, in the order they're found | The order of tools on the Tools tab, by tool id |

`layout.tabs` takes the keys of the built-in tabs (`media`, `tools`, `shelf`, `sound`, `stats`, `agents`) and the ids of tools that have a header tab (the bundled `calendar`). The order in the list is the order of the tabs, so any tab can go anywhere; leave a built-in key out to hide that tab. `sound` isn't listed by default because the Media tab holds the mixer; add it for a tab with the mixer alone. A config.json from before version 3 whose tabs were still the old default, `["media", "tools", "shelf", "sound", "stats"]` (with or without `calendar` after it), drops `sound` once; any other list is kept. A config.json from before version 4 whose tabs don't list `agents` gains it once: just before `stats` when `stats` is listed, otherwise right after the last built-in key (so ahead of any tool tabs such as `calendar` that follow them), otherwise at the end. The old default `["media", "tools", "shelf", "stats"]` becomes today's default that way, and a list that already names `agents` is left as it is. The Agents tab only shows while an assistant is chosen in Settings, so listing `agents` with the assistant off hides it until you turn one on. A tool with a header tab that isn't listed still gets its tab, after the listed ones; turn the tool off on the Tools tab to hide it. Keys that don't match anything are skipped. The Settings page is always reachable from the gear and isn't listed.

A tool without a header tab can't be listed; its page opens from its tile on the Tools tab.

`layout.tools` sets the order of the icons on the Tools tab, which sit in one row that scrolls sideways once there are more than five. Dragging an icon to a new place on the Tools tab rewrites this list. notchbase adds each new tool to the end. Tools missing from the list come after the listed ones.

```json
{
  "layout": {
    "tabs": ["tools", "media", "shelf"],
    "tools": ["screen-recorder", "screenshot", "color-picker", "alerts"]
  }
}
```

This shows three tabs (Tools, Media, Shelf), hides Stats, and puts Record Screen first on the Tools tab. Changing `layout.tools` while notchbase runs moves the icons without restarting anything.

### tools

One entry per tool, keyed by the tool's id:

```json
{
  "tools": {
    "color-picker": {
      "enabled": true,
      "shortcuts": { "pick": "Ctrl+Shift+C" },
      "settings": { "format": "rgb" }
    },
    "screenshot": {
      "shortcuts": { "capture": null },
      "settings": { "save": true, "folder": "D:/Screenshots", "format": "jpeg" }
    }
  }
}
```

| Key | Default | Meaning |
| --- | --- | --- |
| `tools.<id>.enabled` | `true` | `false` turns the tool off completely: it isn't loaded, so it has no tile, page, live activity or shortcuts. Changing it while notchbase runs restarts all tools |
| `tools.<id>.shortcuts.<shortcut>` | the tool's default | A key combination such as `"Ctrl+Alt+P"`, or `null` for no shortcut |
| `tools.<id>.settings.<key>` | the tool's default | The value of one of the tool's settings |

notchbase writes every shortcut and setting of a tool into its entry, at their defaults, the first time it sees the tool, so you'll normally find them all listed. A key that's missing anyway means the default; `null` for a shortcut means deliberately unbound, and stays that way. Key combinations are written as modifiers and a key joined with `+`, such as `Ctrl+Shift+C` or `Alt+F5`; see [Writing key combinations](tools/Settings-and-Shortcuts.md#writing-key-combinations) for every accepted name.

Which settings a tool has is up to the tool. Each one declares them in its manifest (see [Settings and Shortcuts](tools/Settings-and-Shortcuts.md)). Ranges and choices shown in the notch aren't enforced in this file, so stick to sensible values; well-written tools clamp what they read. Number settings are stored as JSON numbers and may be written with a decimal point (`47832.0`); either form works.

#### Built-in tools

**`color-picker`** (Pick Color)

| Shortcut | Default |
| --- | --- |
| `pick` | `Ctrl+Shift+C` |

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `format` | `"hex"`, `"rgb"` or `"hsl"` | `"hex"` | How picked colors are copied: `#3A7BFF`, `rgb(58, 123, 255)` or `hsl(220, 100%, 61%)` |

**`screenshot`** (Screenshot)

| Shortcut | Default |
| --- | --- |
| `capture` | `Ctrl+Alt+S` |

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `copy` | bool | `true` | Put each screenshot on the clipboard |
| `save` | bool | `false` | Also save each one to a file. Screenshots that aren't copied are always saved. The file is the screenshot as captured: what is drawn on it with [Annotate](Annotate.md) reaches the clipboard only |
| `folder` | text | `""` | Where files go. Empty uses the default folder. Hidden from the Settings page |
| `format` | `"png"` or `"jpeg"` | `"png"` | File format. Hidden from the Settings page |

**`screen-recorder`** (Record Screen)

| Shortcut | Default |
| --- | --- |
| `record` | `Ctrl+Alt+R` |

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `microphone` | bool | `false` | Record the microphone (the Mic chip on the tile) |
| `desktop_audio` | bool | `false` | Record the computer's sound (the Desktop chip) |
| `microphone_id` | text | `""` | Which microphone; empty uses the system default (the tile's dropdown) |
| `folder` | text | `""` | Where recordings go. Empty uses the default folder |
| `frame_rate` | `"auto"`, `"60"` or `"30"` | `"auto"` | Auto records at 60 fps, or 30 on very large areas |
| `dim_outside` | bool | `true` | Shade the screen outside the recorded area |

Only `frame_rate` and `dim_outside` appear on the Settings page; the others are set from the tile or here.

**`alerts`** (Alerts)

No shortcuts.

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Run the local webhook. Off stops it; `tools.alerts.enabled` turns off the whole tool |
| `port` | number | `47832` | The port the webhook listens on, from 1024 to 65535 |
| `token` | text | `""` | A token callers must send; empty means none. Hidden from the Settings page |

See [Alerts](Alerts.md) for the webhook itself.

**`timer`** (Timer)

| Shortcut | Default |
| --- | --- |
| `toggle` | `Ctrl+Alt+T` |

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `last_ms` | number | `300000` | The length of the last timer you set, in milliseconds, so the next one starts from it. Hidden from the Settings page |

**`keep-awake`** (Keep Awake)

| Shortcut | Default |
| --- | --- |
| `toggle` | none |

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `length` | `"0"`, `"15"`, `"30"`, `"60"`, `"120"` or `"240"` | `"0"` | Minutes one press keeps the computer awake; `"0"` is until you turn it off |
| `in_notch` | bool | `true` | Show a cup in the notch while it is on |

**`battery`** (Battery), **`camera-mic`** (Camera & Microphone), **`downloads`** (Downloads), **`focus-status`** (Focus)

No shortcuts. See [System tools](System-Tools.md).

| Tool | Setting | Default | Meaning |
| --- | --- | --- | --- |
| `battery` | `charger` | `true` | Show when the charger is plugged in or unplugged |
| `battery` | `low` | `true` | Warn at 20% and again at 10% |
| `battery` | `full` | `true` | Show when the battery is full |
| `camera-mic` | `camera` | `true` | Show a green dot while the camera is on |
| `camera-mic` | `microphone` | `true` | Show an orange dot while an app is listening |
| `downloads` | `shelve` | `true` | Put a finished download on the Shelf |
| `downloads` | `announce` | `true` | Show the file when it lands |
| `downloads` | `offer_unzip` | `true` | Ask to unzip a .zip when it lands, right next to it |
| `focus-status` | `announce` | `true` | Show when a Focus turns on or off (needs Full Disk Access on a Mac) |

**`calendar`** (Calendar)

No shortcuts. It has its own header tab; see [Calendar](Calendar.md).

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `source` | `"apple"` or `"ical"` | `"apple"` on macOS, `"ical"` elsewhere | Where events come from. Only shown on a Mac |
| `link` | text | `""` | One or more secret iCal addresses, separated by spaces. Shown as dots, and only while the source is `ical` |
| `heads_up` | number | `10` | Minutes before a meeting that it appears in the closed notch, 0 to 30 |
| `nudge` | bool | `true` | Shake the notch with a notice as a meeting starts |
| `all_day` | bool | `true` | Show all-day events on the page |
| `week_start` | `"auto"`, `"monday"` or `"sunday"` | `"auto"` | The first day of the week in the month view. Auto is Sunday with a 12-hour clock and Monday with a 24-hour one |
| `setup_skipped` | bool | `false` | Set when "Add a calendar to see your events" is dismissed. Hidden |
| `view` | `"week"` or `"month"` | `"week"` | The last view chosen: `week` is the Days view. Hidden |

Local events, event colors and cached iCal feeds live in the tool's data folder (`tools/calendar` inside the data folder, see [Environment variables](#environment-variables)): `events.json`, `colors.json` and `feeds/`.

## Environment variables

| Variable | Changes | Default |
| --- | --- | --- |
| `NOTCHBASE_CONFIG` | The path of `config.json` | `config.json` next to the executable, or in the data folder for the macOS app |
| `NOTCHBASE_TOOLS_DIR` | The one folder plugins are loaded from | Both the `tools` folder in the data folder (below) and a `tools` folder next to the executable |
| `NOTCHBASE_DATA_DIR` | notchbase's data folder: where an old `settings.txt` is looked for, where the [usage history](Stats.md#history-statshistory) `usage-history.jsonl` is kept, and where each tool's own folder lives (`tools/<id>` inside it), and where Find keeps its `find` folder (on every system, including Windows) | Windows: `%APPDATA%\notchbase`. macOS: `~/Library/Application Support/notchbase`. Linux: `$XDG_DATA_HOME/notchbase`, or `~/.local/share/notchbase` |

Set them before notchbase starts. See [Plugins and Distribution](tools/Plugins-and-Distribution.md#installing) for how plugins in the tools folder are found.

## Upgrading from settings.txt

Older versions of notchbase kept a few choices in `settings.txt` in the data folder. The first time notchbase runs without a `config.json`, it reads that file and carries these over:

| `settings.txt` | `config.json` |
| --- | --- |
| `pick_color` | `tools.color-picker.shortcuts.pick` |
| `screenshot` | `tools.screenshot.shortcuts.capture` |
| `record_screen` | `tools.screen-recorder.shortcuts.record` |
| `assistant` (`claude` or `chatgpt`) | `assistant.provider` |
| `assistant_enabled` | `assistant.enabled` |
| `questions_in_notch` | `assistant.questions_in_notch` |
| `volume_indicator` | `volume.indicator` |
| `all_displays` | `displays.all` |
| `record_microphone` | `tools.screen-recorder.settings.microphone` |
| `record_desktop_audio` | `tools.screen-recorder.settings.desktop_audio` |
| `record_microphone_device` | `tools.screen-recorder.settings.microphone_id` |

A shortcut set to `none` becomes `null` (no shortcut), and shortcuts are rewritten in the friendly form (`shift+control+KeyC` becomes `Ctrl+Shift+C`). Values that don't make sense are skipped and keep their defaults. `settings.txt` itself is left alone and isn't read again once `config.json` exists.
