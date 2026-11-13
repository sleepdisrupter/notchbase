# Find

Find is the file locator in the open panel: a magnifier button mirrored against the settings gear, a page with one text field, and instant results for every file, folder and app notchbase can reach on local volumes. It indexes everything once, keeps the index live from the system's change notifications, searches off the main thread in a few milliseconds, and learns on the device which files you pick for what you type.

## Using it

- **The button.** On the Notch style it sits in the left wing beside the camera; on the Island style in the left corner of the tab row. Pressing it opens the Find page with the caret in the field; pressing it again returns to the tab you came from. Only the button opens Find: re-expanding the panel, a hover or a pin never lands on it and never takes the keyboard.
- **Typing.** Every key asks at once (no debounce). Several words match in any order across the name and its folders; extensions (`.rs`), path fragments (`src/app`), kind words (`pdf`, `folders`, `images`, `apps`) and date words (`today`, `yesterday`) work alongside plain text. An empty field lists predictions, recents, a hot folder and frequent files, and a field that holds only spaces is the empty field.
- **Apps.** An app answers to the name the system shows for it (**Find My**, not `FindMy`), and comes before files: `treesize free`, `TreeSize  Free`, `tree size free` and `treesizefree` all put TreeSize Free first. Each word of an app counts as its start (`chrome` finds Google Chrome), and so do its initials from two letters on (`vsc` finds Visual Studio Code). A slip of the finger still finds it (`sptoify`, `chrm`). Among apps that start alike, the ones you use come first. `apps` or `applications` lists them. [What counts as an app](#apps) differs by system.
- **The system's search key.** With **Replace Spotlight** (macOS) or **Replace the Start menu** (Windows) on, ⌘Space or a lone Windows key opens Find with the keyboard, and closes it again. See [the key](#the-systems-search-key).
- **Spaces.** Words typed apart always find a name that runs them together (`my file` finds `myfile.txt`). The other way round, `myfile` or `my-file` finds `My File.txt`, `my_file.txt` and `my-file.txt` when the text as typed finds fewer than eight results; an app is found that way always. Space, `-` and `_` are the separators that are skipped; a dot is a letter.
- **Keys.** Up and Down move the selection; Enter opens the selected row; Cmd+Enter (Ctrl+Enter on Windows) reveals it in Finder or Explorer; on Windows, Ctrl+Shift+Enter runs the selected app as administrator, behind the UAC prompt, as the Start menu does (on a file it just opens it); Escape clears the field and, on an empty field, closes the panel and gives the keyboard back to the app you were in. Enter never opens a row that was not shown for the text you typed: if you type faster than the answer, Enter waits for it. A space that changes no word (one after the last word, a second one between two) asks again and keeps the selected row.
- **Calm by default.** Only your own files, folders and apps are listed. Hidden files stay out of sight until nothing of yours matches or you type the name with its dot (`.zshrc`). What is not yours (Library, build and cache files, the contents of hidden folders and of packages, what apps keep beside themselves or deep in `/Users/Shared`, machine-made names) is listed only for a path or once you picked it; `find.hidden` lists everything again. Rows named by what you typed come before rows that are merely of that kind, no more than three rows of one name stand together, and a loose guess is made only when it reads as the name.
- **System Settings.** On macOS every pane of System Settings is listed like an app, under the name the system shows (`wifi` finds **Wi-Fi**, `blue` **Bluetooth**, `dock` **Desktop & Dock**), and Enter opens that pane.
- **Where the pointer is.** With notchbase on every display, the system's search key opens Find on the display the pointer is on. One row stands out: the selected row is full size on a glass band with its folder, date and buttons, the others are smaller, dimmer and show only their names.
- **Rows.** Click opens, drag drags the file out, and the two buttons on a hovered or selected row reveal it or add it to the Shelf. An app row shows the app's name without its ending, its own icon, "App" where a file shows its folder, and no date; Enter or a click launches it, and reveal, drag and the Shelf act on the shortcut or the bundle like on any file. On Windows an app row has a third button, a shield, that runs it as administrator, and its right-click menu has **Run as administrator** too. Declining the UAC prompt does nothing more.
- **Settings.** Three rows in Settings › General: **Find files** (the on/off switch, with the item count), **Replace Spotlight** / **Replace the Start menu** (the system's search key, off by default) and **What Find learned · Clear**. Everything else is in [`config.json`](Configuration.md#find).

## How it works

### Threads

| Thread | Does |
| --- | --- |
| `notchbase-find` (the writer) | The only thread that changes the index or the usage model: applies directory listings, turns change notifications into relistings, runs the launch sequence, rebuilds, saves, reconciles, re-probes access. |
| `notchbase-find-query` | Answers questions. It drains its inbox to the newest question, runs it under a read lock, posts the answer and wakes the app. A newer question stops an older one between strides. |
| `notchbase-find-scan` | One to four listers, alive only while there are folders to list, at utility priority (`QOS_CLASS_UTILITY` and utility disk I/O on macOS, background mode on Windows). |
| watcher | macOS: an FSEvents stream on a dispatch queue. Windows: `notchbase-find-watch`, a drain-only completion-port loop over `ReadDirectoryChangesExW` per fixed drive. Other systems: none; roots are reconciled every 30 minutes instead. |
| `notchbase-find-icons` | Draws file icons by type (never by a result's own path), one icon for all folders, and app icons per app. |
| `notchbase-find-recents` | macOS only, transient: one Spotlight query for recently used files at start and when Find opens, at most every 5 minutes. |
| `notchbase-find-apps` (the librarian) | Reads what the system says about each app of the index: the name it shows, whether it only runs in the background, how often and when it was last used. Blocks until the query thread hands it a new list of apps. On Windows it first writes the shortcuts for Store apps, on a short-lived `notchbase-find-store-apps` thread. |
| `notchbase-summon` | Takes and gives back the system's search key as the switch changes; a second thread, `notchbase summon key`, runs the event tap (macOS) or the keyboard hook (Windows) while the switch is on. |

**Idle means asleep.** While Find is hidden and nothing is pending, no Find thread wakes: the writer blocks on its inbox until its next real deadline (an hourly save when the index changed, the usage file 2 s after a pick or every 10 minutes when it changed, an idle rebuild, the next reconcile of a root, which is days away), the query and icon threads block until asked, the scan pool is gone and FSEvents has no thread of ours. While Find is visible, or while work is pending, the writer ticks every second (every 250 ms while changes are settling) and progress wakes the app at most four times a second.

**Respawn.** Every order is carried out inside `catch_unwind`. If the writer thread dies anyway, the next order the app sends finds its channel closed, starts a new writer (which reloads the snapshot and launches again) and hands it the order.

### Launch

1. Load `index.bin` into memory; publish `Ready`. With no valid file, start empty.
2. Find the roots (`/`, other local volumes; fixed drives on Windows), match them to the snapshot's root marks by path and volume id. A root whose volume changed is scanned again; a root that is gone is dropped.
3. Probe Full Disk Access silently and build the plan of what to index.
4. Seed the safe cursor (`Meta.cursor` when the snapshot is valid on macOS, the current FSEvents id otherwise) and start the watcher before anything is listed.
5. Scan every root that was not complete, the one that holds home first, then one that holds an [applications folder](#apps), then the rest; with access, the guarded folders once.
6. Without a cursor (Windows), reconcile tiers 0 and 1 of every complete root gently.

### Staying live

A row is only ever created from a directory listing; change events are hints to look again. A changed file is statted and touched; a path that does not resolve relists its parent; a renamed folder stays findable at its old place until its new place has been listed; dropped events rescan a root at most once per 10 minutes. Noisy build and cache folders wait in a slow lane (5 s, 10 s on battery), so a build storm costs one relisting. The saved cursor never passes a change whose work has not landed, so a quit at any moment only means replaying a little more at the next launch.

### Apps

An app is an ordinary row of the index. Nothing more is stored for it: no flag, no tier of its own, no second list, and the files on disk are the same as before. Whether a row launches something is told from its name and from where it lies, each time it is needed (`find/rank/launch.rs`):

| System | A row is an app when it is |
| --- | --- |
| Windows | a `.lnk` or `.url` file in a folder named `Programs` that lies directly in one named `Start Menu`, or up to five folders deeper: the Start Menu of the user, the one of all users, and the one notchbase keeps for Store apps |
| macOS | a folder ending `.app` that is user content (tier 0) and not inside another package, wherever it lies; a bundle directly in the `Contents/Applications` of such an app (Instruments inside Xcode); or a link ending `.app` directly in a folder named `Applications`, or in `Utilities` directly inside one (Safari is such a link) |

A shortcut on the Desktop, in the Recent folder or anywhere else is a file. So is an `.exe`, and a bundle inside `~/Library` or inside another bundle anywhere but its `Contents/Applications`.

- **The book.** What the index cannot know about an app is kept beside it (`find/apps/`): a `Card` per app, keyed by the path's `Key`, holding the name the system shows, its folded form, whether the app only runs in the background, and how it is used. The cards make a `Book`, an immutable snapshot the librarian thread swaps into `Shared` whole; a question reads the snapshot it started with. The query thread hands the librarian the list of apps (path, key, modification date) whenever that list changes and each time Find opens; the librarian reads only apps that are new or changed, asks for use at most once a minute, and asks the query thread to answer again when the book changed. Nothing of it is saved: it is read again at every start, before Find is opened.
- **Names.** macOS: `NSFileManager.displayNameAtPath` (the Finder's name, so localized names and the names Parallels gives its Windows apps come out right) and `LSBackgroundOnly` from the bundle's `Info.plist`. Windows: a shortcut shows its file name. An app answers to the shown name first, and to the name of its file only at a word start; a row whose shown name differs from its file name is never matched by the file pipeline, so the inside of a name nobody sees finds nothing.
- **Use.** macOS: one Spotlight query for every app bundle with its last-used date and use count (the path is read from each result's item: Spotlight does not hand `kMDItemPath` out as a gathered value). Windows: the two `UserAssist` registry branches (names turned by ROT13, a 72-byte record with the count at byte 4 and the last start at byte 60), mapped to the Start Menu shortcut or to the Store app's shortcut. It adds up to 190 to an app's score (`APP_USED`, `APP_USED_LATELY`).
- **Store apps (Windows).** `find/apps/windows/packaged.rs` lists the shell's Apps folder, keeps the entries whose id holds a `!` (packaged apps) and writes one `.lnk` each into `%LOCALAPPDATA%\notchbase\apps\Start Menu\Programs`, a folder the plan treats as a third Start Menu. Type-checked from a Mac only; it has not run on Windows.

- **The list.** The query thread keeps the ids of these rows (`find/query/apps.rs`): one pass over the names per ending when Find opens, so the first letter does not pay for it. The list is kept until the index is rebuilt or Find opens again; rows added in between are read one by one; and an id is checked again every time it is used, so a stale one can only be passed over.
- **Matching.** Every question is put to the apps by the name they show. An app matches when every word is in that name, or the words run together are (the phrase, below), or a word of two or more letters starts its initials. Failing that, a word still matches loosely when its letters run through the name in order from a word start, or when it is one slip (a wrong, missing, extra or swapped letter, from four letters on) from the start of a word (`find/apps/typo.rs`).
- **Weighing.** An app scores `text + APP + use + newness`, outside every budget and with no floor. Where its file lies costs nothing, so a Start Menu shortcut under `ProgramData` stands exactly where a bundle in `/Applications` stands. A word that starts a word of the name is graded as if it started the name (`APP_WORD`). Only the creation date counts as freshness: an update does not lift an app.
- **Ahead of files.** `rank::finish` sorts in two groups. An app named at a word start (`Fit.ahead`) that lies in an applications folder or was ever used goes ahead of every file; so does anything the user picked before for the same letters (`TAUGHT`). Apps named only inside a word or loosely, background-only apps, Start Menu extras (uninstallers, help, web links) and never-used apps outside the applications folders rank with the files. `ca` puts Calendar above today's `Cargo.toml`, until `Cargo.toml` is picked for `ca` once.
- **One name, one row.** Of two apps with the same file name (the same shortcut in both Start Menus, two copies of a bundle) only the better one is listed; what was picked before counts. Two apps that only show the same name are both listed, each titled by its file name where that says more (`Siri`, `Siri AI`).
- **The row.** `FindRow.app` is set where the row is built (`rank::row_in`): the title is the card's name, or the file name without its ending, the kind is `App`, the highlights lie in the title, and the path stays the shortcut or the bundle.
- **First scan.** The plan knows the folders apps are launched from (`os::launchers`: `%ProgramData%` and `%APPDATA%` joined with `Microsoft\Windows\Start Menu\Programs`, and notchbase's own menu, on Windows; `/Applications`, `/System/Applications`, `/System/Library/CoreServices/Applications` and `~/Applications` on macOS). A folder on the way to one of them, or below one and outside any package, is queued first in its lane, before home and before the rest of every drive. Its rows keep the tier they had: what lies under `ProgramData` is still system data.
- **Every start.** The writer lists those folders again at launch as live work (`recheck_apps`), then tells the query thread to gather the apps (`Asked::Apps`), so the book is read before Find is opened. The same message follows every scan or change that lands.

**Out of sight.** `Judge::weigh` gives every candidate a distance (`Fit.away`, from `place::away`): `InSight`, `Hidden` (its own hidden flag) or `Foreign` (system, build or cache standing, inside a package, below a hidden folder, beside an app in an Applications folder or under `/System`, deep in `/Users/Shared`, inside a `.movpkg`, `.appex`, `.tmp` or `.download` folder, or a machine-made name; all found on the walk that prices its depth). `collect` keeps two shortlists of 256, each with its own floor, so neither crowds the other, and skips what is foreign by its flags outright unless the question reaches that far. `Ground.reach` is how far the question reaches (`Term::reach`: a leading dot that is no known ending reaches `Hidden`, a path reaches `Foreign`; `find.hidden` reaches `Foreign`). `rank::finish` keeps a row when it is within reach or was picked for these letters before, and widens the reach to `Hidden`, never further, when nothing in sight was really found. The empty-field list applies the same test (`suggest::Sight`).

**Order.** `rank::finish` sorts by `ahead`, then puts rows that match only by a reading (`unnamed`: no term owned by the name or by a folder above) behind the named ones, then by score. While it fills the list it lets at most three rows of one folded name through (`crowded`); the others wait and fill what room is left. A reading holds for a term only where the term admits the entry (`Ground::read`), so a path constrains kind words, dates and endings as well, and `named::voiced` refuses an app any term does not admit. `parse::path` drops the apps reading from a word that has folders before it. The resting-row hint (`words::beside`) names the nearest folder without a GUID, and up to three folders when same-named rows share that folder name too.

**Guesses.** `fuzzy::alike` counts a name as a spread match only when `telling`: the first traced letter starts a word and every later one follows its neighbour, starts a word, or is at most two letters late, with one such slip per four letters. Guesses at apps keep the looser rule (`named::loose`) and are dropped once an app was really named or eight real matches are in sight.

**Settings panes.** The door `/System/Library/ExtensionKit/Extensions` is listed like an Applications folder. `launch::launcher` takes a `.appex` folder only directly in `ExtensionKit/Extensions`. `apps::macos::read` opens the bundle: an extension whose `EXExtensionPointIdentifier` is `com.apple.Settings.extension.ui` and that allows the `x-apple.systempreferences` scheme is a pane, titled by its localized `CFBundleDisplayName` (non-breaking hyphens made plain), or by its file name when that reads as a name; every other extension is `unlisted` and never shown. A pane is never titled or matched by its file name. `find::opens` gives the address `x-apple.systempreferences:<bundle id>` that `find_pick` hands to `open`; revealing a pane opens it, and it cannot be dragged or shelved.

**Open apps.** When Find is shown, `platform::running_apps` (macOS: `NSWorkspace.runningApplications` with a regular activation policy) is turned into keys in `Context.running`; `named` adds `APP_RUNNING` (25) to an app that is open. It is a tie-breaker among apps that match alike: it never outweighs daily use (up to 190) or a better match.

**The phrase.** When every word of a question is plain text, the words joined without spaces, `-` and `_` are its phrase, from three letters on. A name holds the phrase when the same letters run through it with those three separators skipped, and such a match is graded like one word: the whole name, the name without its ending, its start, a word start or the inside, less a little for every separator skipped, so a name that matches letter for letter stays ahead. A row that holds every word of a question of two or more words takes the better of the phrase and the mean of its words. When fewer than eight candidates were found, the names that only the phrase finds are looked for as well: in the fuzzy pass's own scan for one word, and with one more pass over the names when that scan was cut short at its 2,000 or several words were typed. What was typed is remembered by its letters, so `google chrome`, `google  chrome` and `googlechrome` are one memory.

**Not covered.**

- **Settings pages**, on either system.
- **A full index.** When the entry cap is reached and system data is dropped (eviction level 4), everything under `ProgramData`, `AppData` and `Program Files` goes, and the Start Menu shortcuts with it.
- **On macOS**, the apps under `/System/Library/CoreServices` other than the Finder and its `Applications` folder (the Dock, Spotlight, …) lie in the sealed system tree and are not indexed.
- **Localized shortcut names on Windows.** A shortcut answers to its file name, not to a name a `desktop.ini` gives it.

### The system's search key

`src/system/summon/` takes the key the system's own search opens with, while `find.system_key` is on.

- **macOS.** An event tap at the HID level, inserted at the head, sees key-down and key-up. `chord.rs` decides: ⌘Space with no other of the four modifiers summons on the first press and swallows its repeats and its release; everything else passes. It needs Accessibility access (`volume::may_watch_keys`, which asks once); refused, the service tries again every 3 s and the Settings row says so. While secure input is on, taps see no keys and Spotlight opens.
- **Windows.** A low-level keyboard hook (`windows.rs`) that asks `lone.rs` for a `Verdict` on every stroke. A Windows key that goes down with no modifier held is swallowed (`Swallow`), and so are its repeats; let go with nothing pressed beside it, its release is swallowed too and Find is summoned (`Summon`), so the system never sees the key and the Start menu cannot open. A key that goes down while it is held is a `Chord`: the hook swallows that stroke and sends the Windows key down and then the stroke itself with `SendInput` (marked as ours in `dwExtraInfo`, with the stroke's own scan code and extended flag), after which everything passes. Sending both in one call keeps their order whatever the system does with injected input. An earlier version sent an unused key (`0xE8`) to mask the press instead; the Start menu still opened. `system_holds` (`GetAsyncKeyState`) tells a repeat from a fresh press after a release that was never seen, as after Win+L. The hook hears keys typed into notchbase's own windows only because the app reads no raw input (`platform::let_hooks_hear_our_windows`, see [Platform: Windows](Platform-Windows.md#hooks)); while winit's raw input was still registered, a Windows key pressed with Find open never reached the hook, so Start opened instead of Find closing.
- **A second press.** A press or a click on the magnifier within half a second of the one that opened Find is ignored (`FindPage::settling`), so a double click or a bounced key does not open Find and close it again. For the same half second, focus that slips away with no mouse button down is taken back instead of closing Find (`find_focus::focus_loss`).
- **The half second is real time.** `FindPage::settling` ends when half a second of painted frames has passed or half a second on the clock since `enter`, whichever comes first (a panel that sits still paints no frames, and counting frames alone left it "opening" for as long as it was open: a second ⌘Space was ignored and every lost focus was taken back).
- **Focus lost.** `find_focus::focus_loss` decides from a `Lost`: a focus loss on the active notch while Find shows closes it only when `left` (from `Signs`: something is being launched, a modifier is held more than a second after opening, or a mouse button is down or was pressed since Find opened (`platform::pressed_within`) with the pointer outside the notch). Otherwise, while the app holds the keyboard, it is taken back (`FocusLoss::Kept`, at most `KEPT_AT_MOST` times a visit, counted in `Finding.kept`). `App::keep_find_focused` runs each frame and feeds the same decision every 250 ms when the window that shows Find does not have focus. The keyboard is released only when no notch is expanded (`Notches::any_expanded`); asking `visible_page` there gave it back while a summoned panel was still fading in. The page takes its caret again whenever the app says it is typing.
- **A click elsewhere.** While Find has the keyboard, the notch window takes only the clicks in the notch's input zone (`frame::takes_every_click`; a text field on any other page, a shortcut being rebound and a file drag being caught still take every click, and Find keeps one only while a press that started on the panel is held), and the pointer leaving the zone does not hand the keyboard back (`frame::hands_focus_back`). So a click beside or below the panel reaches what is underneath; until 0.23.4 the whole 680 x 600 window took it and Find stayed open. `Finding::shown` also starts a `platform::PressesElsewhere` for the visit and `left` drops it: on macOS an `NSEvent` global monitor for left, right and other mouse-down, which AppKit only calls for presses delivered to other apps, so it hears the menu bar and the Dock, which move no focus; elsewhere none, since there such a click moves the focus. `App::close_find_on_a_press_elsewhere` runs every frame before the focus watchdog: a press heard while the active notch shows Find, with the pointer outside the notch and no drag out (`pressed_away`), closes it like a lost focus does (`went_elsewhere`: the keyboard is let go without handing it back, and the notch collapses).
- **Pins.** `App::drop_stale_pins` runs every frame and unpins a notch that is pinned although no tool holds it and it does not hold the keyboard (`pin_wanted`). It only ever unpins: pinning opens the panel. `FindPage::rest` sends `Typing(false)` whenever a visit ends, with or without a caret.
- **Launch.** `FindPage::pick` starts a `Launch` (`pages/find/launch.rs`) for `How::Open` on an app row: `lift` fades everything the page drew and paints the row's emblem at `Launch::pose` (swell, rise to the top middle, shrink, fade) with a halo. The app side (`find_pick`) opens the app at once, sets `Finding.lifting` 240 ms ahead instead of collapsing, and `sync_launch` then collapses the notch that is still finding and calls `Notch::swallow` (`notch/moves/launch.rs`): once the panel has closed, the `gulp` spring is kicked and added to the silhouette's height.
- **One screen.** `find::view::ONE_SCREEN` (4) is both the most rows the page shows at once and the number of suggestions below which the empty list is topped up.
- **The bar.** `FindPage::height_for` is the bar (`FIELD_H`, 46) plus one row per result up to one screen; `held` keeps the last answered count so the panel does not jump while an answer is on its way, and `resting` (nothing typed, suggestions not asked for) is the bar alone. The access row and the pins strip that sit under the resting bar fold away and back on one quick spring each (`Fold`: `access_fold`, `strip_fold`), and the height counts them by how far they show, so the first keystroke and clearing the text glide instead of jumping. `field::paint` draws no box; it takes the selected row as the hit, paints its emblem at the right end and `completes` the typed text with the rest of its name, or the whole name after a dash.
- **Every app.** `launch::launcher` takes any `.app` folder that is not `within_an_app`. `named::unlisted` hides one that is neither `stands_alone` nor `homed` until its card says it is listed; `apps::macos::read` lists a bundle with a `Contents/Info.plist` (or a `Wrapper`) that is not `kept_for_later` (a `Caches` or `.noindex` folder above it), and marks it backstage for `LSBackgroundOnly`, or for `LSUIElement` where `kept_by_the_system`; `named` ignores backstage once the app was used. `apps::every` is Spotlight's list of application bundles; the librarian hands it to the writer as `Order::Adopt`, and `Writer::adopt` plants the ones the plan does not want and the index does not hold (`writer/adopt.rs`). `rank::finish` drops a package row when an app directly inside it is listed.
- **Hover.** `Select::point` says whether the selection changed, and the list answers with `haptic(Feel::Tick)`.
- **Pins.** `find.pins` in the config (`FindPin`: path, name, app, folder) becomes `FindView.pins`, rows with icons from the same icon thread. `pages/find/pins.rs` draws the strip and owns the drag: `following` keeps the saved order except while a pin is held, `slot_at` and `moved` reorder as the held pin's centre crosses a slot, a spring per pin slides the others, and `Chose::Moved` is sent on release when the order changed. A secondary click (`Hit::Menu` on a row, `Chose::Menu` on a pin) opens the notch's context menu, `ui/menu.rs`: `Menu<T>` is held by the page, `open(at, items)` shows it, `show` paints it and returns what was chosen, `close` fades it away, and `reach` says how far down it needs the page to go (`FindPage::height` grows by it). `reach` is `None` from the moment the menu starts to leave, so the page shrinks back at once; the menu itself remembers the rectangle it last stood in (`Open::stand`) and fades there, rather than being placed again inside the smaller page. `pages/find/menu.rs` only lists what Find offers (`items`: pin, unpin or the greyed full notice, and reveal). The page sends `Action::FindPin`, `FindUnpin` and `FindPinsOrder`; `app/find/actions.rs` changes the list (`with_pin`, `in_order`) and saves it.
- **The press.** `App::summon_find` closes Find when it shows, and otherwise arms the Find page on the notch nearest the pointer (`Notch::summon_find`). A summoned page is run from its first frame although the panel has not come into view (`Notch::summoned`), so the keyboard is taken at once and letters typed while the panel opens reach the field.

### Privacy and prompts (macOS)

Without Full Disk Access, Find never touches Desktop, Documents, Downloads, iCloud and the other guarded folders by any route: the plan rejects them at the two gates every path passes (the change coalescer and the scanner), and icons are drawn from probe files of the same type in Find's own folder. Find asks once (a notch message and a Notifications entry), shows a dismissable "See every file" row at the top of its list while access is limited, and re-probes each time it opens. When access is granted, one deep pass indexes the guarded folders, resumably.

### The app side

`App.find` is a `Finding` (`src/app/find/mod.rs`): the service, the icons, the last answer and status. On every wake `sync_find` compares the service's version (and the icons' version) with what it last saw and returns at once when nothing changed; otherwise it takes the answer, asks for its icons, and builds the `FindView` the page reads. The view is `Off` exactly when `find.enabled` is off (or the service is not running, as in previews), so the settings switch follows the config.

- `FindShown(true)` goes through `find_opened_on(id)` (`src/app/find/focus.rs`): that notch becomes the active one, every other notch leaves Find silently, and if the keyboard was held on another display it moves to this one without being released. Find is open on one display at a time.
- `Focused(false)` counts only for the active notch's window. When it fires while Find shows and no drag is out, that notch collapses. So does a press anywhere outside the notch while Find shows (**A click elsewhere**, under [the system's search key](#the-systems-search-key)).
- `FindPick` (`src/app/find/pick.rs`): open checks that the file still exists and only then hands it to the system (never `open_path`, which would create a folder for a missing extensionless path); a missing file is reported once and dropped from the index and the rows. After an open or a reveal the notch collapses and the opened app keeps the focus. Drag uses `drag_paths`, Shelve adds to the Shelf. Each pick is then reported to the service, which learns from it.

## Files

Under the `find` folder of the data directory (`%LOCALAPPDATA%\notchbase\find` on Windows, because the index describes one machine and must not roam): `index.bin`, `usage.bin`, their `.tmp` twins while a write is in flight, and a transient `icons` folder for probe files. All are readable by their owner only. The folder itself is never indexed or watched.

### The frame

Both files share one frame (`src/find/store/frame.rs`). Everything is little-endian.

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 8 | magic `NBFIND\r\n` |
| 8 | 4 | version, 1 |
| 12 | 1 | kind: 1 index, 2 usage |
| 13 | 1 | system: 1 macOS, 2 Windows, 3 other |
| 14 | 2 | block count |
| 16 | 8 | total file length |
| 24 | 8 | reserved, zero |
| 32 | 20 per block | directory: a 4-byte tag, the raw length and the packed length |
| after the directory | | the blocks in directory order, each packed with lz4 |
| last 32 | 32 | SHA-256 of every byte before it |

A file is written as `<name>.tmp` and renamed into place. It is a cache, so there are no migrations: any refusal (wrong magic, version, kind or system, a wrong length or checksum, a block over 1 GiB or that does not unpack to its stated length, a broken invariant) deletes the file and Find starts over from the disk. A change to the name fold must bump the frame version.

### `index.bin`

| Tag | Content |
| --- | --- |
| `HEAD` | entries, arena length, the safe cursor, saved at, eviction level, flags (`GUARDED_DONE`), then each root: path, volume id, `COMPLETE`, scanned at, swept at |
| `PRNT` | parent of each entry, delta-coded in breadth-first order |
| `NAME` | the folded names, NUL-separated |
| `CASE` | one bit per name byte that was an upper-case ASCII letter |
| `MTIM`, `CAGE` | modified time, and modified minus created |
| `FLAG` | folder, hidden, tier, inside a package, cloud, link, exotic |
| `XOTC` | the original spelling of names the fold changed beyond letter case |

### `usage.bin`

`HEAD`, then `FILE` (48-byte records: key, file id, last open and edit, opens and edits, dayparts, app slots, skips, flags), `FOLD` (folder heat), `PREF` (at most 12 bytes of typed text per memory, without its spaces, `-` and `_`, and the file it led to), `PAIR` (what you open after what), `APPS` and `PATH`. At most 4,096 files, 512 folders, 2,048 typed memories and 1,024 pairs; under 300 KB on disk. Nothing leaves the machine. Clear it from Settings, with `find.learn = false`, or per folder with `find.excluded`.

## Tuning

Every number is a named constant; the ones worth knowing:

| Where | Constant | Value |
| --- | --- | --- |
| `find/writer/upkeep.rs` | `FAST_TICK`, `TICK` | 250 ms while changes settle, 1 s while busy or visible; asleep otherwise |
| | `SAVE_EVERY`, `SAVE_LATEST` | save hourly when changed; on battery wait for mains, at most 6 h |
| | `OVERLAY_ROOM`, `ROW_BYTES` | 60 MiB of the memory budget kept for rows added since the last rebuild; 35 bytes per entry |
| `find/writer/reconcile.rs` | `RECONCILE_AFTER`, `SWEEP_AFTER`, `RECONCILE_UNWATCHED`, `QUIET` | 7 days, 30 days, 30 minutes; after 60 s without changes, on mains, hidden |
| `find/watch/coalesce.rs` | `SETTLE`, slow lane | 250 ms; 5 s (10 s on battery) |
| `find/scan` | `BATCH_ROWS`, `LIVE_DIRS` | 4,096 rows per listing batch; 2,048 folders before a live errand moves to the background lanes |
| `find/weights.rs` | every ranking and learning weight | see the build specification, section 6.4 |

The memory budget is a 32nd of RAM between 192 and 512 MiB, sized so a rebuild (old and new index side by side) fits; `find.max_entries` overrides the entry cap.

## Testing

Every test works in temporary folders it creates; none lists, watches or queries the real home folder. `src/find/tests/engine.rs` drives the real writer over a fake system (`find::writer::tests::World`: roots, clock, power, access and a recording scanner) and the real query thread end to end; `src/find/tests/apps.rs` follows a Start Menu shortcut the same way, from its listing to its app row. The million-entry timings are ignored tests run by hand in release:

```
cargo test --release -p notchbase find::tests::timings -- --ignored --nocapture
cargo test --release -p notchbase first_scan_within_budget -- --ignored --nocapture
```

The seven page states have [previews](Previews.md) (`find`, `find-results`, `find-actions`, `find-none`, `find-indexing`, `find-access`, `find-off`).
