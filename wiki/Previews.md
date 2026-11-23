# Previews

Most of the notch's states are fleeting: an activity lasts two seconds, a peek needs the cursor in exactly the right spot, a question only appears when Claude asks one. Previews pin the notch in any of these states with sample content, so you can look at it, take screenshots for docs or pull requests, and check a visual change in both styles without reproducing the real situation.

Set `NOTCHBASE_PREVIEW` to a state name and start the app:

```bash
NOTCHBASE_PREVIEW=playing cargo run
```

```powershell
$env:NOTCHBASE_PREVIEW = "playing"; cargo run
```

Names are case-insensitive and surrounding spaces are ignored. The notch states are parsed by `Preview::parse` in `src/notch/preview/mod.rs`; a few names also set something up in the app (`start_preview` in `src/app/handler.rs`).

A preview skips the [single-instance lock](Architecture.md#one-running-copy-per-data-directory), so it starts even while notchbase is running. It still publishes its own `assistant.json`, so give a preview its own `NOTCHBASE_DATA_DIR` when the real app uses the same data directory.

## What a preview does

In `Notch::with_preview` the notch is built at rest and then **pinned**:

- the requested mode, page, activity, song, device, prompt or live activity is set up with sample data;
- activities and music are given an expiry a year away, so nothing times out;
- every spring is snapped to its target, so the state appears fully formed with no animation in;
- `tick` stops following the pointer and `next_wakeup` returns nothing, so hovering or moving away does not change the state, and the app does not keep waking up for it;
- calls that would change the state (`collapse`, `show_ask`, `set_now_playing`, `set_live`, `show_volume`, `show_brightness`, `show_device`, and so on) are ignored.

The sample tool notices are built with the real SDK notice builders (the same `Notice`, `adaptive`, `swatch`, `badge`, `timer` elements tools use), so a preview shows exactly what the element renderer paints for a tool.

The app treats the process as "previewing" whenever the variable is set, and then:

- never installs or removes assistant hooks,
- does not intercept the volume, brightness and media keys,
- ignores Bluetooth changes,
- never offers the [What's New](Updates.md#whats-new) notes by itself and never marks them seen, so `updates.seen` in the `config.json` it shares is left alone,
- never writes the [usage history](Stats.md#history-statshistory), and for `stats` and `stats-history` shows the sample week instead of the real one (`Notch::previewed_history`, pinned with `Stats::pin_history`),
- uses the previewed song (if any) as now playing instead of the real player, and none at all for `media-quiet`.

This makes previews safe to run next to your real notchbase.

## Notch states

| Name | Shows |
| --- | --- |
| `idle` | the notch at rest (on an island, the tucked tab) |
| `attracted` | the island fully drawn out into its capsule, as if the cursor were near |
| `peek` | the hover peek |
| `expanded` (also `tools`) | the open panel on the start page (Tools by default): the icons alone, with the card put away |
| `media` | the open panel on the Media page with a sample song and the folded volume row under it: the Output slider and the chevron |
| `media-quiet` (or `nothing-playing`) | the open panel on the Media page with no song: the one-row "Nothing Playing" and the folded volume row |
| `media-mixer` (or `media-expanded`) | the Media page with a sample song and the chevron open: the volume row, then the Output picker, Input and Apps |
| `sound` | the open panel on the Sound page: the mixer alone, even without a Sound tab |
| `stats` | the open panel on the Stats page, with a sample week under the usage card that never hit the limit (the gauges and the card itself show your real numbers) |
| `stats-apps` (or `apps`) | the Stats page with the CPU gauge chosen and a sample list of six apps under the gauges (monograms instead of icons, Finder without an x) |
| `stats-disk` (or `storage`) | the Stats page with the Disk gauge chosen and a sample storage view: seven parts with a few items each on a 494 GB disk |
| `stats-history` (or `usage-history`) | the Stats page with a heavier sample week: two days at 100%, one day without samples, "Hit the limit 2 days" |
| `shelf` | the open panel on the Shelf page |
| `settings` | the open panel on the Settings page |
| `notifications` (or `update`) | the open panel on the Notifications page with one sample unread update, "notchbase 0.5.0 is available" with **Update Now**, and the blue dot on the gear |
| `assistant-settings` (or `assistant`) | the open panel on the Settings page's Assistant view: the Claude / ChatGPT / Off choice and the two switches |
| `catalog` (or `installed-tools`) | the open panel on the Settings page's Tools view: folders, the installed tools and their switches |
| `playing` (or `music`) | a sample song ("Neon Tides" by Harbor Lights) revealed at rest, dancing |
| `docked` | the sample song hugging the top edge |
| `loud` (or `music-volume`) | the sample song with the volume strip under it |
| `volume` | the volume indicator |
| `brightness` | the brightness indicator |
| `bluetooth` (or `device`) | "AirPods Pro" connected, 82% battery |
| `bluetooth-off` (or `device-gone`) | the same device disconnected |
| `message` | a message activity ("Allow Screen Recording for notchbase in System Settings") |
| `copied` | the Pick Color notice: a popping swatch and `#3A7BFF` |
| `shot` | the Screenshot notice: on an island a thumbnail, "Copied", `2560×1600` and the **Annotate** button tucked into the end of the capsule; on a notch the thumbnail and a green check in the left wing and **Annotate** in the right (the app also puts a sample thumbnail in the image store). Like everything in a pinned preview, the button takes no presses |
| `recorded` | the Record Screen "Saved 0:42" notice with a green checkmark badge |
| `alert` | an Alerts banner: an orange bell badge beside "Deploy finished" with a line under it, shaking |
| `recording` | the Record Screen live activity: a red dot and a timer at 0:42 and counting; the app also reports a recording in progress to tools, so the tile shows it too |
| `drop` | the drop well with three files |
| `permission` | an assistant permission prompt ("Run this command?" with a sample command, and **Always allow**) |
| `question` | an assistant question with four options, each with a description |
| `choices` (or `multi-select`) | two questions asked together, the first a multi-choice one ("1 of 2") |
| `working` (or `thinking`, `agent`) | the assistant thinking: the Claude spark turning slowly and 3:12 counting |
| `doing` (or `agent-doing`) | the assistant at work: a faster spark and "Editing" |
| `needs-you` (or `agent-waiting`) | the assistant waiting on you: a yellow spark and "Needs you" |
| `sides` (or `activities`) | three activities: Claude "Editing" in the middle, a recording dot as the right ear and a timer ring as the left |
| `music-sides` | a song leading in the middle with Claude and a recording as ears |
| `agent-done` (or `done`) | the "notchbase done" notice with a green checkmark |
| `agents` | the open panel on the Agents page: Claude "Editing" in notchbase and "Thinking" in website, each with an Open pill, then Recent with rocket ("Shipped the fix, tests green. The crash came from…", 2:31, 4 minutes ago) and docs. Rocket's message runs past six lines, so hovering its row opens it to six lines ending in "…". The runs' window is a process id that can't exist, so Open does nothing |
| `agents-expanded` | the `agents` panel with rocket's row held open, as if hovered: six caption lines of its message under the title, the sixth ending in "…", and docs pushed down |
| `agents-waiting` | the Agents page with one run waiting on "Run this command?": the yellow dot on the Agents tab, and the Open pill left of the row's chevron |
| `agents-many` (or `agents-overflow`) | the Agents page with three running sessions and eight recent turns, taller than the panel: it scrolls, the bottom edge fades into the panel and the scrollbar shows in the right-hand padding |
| `agents-empty` | the Agents page with nothing running and nothing recent |
| `shelf-convert` (or `convert`) | the Shelf page with the Convert row open: the formats for the files on the shelf, each an icon over its name, and the x. The shelf needs files of one kind, so set `NOTCHBASE_SHELF_FILES` as for `shelf` |
| `shelf-airdrop` (or `airdrop`) | the Shelf page with the first three items selected, folders included. On a Mac the row ends with the AirDrop button; Windows has no AirDrop, so there is none (with files on the shelf, as above) |
| `video-drop` | the notch offering to take a YouTube tab: **Video** (aimed at) and **Audio**, side by side |
| `layouts` (or `snap`) | the window layout picker, the left half marked as the app's usual layout: side-by-side layouts on top, stacked ones under them |
| `layouts-stacked` | the layout picker with **Middle third** aimed at |
| `video-layouts` | the layout picker with its two download cells, **Download video** aimed at |
| `video-finding` | a [video download](Video-Downloads.md) looking its page up: a light chasing its tail and "Finding" |
| `video` (or `video-downloading`) | a download 62% done: the ring, "62%" and the light under the notch |
| `video-saved` | the download landed: a jade tick and "Saved" |
| `video-sides` | two downloads and a recording: an Instagram download leads, the YouTube one and the recording are its ears |
| `video-panel` | the open panel on the Tools page with a download's row at the bottom: cover, title, progress and × |
| `find` (or `find-suggestions`) | the Find page with an empty field and four suggestions, a prediction on top ("Usually after Cargo.toml · src"); the top row is selected, so its reveal and shelf buttons show instead of its date |
| `find-results` | Find answering "notch": an app on top, titled by its name alone with "App" where a file shows its folder; below it titles in two tones (the match in pearl), home-relative places, relative dates, a long name cut in the middle around its match |
| `find-actions` (or `find-selected`) | the same answer with the second row selected and its two round buttons out; the app row above it is at rest and shows no date |
| `find-none` (or `find-nothing`) | a query with no answer: "No matches" and the caption that Desktop, Documents and Downloads are not indexed yet |
| `find-indexing` (or `find-starting`) | the compact page while the first index is built: "Looking through your files", the count, and the ring in the field |
| `find-access` | the suggestions under the access row: a lock, "See every file", **Allow** and the dismiss disc |
| `find-off` | the compact page with Find switched off: a dimmed field, "Find is off" and **Turn On** |
| `whats-new` (or `changes`) | the open panel on the [What's New](Pages.md#whats-new-pageswhats_new) page with the running version's real notes: the header with **Done** and the changes as a list of bullets, under the release's picture for this notch's style when it carries one. No tab is lit. Done and the tabs still work, as on any pinned panel, and nothing is marked seen |
| `welcome` (or `walkthrough`) | the open panel on the first-launch [Welcome](Pages.md#welcome-pageswelcome) page: the hello with **Get Started** and **Skip** |
| `welcome-ready` | the hello with nothing to ask (as on Windows, or a Mac with every permission on): **Continue** alone |
| `welcome-step` (or `permission-step`), `welcome-asked`, `welcome-allowed` | the Accessibility step not yet asked (**Allow**), asked (**Open Settings** and its hint) and on (a green check and **Continue**), with a dot for each of the eight permissions |
| `welcome-done` | the last card, **You're all set** |
| `updated` | the "Updated" notice a copy shows at start while its notes are unseen, built by the real builder with the running version: the spark badge, "Updated to notchbase 0.14.0" and **View Changes** tucked into the end of the capsule on an island; the badge and "Updated" in the left wing and **Changes** in the right on a notch. The button takes no presses. Mind the d: `update` is the Notifications preview |

## App states

These set something up in the app as well as (or instead of) the notch:

| Name | Shows |
| --- | --- |
| `shelf` | as above, and if `NOTCHBASE_SHELF_FILES` is set, those files are put on the shelf |
| `picker` | starts the color sampler immediately |
| `screenshot` | starts the region picker immediately (on Windows, with `NOTCHBASE_PREVIEW_SELECTION=x0,y0,x1,y1` a selection is already drawn) |
| `annotate` | opens the [annotator](Annotate.md) on a sample screenshot (a 1600 x 1000 page of lines on a sky gradient, `annotate::sample`), as if Annotate had been pressed. The notch itself is not pinned. The window is the real one: drawing and typing write to the real clipboard, and a brush change is saved to the `config.json` in use, so give it its own `NOTCHBASE_CONFIG` to keep your brush |
| `recording-tools` | the open panel on the start page (Tools by default) while a recording is in progress (started 42 seconds ago), so the Record Screen tile shows its recording state |
| `recording-area` | a recording in progress plus the recording dim around the middle quarter of the primary display |
| `shortcut`, `shortcut-taken` | the open panel (Tools by default) with the second tool shortcut being rebound, and the same with the "In use by …" problem |
| `calendar` | the open panel on the Calendar tab, held open by the Calendar tool. It shows your real calendar settings, so it's also the quickest way to look at your own month |
| `calendar-new` | the same with **+** pressed, showing the new-event form |
| `calendar-dates` | the new-event form with its date calendar open |

`NOTCHBASE_SHELF_FILES` is a list of paths separated like `PATH` (`;` on Windows, `:` on macOS):

```powershell
$env:NOTCHBASE_PREVIEW = "shelf"
$env:NOTCHBASE_SHELF_FILES = "C:\pics\a.png;C:\docs\b.pdf"
cargo run
```

The shelf in a preview is still the shelf in your data directory. Point `NOTCHBASE_DATA_DIR` at a scratch folder to keep preview files off your real shelf (see [Building and Testing](Building-and-Testing.md#running-a-development-copy-next-to-your-real-one)).

## Both styles

The notch style follows the display: a screen with a hardware notch shows the Notch style, anything else the Island style (see [Notch](Notch.md#two-styles)). To see both, run the preview on a MacBook's built-in display and on an external monitor, or turn on "All displays" so every screen shows a notch at once.

### A MacBook notch anywhere

Set `NOTCHBASE_NOTCH` to a size in points, such as `190x33` for the 13-inch MacBook Air, and the primary display is treated as if it had a camera notch that size, so the Notch style can be designed and checked on any computer:

```powershell
$env:NOTCHBASE_NOTCH = "190x33"; $env:NOTCHBASE_PREVIEW = "sides"; cargo run
```

## Screenshots of overlays on Windows

Capture overlays and the color loupe exclude themselves from screen capture, so a screenshot tool would not see them. Set `NOTCHBASE_OVERLAY_IN_CAPTURES=1` (pickers and the recording dim) or `NOTCHBASE_LOUPE_IN_CAPTURES=1` (the loupe) while taking screenshots of them.

## Looking at a state without a window

`notch/tests/snapshot.rs` has an ignored test that paints previews through the same headless frames as the other tests, rasterizes the triangles egui produced (two pixels to the point, over a plain gradient standing in for the desktop) and writes one PNG per state and style to `target/snapshots` (or `NOTCHBASE_SNAPSHOTS`). The rasterizer itself lives in `ui/snapshot/` (test builds only): `rasterize` and `Atlas` turn egui's output into a `Frame`, and `Camera` (`ui/snapshot/camera.rs`) wraps them for anything that is not a notch, running any `FnMut(&mut egui::Ui)` at a given size and scale. It is for looking at a change where the app can't be run or screenshotted. `NOTCHBASE_SNAPSHOT` is a comma-separated list of preview names; `shortcut` is also drawn with the second tool's shortcut being rebound, which is the Tools page with its card open. `NOTCHBASE_SHELF_FILES` puts files on the shelf as it does in the app, `NOTCHBASE_SNAPSHOT_POINTER=x,y` rests the pointer at a point of the canvas, to look at a hover, `NOTCHBASE_SNAPSHOT_TOOLS=n` keeps only the first `n` of the five tools, to look at the Tools row with three tools (`3`) or none (`0`), and `NOTCHBASE_SNAPSHOT_DESK=bright` swaps the desktop gradient for a pale one, to look at a state as it sits on a bright wallpaper, and `dark` for a near-black one (grey 19 to 14), where an island's [keyline](Notch.md#the-keyline-notchkeyliners) shows most. The desktop has no blur, but what shows through a glass pane is dimmed the way the macOS backdrop dims it (the same `erase::SMOKE`), so buttons come out as dark as they are on screen:

```bash
NOTCHBASE_SNAPSHOT=expanded,shortcut,media cargo test notch::tests::snapshot -- --ignored
```

The annotator has a snapshot of its own, since it is a window and not a notch state. `annotate/editor/snapshot.rs` drives the editor through a `Camera` and writes `annotate.png`, `annotate-drawing.png`, `annotate-small.png`, `annotate-plain.png` and `annotate-text.png` to the same folder (see [Annotate](Annotate.md#preview-and-snapshot) for what each shows). It takes no environment variables besides `NOTCHBASE_SNAPSHOTS`:

```bash
cargo test annotate::editor::snapshot -- --ignored
```

## Previews in tests

The same states are exercised in tests: `notch/frame.rs` paints every notch preview state headlessly in both styles, and `notch/preview/mod.rs` checks that pinned states do not animate, expire or react to the pointer and paints the Shelf previews in both styles.

The Find previews pin a sample `FindView` (`pages/find/samples.rs`) with `Pages::pin_find`, which hands it to `FindPage::pin`: the page shows the sample with a caret in the field, never arms, never takes the keyboard and asks the service for nothing. Sample rows carry no icon, so the pictures show the glyph wells.

The What's New previews need no sample: the notes are compiled in, and `updated` pins the notice `notch::notice::whats_new::updated` builds for the app, so the preview cannot drift from the real thing (`notch/preview/whats_new.rs` checks both).

`shelf-airdrop` stages its selection inside the Shelf page once the shelf has files (`ShelfPage::stage_airdrop`), without touching `config.json`. When you add a new notch state, add a preview for it and include it in that test, so it is both easy to look at and covered by the paint test.
