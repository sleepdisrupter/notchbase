# Pages

When the notch is expanded it shows a header with tabs and one **page** below it. Pages live in `src/pages`, one module per page. They are plain structs that keep their own animation state, measure their height, draw with the [canvas](Rendering-and-Motion.md#the-canvas), and return [actions](Architecture.md#data-flow). They never call services directly.

| Page | Module | Opened by |
| --- | --- | --- |
| Media | `pages/media` (the player from `pages/player`, then one volume row from `pages/mixer`: the Output slider and a chevron that unfolds the rest of the mixer) | the `media` tab |
| Tools | `pages/tools` | the `tools` tab |
| a tool's page | `pages/tool.rs` | the tool's tab, or the tool's `OpenPage` request |
| Shelf | `pages/shelf` | the `shelf` tab, or dropping files on the notch |
| Sound | `pages/mixer` | the `sound` tab, when `layout.tabs` lists it |
| Stats | `pages/stats` | the `stats` tab |
| Agents | `pages/agents` | the `agents` tab (only while an assistant is chosen), or clicking the assistant's live activity |
| Settings | `pages/settings` | the gear in the header |
| What's New | `pages/whats_new` | by itself, the next time the notch is opened in full while this version's notes are unseen; **View Changes** on the "Updated" notice; or the **What's New** row in Settings |
| Welcome | `pages/welcome` | by itself on a new install's first launch, and on every open until the walkthrough is finished; or the **Permissions** row in Settings (macOS) |

## How a page plugs in

Every page follows the same contract:

- **`height(...)`** returns the page's natural height for the current data. The panel asks every page each frame, and morphs to fit the visible page. Heights must be cheap and must not depend on the page being drawn first. A page taller than the panel allows is shown clipped and scrolls with the wheel (see [Notch](Notch.md#page-panel-notchpanelmodrs)).
- **`ui(&mut self, ctx: &mut PageCtx, data...) -> Vec<Action>`** draws the page and returns what the user asked for.
- **`is_animating()`** is true while the page has springs in motion, so the app keeps redrawing.

`PageCtx` (`src/pages/mod.rs`) carries:

| Field | Meaning |
| --- | --- |
| `ui` | the egui `Ui`, for hit testing (`ui.interact`) and input |
| `canvas` | where to draw; the panel presents it with the page's reveal, slide and opacity |
| `rect` | the page's content rect inside the panel (for a scrolled page, its full natural height, offset by the scroll) |
| `alpha` | how visible the page is (0..1). Pages treat themselves as **interactive only when `alpha >= 0.5`**, so a page that is fading out or sliding away cannot be clicked by accident |
| `dt` | the frame's time step, for springs |
| `now` | the frame's `Instant`, for timelines |

Data comes from `PanelData`, which the app builds each frame (see [Architecture](Architecture.md#data-flow)). The app only samples expensive sources for the page that is visible, once per `stats.refresh_ms` (1 second by default): the mixer for the Media, Sound and Tools pages (the Media page holds the mixer, and the recorder tile lists microphones), stats for the Stats page, and a check for vanished files on the Shelf page.

## Tabs and the header

The default tabs are **Media**, **Tools**, **Shelf**, **Agents**, **Stats** and **Calendar** (Agents only while an assistant is chosen in Settings); the sound mixer lives on the Media page, behind the chevron at the end of its volume row. Reorder them with `layout.tabs` in `config.json`, for example `["calendar", "media", "tools", "shelf", "stats"]` puts Calendar first; leave a built-in key out to hide it, or add `sound` for a tab with the mixer alone. Tools that ask for a header tab in their manifest, such as the bundled Calendar, go wherever they're listed, or after the listed tabs when they aren't. The tabs are the row of icons along the top of the open panel, which makes room without hiding anything when it gets crowded; see [Notch](Notch.md#the-page-icons-notchpanelheader). The notch opens on `notch.start_page` (Tools by default), on the Shelf page while the shelf holds anything, or else on the Media page when a song is known and `music.open_to_player` is on. Ahead of all of them, while this version's notes are unseen, it opens on [What's New](#whats-new-pageswhats_new). Dropping files on the notch opens the Shelf page. See [Configuration](Configuration.md) and [Notch](Notch.md#the-open-panel) for the header and pager.

## Media (`pages/media`)

All audio lives on the Media page: now playing on top, then, 10 points below it, a single 22 point row from the [mixer](#sound-pagesmixer): a round speaker well at the left (click it to mute), the Output slider with its live level meter, drawn as an 8 point track that thickens to 12 while hovered or dragged (`FOLD_TRACK` in `pages/mixer/layout.rs`), and a round **chevron** well of the same size at the right end. Clicking the chevron springs the rest of the mixer open beneath the row, in place: the Output device picker (the "Output" label and its popup), the Input row and the Apps section; the chevron turns to point up, and clicking again folds them away. The open state belongs to the page and is not saved. The page draws its own folded `MixerPage` (`MixerPage::folded`) in the rect under the player, and its height is the player's plus that gap plus the mixer's, which while folded is just the row and while opening grows by the spring's share of everything below it.

The player (`pages/player`) shows the current song from [Media and Music](Media-and-Music.md):

- 56 point artwork with rounded corners, the title (semibold) and the artist under it, centered on the artwork,
- a **scrubber**, shown when the player reports a timeline: a 4 point line (7 while hovered or dragged) with the elapsed time at its left end and the remaining time at its right, in whole seconds in the system font. The room for the times is set by the track's length, so the line never shifts as the seconds tick; dragging it sends `Action::Media(Control::Seek(seconds))`,
- **transport** buttons: previous, play/pause, next, bare glyphs with a faint round wash while hovered, each sending `Action::Media(...)`.

The system volume is the slider right below. Players are slow to confirm changes, so the player **holds** what you did (`pages/player/holds.rs`): after a seek, the scrubber shows the position you chose until the player's reported position comes within 1.5 seconds of it, and after play/pause the button shows the new state, each for up to 2.5 seconds. With no player it is a single 24 point row, a note and "Nothing Playing", above the volume row.

When the song changes, the title and artist hand over like the notch's (`ui::handover`): the old ones slide out the way you skipped (going back with the page's own previous button slides the other way) while the new ones slide in, and the artwork waits for the next song's cover and dissolves to it (`ui::cover`), instead of all of it swapping in one frame.

## Tools (`pages/tools`)

The Tools page is a **launcher**: one round icon per enabled tool that provides a tile, in the order of `layout.tools` in `config.json`, and nothing else until you linger on one: then a **card** with everything about that tool opens under them. The icons sit in a single row. Up to five share it; with more, the row scrolls sideways (a sideways swipe or the mouse wheel over it), and icons shrink and fade as they near an edge that hides more, so none is ever cut off. Dragging an icon to another place reorders the tools: the others glide aside, holding it at an edge scrolls the row on, and dropping it saves the order to `layout.tools` (`Action::MoveTool`) without restarting any tool. With no tiles the row reads "No tools yet".

**Layout** (`pages/tools/mod.rs`), top to bottom:

- **Running** (`pages/tools/running.rs`): every tool whose tile has a `tint` (recording, a timer counting down, Keep Awake on) gets a slim 34 point row, washed with its tint: its icon, its title and its footer (the recording time, the time left). Clicking the row sends the tile's `on_press`, so it stops a recording or opens the timer. A row that starts while the page is showing slides out from under the header on a spring and the icons move down with it, and a row that stops slides back up, instead of the icons jumping 42 points under the pointer; rows already running when the panel opens are simply there.
- **The icons** (`pages/tools/launcher.rs`): four to a row (exactly five share one row, so the fifth is never left alone on a second), each a 48 point well (glass, or a solid circle in the tile's `well` tone) with the tile's icon grown by half and its title as a caption underneath. A tool with a `tint` has a ring of that color round its well; the tool in the card has a faint white ring. The rings, the bright caption and a running tool's tint ease in and out (`TileState`'s tint spring, `controls::animate` for the focus) instead of jumping from icon to icon. Clicking an icon does what clicking its tile did, sending `on_press` or opening the tool's page; it doesn't open the card, but switches a card that is already showing to that tool. A disabled tile is dimmed and does nothing.
- **The well** and its glyph size are painted by `pages/tools/well.rs`.
- **The card**: the full tile (below) of one tool. It is put away when the panel opens, so the page is the icons alone. Lingering on an icon for 0.45 seconds opens it (`pages/tools/focus.rs`): the page grows by the card and its gap on a spring and the card slides out from under the icons as it fades in, its bottom edge riding the page's, so it never pokes out past the panel; a pointer that only stops to click never opens it. Once it is showing, resting on another icon for 0.14 seconds brings that tool in, so sweeping across the icons on the way to the card changes nothing. The card keeps its tool while the pointer is on it, while a dropdown it opened is showing, and while a shortcut is being rebound (rebinding always shows that shortcut's tool). It stays open until the panel closes. When it moves to another tool its glass stays put and only the contents hand over: the old tool's (`was`) are gone by half-way and the new one's fade in from a quarter of the way.

If the card opens a dropdown, the dropdown's list appears below it and the page grows by the list's height (see [dropdowns](Rendering-and-Motion.md#tool-elements-uielement)). A tile's `wide` flag no longer changes anything here.

**A tile** (`pages/tools/tile.rs`), as the card paints it, is a `TileNode` from the tool, painted on a flat card:

- an icon (or any symbol element) in a 28 point well at the top left, either glass or a solid circle in a tone (`well`),
- the title in the headline font, truncated to fit,
- an optional **accessory** element at the top right (for example the recorder's microphone and desktop-audio chips, or a running timer),
- at the bottom, the tool's **shortcut**, and an optional **footer** element to its right. At rest the shortcut is a quiet one-line caption (`⌃⇧C` on macOS, `Ctrl+Shift+C` on Windows, built by `platform::hotkey_caption`), and nothing when none is set. Hovering the tile crossfades it into the **shortcut field**: keycaps, or "Add Shortcut" when none is set (`pages/tools/shortcut.rs`). While rebinding, the recording prompt shows whether or not the tile is hovered,
- an optional **tint** that washes the glass with a tone (the recorder turns its tile red while recording).

Hovering lightens the card; pressing scales the card to 96% and, when released over it, sends the tile's `on_press` message to the tool. A disabled tile does not react. Accessory and footer elements are fully interactive element trees: their buttons, chips and dropdowns send their own messages. All of it becomes `Action::Tool(slot, Surface::Tile, handler, value)`, which the app delivers to the tool.

### Rebinding shortcuts

Clicking a shortcut field starts rebinding it (`Action::RecordShortcut`). The notch is pinned open and takes keyboard focus, every tool hotkey is temporarily unregistered, and the field turns into a pulsing blue prompt, "Type shortcut…". `KeyCapture` (`pages/tools/capture.rs`) then reads the keyboard:

| Input | Effect |
| --- | --- |
| a key with modifiers | saved as the new shortcut (`Action::SetShortcut`), written to `tools.<id>.shortcuts` in `config.json` |
| Escape | cancel |
| Backspace or Delete | clear the shortcut (when it has one) |
| a key that cannot be a global hotkey | the prompt shakes and says "That key can't be a shortcut" |
| a click outside the field, or the small x button | cancel, or clear |

The app checks the new combination before saving: if another tool uses it, the prompt turns orange with "In use by <title>"; if the system or another app has it, "In use by another app". The physical key is preferred over the layout's character, and the clipboard shortcuts (Copy, Cut, Paste), which egui reports as clipboard events, are turned back into their keys. Modifiers held on the keyboard are merged in from the OS, so Windows-key and Command combinations work.

## A tool's page (`pages/tool.rs`)

A tool can also have a whole page (a `Node` view), reached from its tab or its `OpenPage` request. `ToolPages` keeps a surface state (dropdown and text focus) per tool:

- If the page is a column, each child is a **block** laid out top to bottom with the column's spacing; otherwise the page is one block.
- A dropdown opened in a block shows its list right below that block and pushes the rest of the page down.
- Text inputs work here (and in tiles): clicking one focuses it (`Action::Typing(true)`), which pins the notch and takes keyboard focus; Enter submits, Escape cancels, and closing the panel releases it.
- If the tool has no page view, or it has stopped (panicked), the page shows "This tool stopped working".

Interactions become `Action::Tool(slot, Surface::Page, handler, value)`.

## Shelf (`pages/shelf`)

The shelf page is the UI for [the Shelf](Shelf.md):

- **Tiles**: a horizontally scrolling strip of tiles, five visible at a time, each with a thumbnail (or a kind icon while the thumbnail loads, with nothing behind it) and the file name. A selected tile has a soft grey backdrop behind it, as in Finder, and its name in full white; the others' names are dimmer, and hovering a tile lays a fainter one. New items pop in with a spring. The strip is laid out on `ui::rows` slots, so a tile that goes shrinks and fades where it was while the ones after it glide over, and clearing folds the tiles away.
  - click toggles selection,
  - double-click reveals the file in Finder or Explorer (`Action::ShelfReveal`),
  - dragging a tile drags the file (or the whole selection, if the tile is selected) out to another app (`Action::DragOut`),
  - the badge's remove button takes the item off the shelf (`Action::ShelfRemove`); its glass lights up gradually under the pointer.
- **Status line**: what is in scope and its total size ("All 4 files · 12 MB", or "2 of 4 selected · 3.1 MB"), with links to **Clear** and **Select All** when nothing is selected, or **Remove** and **Deselect** when something is. While an operation runs it shows only what is in scope, without links, and when one finishes it shows the result notice ("Compressed 3 files · 41% smaller") for `notices.shelf_ms` (4 s by default).
- **Action buttons**: the operations available for the current scope (everything, or the selection), computed by `shelf::available_ops`: Convert, Compress, Combine and Split for images and PDFs, Convert and Compress for videos, and Convert for audio. Choosing Convert swaps them for a row of **formats** (PNG, JPEG, WebP, TIFF, BMP, GIF, ICO or PDF for images; MP4, MOV, GIF or Audio (M4A) for videos; M4A, MP3, WAV or FLAC for audio, limited to what the platform can write). The formats are flat, with no glass behind them: each is its own icon (`ui::glyph`, `crates/paint/src/glyph/formats.rs`) over its name in the subheadline font, spread evenly across the row, with a flat **x** at the right end that goes back to the operations. Resting the pointer on one brightens it and lays a faint wash behind it; pressing shrinks it to 92%. The operations themselves are glass buttons, each its glyph over its name. Each sends `Action::ShelfRun(op, ids)`. With nothing actionable in scope and no AirDrop button, a one-line hint asks you to select files.
- **AirDrop** (Mac, `pages/shelf/buttons.rs`): the action row always ends with an **AirDrop** button (`Glyph::AirDrop`, a dot inside two rings open at the bottom) while the shelf has anything on it, folders included, even when no operation fits. It sends what the other buttons would work on: the selection, or every file when nothing is selected. Pressing it posts `Action::AirDrop(items)`, deselects and lifts the tiles off, and the app opens the system's AirDrop window. When the window closes the notch shows "AirDropped photo.png" (or "AirDropped 3 files"), or "Couldn't AirDrop photo.png" in orange, as the result notice; closing the window without sending says nothing. Windows has no AirDrop, so there is no send button at all (`shelf::airdrop::AVAILABLE`). The `shelf-airdrop` preview shows it, and `pages/shelf/airdrop_flow.rs` tests what it sends.
- **Lift-off** (`pages/shelf/liftoff.rs`): each sent tile rises and shrinks towards the top middle of the page on a slight arc (it rises first and drifts sideways as it goes), fading out over a 0.45 s critically damped spring. The files stay on the shelf: once the page is shown again (or after 1.2 s if the panel stays open) the tiles fade back in, in place, over a short settle. The overlay only transforms the tiles' shapes, so the page height never changes and a flying tile ignores the pointer.
- **While an operation runs** (`pages/shelf/busy.rs`) the action row becomes one row: a progress ring at the left that eases between steps, what is being made in the headline font ("Converting to MP3", "Compressing"), the file under it with the percentage ("Trials.mp4 · 40%", or "2 of 3 · 50%"), and **Cancel** as plain semibold text at the right (`Action::ShelfCancel`). Affected tiles show a moving sheen and a white progress trace around their edge.

The status line and button row crossfade when their content changes (`pages/shelf/crossfade.rs`): the old state keeps being drawn and is gone by half-way while the new one fades in from a quarter of the way, and a quick second change keeps the one that was really seen leaving. Only the incoming one takes presses. The status line's state carries its words, so a change of selection crossfades it too. The page's height is fixed per state (empty, or tiles plus status plus buttons) so the panel does not jump; the empty card fades in as the last tiles fold away.

## Sound (`pages/mixer`)

The mixer is the UI for the [Sound Mixer](Sound-Mixer.md). Folded, it is the lower part of the Media page (one row with the Output slider and a chevron, which opens the Output picker, Input and Apps below it); unfolded, it is the whole Sound page, which has no tab unless `layout.tabs` lists `sound`:

- **Output** and **Input** sections, each with a glass slider for the default device with a live level meter; clicking the glyph at the start of the slider toggles mute. Muting dims the fill, fades the meter and greys the glyph along the mute's own spring (`Slider::dimmed`), not in one frame. A popup button on the right shows the device's name; clicking it opens a **picker** listing all devices of that kind (up to five rows, scrollable with the wheel, the default one scrolled into view). Choosing one sends `Action::SetDefaultDevice`.
- **Apps**: up to four rows of apps that are playing or recently played audio, each with the app's icon and name and a slider; clicking the icon toggles mute. Sliders send `Action::SetVolume(id, volume)`, mute toggles send `Action::SetMuted`. Consecutive identical values are not resent.
- If per-app volume is unavailable, the apps section shows the reason instead (for example the macOS permission to allow, or the minimum macOS version). With no apps it says "Nothing is playing", which fades out as the first row comes in and back as the last one goes.

Rows are kept still while the pointer is over the mixer or a slider is being dragged: new apps wait and leaving apps hold their slot until they have faded, so nothing moves under your cursor. Changing the master volume from the slider quiets the volume indicator for `volume.quiet_after_mixer_ms` (1.2 s by default) so it does not pop up for a change you are already looking at.

## Stats (`pages/stats`)

- **Gauges**: four rings for CPU, GPU (with the GPU's name underneath), Memory (used of total) and Disk (free of total), animated with value springs.
- **Apps** (`pages/stats/apps.rs`, macOS only): clicking the CPU or Memory gauge puts it in a glass lens and unfolds a list under the gauges (`Action::ListApps`); clicking it again folds the list. The list is the five apps using the most of that resource, each a 28 point row with the app's icon, its name and its share of the whole machine's CPU or its memory, then an "Everything else" row for what the gauge counts beyond those five. Rows appear one by one as the list opens, and the usage card slides down beneath it. Hovering a row shows an x on the right: a click asks the app to quit (`Action::QuitApp`), the row reads "Quitting…" for three seconds, and if the app is still there the x stays, in red, and the next click force quits. Option-click force quits straight away. Apps that can't be quit from here (notchbase, Finder, Apple's menu bar agents) have no x. The order holds still while the pointer is over the list, so a row never moves under the cursor. Otherwise a new order glides: each row springs to its new place, an app new to the five fades in where it lands, and one that drops out fades where it was.
- **Storage** (`pages/stats/storage.rs`, macOS only): clicking the Disk gauge unfolds a view of what the disk holds, sorted by what things are rather than where they are. A header reads "Storage" with used of total (or "Measuring…" while the scan runs), then one bar split into the parts, then a row per part: a dot in the part's bar color, its name and its size. The parts go by size in fading shades of pearl (Documents, Apps, Virtual machines, App data, Developer, Photos and video, Music, Desktop, Downloads, Other files), eight at most, and **System and other** closes the list with whatever was not measured. Clicking a part slides in its seven largest items as a flat list, each an actual thing (an app with its icon, a large file wherever it is nested, one app's data, one project's build folder) and "Everything else" for the rest; the header becomes a back row and the bar eases the other parts down to 30%. Clicking an item shows it in Finder (`Action::RevealPath`). The list's height springs to the number of rows.
- **Usage card**: shown when an assistant is chosen in Settings. It shows the plan usage for Claude or ChatGPT (Codex) as two bars, **5-hour** and **Weekly**, with the time each resets. It shimmers while loading, explains what to do when signed out ("Sign in to Claude or Claude Code to see usage", "Use Codex on this computer to see ChatGPT usage"), and shows errors. Clicking the card refreshes it (`Action::RefreshStats`) with a spinning glyph. The bars are tinted in the assistant's color and warm to red from 85% to 95% used (`pages/stats/tone.rs`).
- **This week** (`pages/stats/week.rs`): under the usage card, a "This week" section header (the same `section` as the Agents page's "Recent") with "Never hit the limit" or "Hit the limit 2 days" on the right, and a strip of seven columns, six days ago through today, with each day's initial underneath. A column's height is that day's peak 5-hour usage, in the same tone as the card's bars; earlier days are a little dimmer than today, and today's initial is brighter. A day with no samples is a hairline on the baseline. The section is 64 points tall, only exists when an assistant is chosen, shows that assistant's days, and is left out until it has at least one day of [history](Stats.md#history-statshistory). The days arrive already computed in `Snapshot::history`, so the page only paints them.

The page's height is the gauges, the open part of the app list or the storage list, the card and the week exactly, so the panel fits it. See [Stats](Stats.md) for where the numbers come from.

## Agents (`pages/agents`)

Every Claude Code or Codex session notchbase hears through its [hooks](Assistant.md), on glass cards with 50 point rows like Settings:

- **Running**: one row per live turn, the newest first. The assistant's mark sits on the left and moves like the [live activity](Notch.md#the-live-activity) (turning while it thinks, spinning while it works), the project is the title, and underneath is what it's doing and for how long: "Editing · 3:12", "Thinking · 0:42".
- **Open**: when the hook found the window the run lives in (its terminal, IDE or the Claude app, see [Assistant](Assistant.md#finding-the-assistants-window-hookwindow)), the row has a small **Open** pill at the right, in the Settings pill style (`settings::pill`). Clicking it, while the page is at least half visible, sends `Action::FocusWindow(pid)`: the app brings that window to the front with `platform::focus_window` and, when that worked, collapses the notch the way a tool's `Collapse` request does. On Windows it takes the process's frontmost visible top-level window, restores it if minimized (`SW_RESTORE`) and calls `SetForegroundWindow`, falling back to `AttachThreadInput` with the foreground thread and `BringWindowToTop` when Windows refuses; on macOS it hands activation over with `yieldActivationToApplication` and `activateFromApplication` while notchbase is active, and calls `activateWithOptions` otherwise. On macOS the app also forgets the app it would hand focus back to when the pointer leaves the notch, so the window you opened keeps it.
- **Needs you**: the mark stops and turns yellow, and the line underneath is the prompt's title in yellow ("Run this command?", the question, or "Needs you" when nothing more is known). The row has a chevron and, when clicked anywhere but its Open pill, opens that prompt in the notch's Ask mode (`Action::ShowPrompt`), even with "Questions in the notch" off; the Open pill sits left of the chevron and still opens the window. The app holds on to that prompt until it's answered or gone. Choosing "Answer in Claude" there also brings the run's window to the front (see [Assistant](Assistant.md#from-the-board-to-the-notch)). While any run waits, the Agents tab in the header carries a yellow dot (see [Notch](Notch.md#the-page-icons-notchpanelheader)).
- **Recent**: below a "Recent" header, up to eight finished turns from the last three hours, the newest first: a green checkmark badge, the project, how long it took and how long ago it ended on the right ("2:31 · 4m ago"), and the first line of the assistant's last message underneath.
- **Hover to read more**: while the pointer is over a Recent row (and the page is at least half visible), the row grows with a `SMOOTH` spring to show the assistant's last message wrapped at the row's text width, in the same caption font and inset, capped at six lines; when the message runs past the sixth line, that line ends in "…". The title, badge and "2:31 · 4m ago" stay where they are, the first wrapped line takes the place of the one-line summary (the two crossfade), each further line fades in as the row makes room for it, and the rows below move down. Leaving the row springs it back. Only the hovered row is open, and a message that fits on one line never grows. The wrapped message drops blank lines, and its lines are measured once per finished turn and row width (`agents/wrap.rs`), not every frame. The springs live in `agents/unfold.rs`; when the page is not interactive (fading, sliding away or hidden) they snap shut. Running rows don't grow: a waiting row already opens its whole prompt in Ask mode on click.
- **Nothing yet**: one row, "Nothing running", with "Claude Code shows up here while it works" (or Codex) under it.
- **Coming and going**: while the page is on screen, a run that starts grows its row in where it belongs and a finished one folds away where it was (`ui::rows`), its words fading in once there is room, and the Recent header comes in with its first row; the page's height follows the rows, so the panel grows and shrinks with them. Rows already there when the page opens are simply there, and the page emptying out shows "Nothing running" at once.

The page's height is its rows exactly, including each Recent row's current extra height, so the panel morphs with an opening row, and it keeps redrawing while a turn is running or a row is still moving so the marks move and the clocks count. With only Recent rows it doesn't animate: while the page shows, the app (`app/assistant/agents.rs`) wakes once when the soonest "ago" label is due to change (the next whole minute, or hour after the first hour) and redraws, so "4m ago" keeps up. `app/assistant/view.rs` rebuilds the `AgentsView` whenever the board changes. With more rows than the panel can hold, the page scrolls and its clipped edges fade into the panel (see [Notch](Notch.md#page-panel-notchpanelmodrs)). The `agents`, `agents-expanded`, `agents-waiting`, `agents-many` and `agents-empty` previews show it (see [Previews](Previews.md)), and tests in `notch/preview/agents.rs` click the previews' rows: the Open pill gives `FocusWindow`, a waiting row gives `ShowPrompt`, hovering rocket's row grows the page by two caption lines and leaving folds it, and `agents-expanded` holds it open.

## Notifications (`pages/notifications.rs`)

The **Notifications** row at the top of Settings opens this page, and its **‹ Settings** back button returns there. Each notification is a glass card with a colored badge, a title, a detail line, an optional thin progress bar and an optional blue button whose action goes back to the app. Today the updater is the only source: an available, downloading, ready or failed update, with **Update Now**, **Restart Now** or **Try Again** (`Action::UpdateNow`). With nothing to show, the page says "No notifications". Cards are laid out on `ui::rows`, so one that arrives grows in and one that goes folds away, and "No notifications" fades in as the last one goes; an update's progress bar eases to each reading, and a card's button fades in and out over 0.2 s. The app remembers which notifications you have opened the page on (`app/notifications.rs`); anything newer puts a blue dot on the gear.

## Welcome (`pages/welcome`)

The first-launch walkthrough (see [Platform: macOS](Platform-macOS.md#the-first-launch-walkthrough) for what it asks and the rules behind it). One centred card at a time: a 44 point glass well with a glyph (a green check once a permission is on), a semibold 17 title, a line in the dim callout balanced over at most two rows, an optional dim hint, a pearl primary capsule beside a glass one, and on permission steps a row of dots whose current one is a wide pill. The card keeps one height from the hello to the end so the panel does not jump. A new stage fades up from 8 points below over 0.3 s and takes no clicks until it is mostly in, so a double click never answers two steps. When a permission comes through while its step shows, the step eases over 0.3 s instead of swapping: the glyph shrinks to 60% and fades as the green check grows in from 60%, the hint and **Not Now** fade, and the primary capsule's label cross-fades to **Continue** as it glides to the middle (`buttons_between`).

The page draws whatever `PanelData.welcome` (`WelcomeView`) says: the stage (`Hello`, `Step(index)`, `AllSet`) and the steps, with each permission's standing and whether it was asked this time. It decides only the labels (`copy.rs`: **Allow** until macOS would only show System Settings, then **Open Settings**; **Continue** once on) and sends `WelcomeStart`, `WelcomeAsk(access)`, `WelcomeNext`, `WelcomeSkip` or `WelcomeDone`. The app (`app/welcome.rs`) owns the tour: it builds the steps, records each one put to you in `permissions.asked`, looks at the standings twice a second while the page is up and moves on 0.9 s after the current one turns on. **Done** asks the panel to leave (`WelcomePage::take_done`, read by `Notch::leave_welcome_when_done`): back to the tabs after a first run, back to Settings after **Settings → Permissions**. The page holds the panel open, so it stays up while you are in System Settings.

## What's New (`pages/whats_new`)

The page for a release's notes (see [Updates](Updates.md#whats-new) for when it shows and when the notes count as seen). It has no tab and no icon: while it shows, no tab is lit, and **Done**, any tab, the gear or the magnifier leaves it (each of them marks the notes seen). It is a plain list to take in at a glance: the panel is its only surface, with no cards, no wells and no dividers, two levels of text, pearl on black, and one picture at most.

`Pages` hands it the latest release (`whats_new::latest()`) and whether this notch sits beside a camera, read from the `Env` it is measured and painted with. It takes nothing from `PanelData`, and what it shows never depends on whether the notes are seen, so marking them seen changes nothing on screen.

- **The header** is one line on a shared baseline: "What's New" in semibold 17, "notchbase 0.14.0" beside it in the dim subheadline (the version the notes belong to), and **Done** at the right as plain semibold text that brightens under the pointer. It is in the header so it is in reach without scrolling. Pressing it sends `Action::ChangesSeen` and asks the panel to leave (`WhatsNewPage::take_done`, read by `Notch::leave_changes_when_done`), which goes back to the tab the page came from or the one the lens rests under.
- **One picture** for the whole release, when it carries one, 18 points under the header: the `Shot`'s picture for this notch's style, with continuous corners and a pearl hairline (`ui/picture.rs`, see [Rendering and Motion](Rendering-and-Motion.md#pictures-uipicturers)). 0.14.0 carries none, so its page is the header and the list (see [Updates](Updates.md#giving-a-release-a-picture) for when a picture earns its place).
- **A bullet per change**, 20 points under the picture (or under the header when there is none) and 18 apart. The bullet is the change's glyph, painted bare at 15 points in the title's pearl and centred on the title's line. Past it, on one left edge 27 points in, come the title in the headline font and, a point under it, the caption in the dim subheadline. A caption is written to fit one line; a longer one wraps to a second, split into two lines of even length so the last word is never left alone, and ends in "…" past that. Line heights are measured, not assumed.
- **One column.** The header, the bullets and a picture share one left edge and one right edge: a column as wide as a picture is (`whats_new::PICTURE_WIDTH`, 340 points), which is the whole page on an island and on Windows and is centred in the slightly wider page beside a camera.
- **With no notes** for this version the page is the header and one dim line, "Nothing new in this version".

One function, `layout::plan`, lays the page out, and both `height` and `ui` call it, so the height the panel morphs to is always the height that is painted. For a release with a picture, `height` only reads the picture's size, which was taken from the PNG's header when the app was compiled; nothing is decoded until the picture is first painted. The page's `Album` keeps the texture and `rest` frees it when the panel hides, where the panel also forgets the page's scroll offset. Notes taller than the panel scroll and fade like any page (see [Notch](Notch.md#page-panel-notchpanelmodrs)); 0.14.0's are 163 points tall in both styles, a third of what the panel could show, and a test holds both to not scrolling.

The `whats-new` preview shows it (see [Previews](Previews.md)). Tests in `pages/whats_new/tests.rs` hold the height to what is laid out, grow it by the picture and by a longer caption, check the shared edges, the bare glyph on its title's line and the air between bullets, hold this release's captions to one line, click Done at full and half visibility and find it deaf while the page fades (`tests/captions.rs` covers the even split); `notch/tests/whats_new_page/mod.rs` covers it inside the panel, and `notch/tests/whats_new_page/leaving.rs` what leaving it tells the app.

## Settings (`pages/settings`)

Settings has a general view and three views it opens, switched inside the page (`pages/settings/mod.rs`): **Assistant**, **Tools** and a tool's settings. It always reopens on the general view. A view that opens slides in 12 points from the right as it fades in, and going back slides in from the left (`Slide`, on a `SMOOTH` spring), the way pages switch; each view keeps its own scroll position (`Pages::keep_settings_scroll` swaps the page's offset as the view changes), so a sub-view opens at its top and the general view comes back where it was left. Rows are 50 points tall and painted on flat cards with hairline dividers (`pages/settings/rows.rs`).

### General (`pages/settings/general.rs`)

The notch card:

| Row | Control | Effect |
| --- | --- | --- |
| **Volume indicator** | switch | replaces the system volume and media popups (`volume.indicator`). See [Volume and Media Keys](Volume-and-Media-Keys.md) |
| **Brightness indicator** | switch | macOS only: replaces the system brightness popup (`brightness.indicator`). See [Volume and Media Keys](Volume-and-Media-Keys.md#brightness-keys) |
| **All displays** | switch | shows an island on every screen (`displays.all`) |
| **Wake at the top edge** | switch | the island only wakes once the pointer hits the top of the screen, so browser tabs under it stay clickable; a Mac's real notch only opens with the pointer on the notch itself (`notch.wake_at_edge`). See [Notch](Notch.md) |
| **Move away for keys and clicks** | switch | the notch slides out from under the pointer while any key is held or a mouse press starts beside it; off, it stays put for both (`notch.flee_from_keys`). See [Notch](Notch.md) |

Then the app card. See [Installing](Installing.md) and [Updates](Updates.md):

| Row | Control | Effect |
| --- | --- | --- |
| **Install notchbase** | row (only while running from somewhere else) | installs this copy as an app and relaunches it from there |
| **Open at login** | switch | starts notchbase when you sign in |
| **Automatic updates** | switch | installs new releases from GitHub when the notch is idle; the subtitle shows the version, download progress or the last problem (`updates.automatic`) |

Then a card of rows that lead elsewhere:

- **Notifications**, whose subtitle sums up what's waiting (the one notification's title, such as "notchbase 0.5.0 is available", or "2 unread", or "Nothing new"). It opens the [Notifications](#notifications-pagesnotificationsrs) page,
- **Assistant**, whose subtitle says what it's set to ("Claude · questions and progress", "ChatGPT · questions", "Claude · usage only", "Off"),
- **Tools** ("7 installed · their settings and folders"). Tool settings live there, one page per tool, so the main Settings page stays short,
- **Indicators** ("Battery, camera, downloads and other notices"), the same list for the background tools,
- **What's New** ("See what changed in 0.14.0"), there whenever the running version has notes (`pages/settings/general/news.rs`). It doesn't open a Settings view: it sends `Action::ViewChanges`, and the app opens the [What's New](#whats-new-pageswhats_new) page on this notch.

Last, **Quit notchbase**, a plain centered text button that closes notchbase until you open it again (`Action::Quit`).

### Assistant (`pages/settings/assistant.rs`)

A **‹ Settings** back button and one card:

| Row | Control | Effect |
| --- | --- | --- |
| **Assistant** | Claude / ChatGPT / Off | whose usage the Stats page shows and whose questions appear in the notch (`assistant.provider`, `assistant.enabled`) |
| **Questions in the notch** | switch (only with an assistant) | installs the assistant's hooks and shows its prompts and questions in the notch (`assistant.questions_in_notch`). See [Assistant](Assistant.md) |
| **Progress in the notch** | switch (only with an assistant) | shows a live activity while the assistant works and a notice when it's done (`assistant.progress_in_notch`). See [Assistant](Assistant.md#progress) |

The `assistant-settings` preview opens this view directly (see [Previews](Previews.md)).

### Dock (`pages/settings/dock.rs`)

macOS only, from the **Dock** row of General. It shows [notchbase dock](../dock/README.md)'s settings. A **‹ Settings** back button, then a card with the **notchbase dock** switch and a **Permissions** row. While the dock is off, the switch row sums it up ("Liquid animations, live previews and its own Cmd+Tab") and the Permissions row says macOS will ask for Accessibility and Screen Recording next, so the reason comes before the system dialogs. The switch starts or quits the dock and sets `dock.enabled` (see [Configuration](Configuration.md#dock)). Four groups follow, each written straight to the dock's `dock.json`, which the running dock takes within half a second:

| Group | Rows |
| --- | --- |
| Appearance | Position (Left, Bottom or Right: the edge of the screen it stands on), Icon size (16 to 96 pt), Magnification, Glass (Light, Smoked, Dark), App names |
| Behaviour | Hide in full screen, Keep windows clear, Window previews, Mission Control (Stay or Fade) |
| Animations | Open effect, Minimize effect (Liquid or macOS), Minimize time (0.25 to 0.8 s, Liquid only) |
| Cmd+Tab | Dock's own switcher |

The rows stay greyed out until the dock has run once and written its file. The one-time **Meet notchbase dock** notice (`notch/notice/dock_intro.rs`, preview `dock-intro`) leads here from **Turn On**. The `dock-settings` preview opens this view directly.

### A tool's settings (`pages/settings/tool.rs`)

Opened from the cog beside a tool in the Tools view, or straight from a tool with `Command::open_settings` (the Calendar's **Open Settings** does this). It has a **‹ Tools** back button, the tool's name, and a row for each setting its manifest declares as visible. Like the rest of Settings, the notch stays open while it's showing, so a long address can be pasted from elsewhere:

| Setting kind | Control |
| --- | --- |
| toggle | a switch |
| choice with up to four options | a segmented control |
| number | − and + steppers around the value (and its unit), clamped to the setting's range and step |
| folder | the chosen path (or the placeholder, dimmed), a **Choose…** pill that opens the system folder picker (`Action::PickToolFolder`), and a × that clears it back to the tool's default |
| text | a text field (`src/ui/field/`, shared with tool pages). It saves, trimmed, on Enter, a click elsewhere or the notch closing, and only if the text changed (`pages/settings/editing.rs`); Esc drops the edit. A × beside a filled field clears it. A secret one shows dots, even while typing |

A setting with `shown_when` only appears while that toggle is on, and one with `shown_when_is` only while that choice has that value. The rows are laid out on `ui::rows` (`tool::Shown`), so such a row grows in under the setting that wakes it and folds away when that changes back, the card's height following, and it takes presses once fully shown; opening the view shows the rows as they are. Data settings, choices with more than four options, and hidden settings are not shown (they are still in `config.json`). A change sends `Action::SetToolSetting`: the value is written to `tools.<id>.settings` and every tool gets its settings again. See [tools/Settings-and-Shortcuts.md](tools/Settings-and-Shortcuts.md).

### Tools (`pages/settings/catalog.rs`)

- a **‹ Settings** back button,
- **Tools folder** and **config.json** rows showing their full paths (truncated from the start, so the end stays visible) with an **Open** button each (`Action::OpenPath`; a missing tools folder is created first),
- a hint: "One folder per tool in the tools folder, with its .dll inside" (`.dylib` on macOS),
- in the same card as the two folder rows, a **Reload tools** row with a **Reload** button (`Action::ReloadTools`, which rescans the tools folder and restarts every tool),
- **Installed (N)**, one row per tool in the catalog that is not a background tool (those are under **Settings › Indicators**, a list of the same rows for tools whose manifest says `.background()`: Alerts, Battery, Camera & Microphone, Downloads and Focus): its name and version, and underneath either "Built in", the plugin's library path, or, in orange, the problem that stopped it loading or running. Each loadable tool has a switch that turns it on or off (`Action::SetToolEnabled`): this writes `tools.<id>.enabled` to `config.json` and reloads the tools. A turned-on tool with settings has a round cog beside its switch that opens its settings. The cog fades in and out with the switch (0.2 s), and the detail line's room follows it.

The `catalog` preview opens this view directly (see [Previews](Previews.md)).

## Writing a new page

1. Add a module under `src/pages` with a struct that implements `height`, `ui` and `is_animating`.
2. Keep all state that should animate in springs, and step them with `ctx.dt`.
3. Only react to input when `ctx.alpha >= 0.5`.
4. Return actions for anything that changes the world. If a new kind of change is needed, add an `Action` variant and handle it in the app. For most new features, prefer writing a tool instead, which gets its own tile, settings, shortcuts and optional page and tab without touching the app; see [tools/Getting-Started.md](tools/Getting-Started.md).
5. Add tests next to the page for its layout math and any state logic (see the tests in `pages/mixer/slots.rs`, `pages/player/holds.rs` and `pages/tools/capture.rs` for examples).
