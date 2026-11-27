# Updates

notchbase updates itself from its [GitHub releases](https://github.com/sleepdisrupter/notchbase/releases). Three seconds after it starts, every 3 minutes after that (2 minutes after a failed check), and whenever you open the notch (at most once a minute), it reads the newest release's `latest.json` to find the build for its platform. When one is newer than the running version, the notch says "notchbase x.y.z is available" right away, and it shows up on the **Notifications** page (Settings, then **Notifications**; a blue dot on the gear says it's there) with an **Update Now** button. **Update Now** installs straight away, even while Claude is working or an activity is live.

With Settings › notchbase › **Automatic updates** on (`updates.automatic` in `config.json`), it also installs by itself: it waits until the notch has been idle for 20 seconds, downloads the release while the notch shows an **Updating** live activity whose ring fills with the download, checks the file's SHA-256, swaps it in and relaunches. The new copy greets you with "Updated to notchbase x.y.z": a notice with a **View Changes** button when that version has notes you haven't seen (see [What's New](#whats-new)), and a plain message when it has none. With it off, notchbase still checks and tells you, and installs only when you press **Update Now**.

**Update Now** skips the wait: it closes the panel so the Updating ring shows, downloads, and installs straight away. While downloading, the notification shows the progress, and once the download is ready it offers **Restart Now**. After a failed check or download it says why and offers **Try Again**. The Settings row's subtitle, and the **notchbase** tile's second line, also show what the updater is doing: the version, "Up to date", download progress, or why the last check failed.

## What's New

A release can carry **notes**: a short list of what changed, each change a glyph, a title and one line, and one picture at most. They are compiled into the build (`src/whats_new/notes.rs`), not fetched, so they are there offline and always match the copy that shows them. A copy offers its notes until you have seen them, and never again after that.

### What you see

- **The notice.** When a copy starts and its notes are unseen, the notch shows a notice for 8 seconds: a pearl badge with a spark, "Updated to notchbase 0.14.0" and a **View Changes** button. On an island it is one 37 point capsule with the button tucked into its end; beside a camera the badge and just "Updated" sit in the left wing and the button, reading **Changes**, ends the right wing, so the notice stays one slim strip. It shows on every display's notch, and it stands in for the start-up greeting however the copy arrived: updated by itself, installed over an older copy, or simply started before the notes were seen. A start that has a problem to tell you (a shortcut another app holds, a tool that did not load) shows that message and leaves the notice out, because the message can be read nowhere else and the notes still open by themselves. A pointer resting on the button holds the notice, as on any [notice with controls](Notch.md#tool-notices).
- **View Changes** opens the **What's New** page on the notch it was pressed on, and ends the notice on every display.
- **Without pressing it**, the page shows by itself the next time the notch is opened in full, by resting on it or by a click: it opens on What's New instead of the page it would have opened on, ahead of a filled Shelf and a known song. A click on a live activity that has a page of its own still goes there (the assistant's to Agents, a download to the Shelf), and so does anything that asks for a page by name, such as a drop; What's New waits for the next plain open. See [Notch](Notch.md#opening-onto-whats-new).
- **The page** (see [Pages](Pages.md#whats-new-pageswhats_new)) is a header with **Done** at its right and the changes as a short list of bullets, under one picture when the release carries one (0.14.0 carries none). Done, a tab, the gear or the magnifier leaves it, and leaving it that way is taken as having seen it; the pointer leaving closes the panel as on any other page, and the panel never reopens into it by accident.
- **Later**, the **What's New** row behind the **notchbase** tile in Settings ("See what changed in 0.14.0") opens the page again. The row is there whenever the running version has notes.

A build without notes at or below its own version shows none of this: no notice, no row, and the plain "Updated to notchbase x.y.z" message after an update.

### When the notes count as seen

`updates.seen` in `config.json` holds the version whose notes you last saw (see [Configuration](Configuration.md#updates)). The notes on offer are the newest ones that are not newer than the running version (`whats_new::latest`), and they are **unseen** while their version is above `updates.seen` (`whats_new::unseen`). Four things mark them seen, and each writes the running version to `updates.seen`:

- pressing **View Changes**,
- pressing **Done** on the page,
- walking away from the page while it is on offer, by pressing a tab, the gear or the magnifier in the header: that is a choice, so the page does not take the next open as well,
- the page staying up on any display for 1.2 seconds without a break (`SEEN_AFTER` in `app/whats_new.rs`). A shorter glimpse that ends with the pointer leaving does not count: the wait starts over, and the page comes back at the next open.

The page stays as it is when the mark is made: what it shows never depends on it.

- **A first run tells you nothing.** When there is no `config.json`, the one that is created carries the running version in `updates.seen` (`ConfigStore::create`), so somebody new is not shown "changes" to an app they have just met.
- **A file from before the key reads as never seen.** Everybody who updates from 0.13.0 or earlier gets the notes once. Saving another setting writes `"seen": ""`, which still means never seen.
- **A release without notes of its own** (a small fix) offers nothing to someone who saw the last notes. Someone who skipped them is offered the newest notes there are, under their own version: the notice names the running version, the page and the Settings row name the version the notes were written for.
- **A `config.json` with a mistake** at start leaves the notes unoffered for that run, because nothing could be remembered; they are offered at the next start with a file that reads.
- **A [preview](Previews.md)** never offers the notes and never writes the mark.
- **Editing `updates.seen` while notchbase runs** takes effect within a second: set it to an older version (or `""`) and the next open shows the page again. The notice itself is only posted at start.

### The pieces

| Piece | Where |
| --- | --- |
| The notes, and which are the latest and whether they are unseen | `src/whats_new/` (`notes.rs` holds the list) |
| `updates.seen`, and the stamp a first run gets | `src/config/updates.rs`, `src/config/store.rs` |
| What the app remembers while it runs (`News`), the notice at start, following the page, marking seen | `src/app/whats_new.rs` |
| The notice and its button, and opening onto the page by itself | `src/notch/notice/whats_new.rs` |
| The page | `src/pages/whats_new/` |
| A release's picture, when it carries one | `src/ui/picture.rs` |
| The Settings row | `src/pages/settings/news.rs` |

`App::new` works out once, right after the configuration opens, whether the notes are unseen (never in a preview, never with a broken file), and `AppView.news` carries that to every notch each frame. `resumed` posts the notice last, once the hotkeys, the tools and the assistant have started, through `Notch::show_updated` (`notch/notice/whats_new.rs`). That leaves the notice out on a notch that is showing a message by then: `resumed` runs without a frame in between, so a message live at its end was said at start (a start-up problem, a shortcut that could not be registered, what a tool said with `Notify` as it started), and the notice would cover it before it was ever painted. A tool's own notice at start gives way to the Updated notice as to any other. When the notes are unseen no start-up greeting is queued at all. `Action::ViewChanges` (the notice's button and the Settings row) is routed in `paint_notch` to `view_changes` with the window that sent it, so only that notch opens; `Action::ChangesSeen` is the page's Done, and what the panel sends on the frame a press in its header takes it off the page while the notes are due (`walked_away_from_news`). `follow_whats_new_page` runs every frame beside `follow_notifications` and asks every notch for its visible page, and `whats_new_wakeup` wakes the app when the 1.2 seconds are up.

The notice is best effort: it is left out at a start that has a message to show, and like any notice it is replaced by whatever the notch shows next, waits under an assistant prompt with its clock running, and is not posted again to a notch window that is re-created later. Opening onto the page is what makes sure the notes are seen.

### Writing the notes for a release

1. Raise `version` in the root `Cargo.toml` first. Notes for a version above the running one are never shown, and a test fails on them (`newest_notes_arent_from_the_future`).
2. In `src/whats_new/notes.rs`, put a `Release { version, picture, changes }` at the top of `NOTES`; releases are listed newest first. Only the newest release is ever shown, and a picture is in the binary byte for byte, so the entry it replaces can go, along with any picture it had.
3. Write one `Change` per thing worth telling, most important first, and stop at three or four: the page is a list to take in at a glance.
   - `glyph`: the feature's own glyph from `ui::glyph` (`crates/paint/src/glyph/`), as it is. It is the bullet: painted bare, with no well, on the title's line. The header's icons are in view right above the list, so a glyph that is also one of them points at it: the magnifier on the Find bullet is the Find button's, and a change that is not about a tab never wears that tab's icon (the spark is the Agents tab's).
   - `title`: the feature's name, or a few words in sentence case about what it does for you ("Find", "Your usual picks come first").
   - `caption`: one clause in the present tense, about what it does for you or where to press, with no full stop, and short enough for one line. 0.14.0's longest takes 223 of the 313 points an island gives a caption, measured with a Mac's SF Pro. A longer caption wraps to a second line and ends in "…" past that. The computer is "this Mac" or "this PC", chosen with `cfg` the way the caption about what Find learns is. A release that is about one system only is listed for that system alone, also with `cfg`: 0.15.0's notes (Keep Awake with the lid closed, the padlock on the lock screen) are a Mac's, so a PC on 0.15.0 still offers 0.14.0's.
4. Leave `picture` at `None` unless a picture shows something the list cannot (see [Giving a release a picture](#giving-a-release-a-picture)). 0.14.0 has none: its one new button, the magnifier, is in the header right above the list, and the first bullet wears the same glyph.
5. Keep the page short. 0.14.0's three changes come to 163 points in both styles, a third of the 492 points a page may be on an island and of the 474 beside a 32 point camera before it scrolls. `this_release_fits_without_scrolling` (`notch/tests/whats_new_page/shown.rs`) fails when either would scroll, and `captions_fit_one_line_on_island` (`pages/whats_new/tests.rs`) when a caption wraps or a title is cut short on the computer running the tests.
6. Run `cargo test whats_new`. The tests also check that versions parse and descend, that every release says something, that no title or caption ends in a full stop or adds a second clause with ", and", and that every picture decodes and is exactly as wide as the page's column (`whats_new::PICTURE_WIDTH`, 340 points).
7. Look at it in both styles with the `whats-new` and `updated` [previews](Previews.md), in the app or as [pictures without a window](Previews.md#looking-at-a-state-without-a-window):

   ```bash
   NOTCHBASE_SNAPSHOT=whats-new,updated cargo test notch::tests::snapshot -- --ignored
   ```

8. To walk the real path, start a [development copy](Building-and-Testing.md#running-a-development-copy-next-to-your-real-one) whose `config.json` already exists and has no `updates.seen`, or an older one. A missing file would be a first run and show nothing.

### Giving a release a picture

A release shows one picture at most, between the header and the list, and most need none: the page is a list to take in at a glance, and a picture is the loudest thing on it. One earns its place only when it shows something the reader cannot already see, such as a control that appears somewhere else. The header's own buttons are in view right above the list, so a picture of them only repeats them.

- **It is a `Shot`**: a `static` in `notes.rs` holding one `Picture` for an island and one for a notch beside a camera, each embedded with `Picture::new(include_bytes!(..))` from `assets/whats-new/`, and handed to the release as `picture: Some(&..)`. `Picture::new` is marked as expected dead code outside tests while no release carries a picture; the compiler says so when the first one arrives, and the attribute goes.
- **Each variant is exactly 680 pixels wide**, which at two pixels to the point is the page's column (`whats_new::PICTURE_WIDTH`, 340 points), so it is shown texel for pixel. `pictures_decode_at_page_width` holds both to that.
- **It is not a photograph of a screen.** Render a [preview](Previews.md) through the same headless frames as the [snapshots](Previews.md#looking-at-a-state-without-a-window) in an ignored test, at the scale that makes the crop 680 pixels wide, and take the crop from the notch's own geometry, so the picture can be remade when what it shows changes. Glass in those frames is tinted by the snapshot's desk.
- **It has to read in both styles.** Beside a camera the panel hangs from the top edge of the screen with no desk above it, so a crop of the panel's top has no frame along its top edge there and reads as part of the panel around it. Look at both variants on the page with the `whats-new` preview before keeping them.

## Platforms and targets

Every build knows which platform it is from the Rust target it was compiled for, and only ever installs a release made for that platform:

| Rust target | Platform in `latest.json` | Release asset |
| --- | --- | --- |
| `x86_64-pc-windows-msvc` | `windows-x86_64` | `notchbase-<version>-windows-x86_64.exe` |
| `aarch64-pc-windows-msvc` | `windows-aarch64` | `notchbase-<version>-windows-aarch64.exe` |
| `aarch64-apple-darwin` | `macos-aarch64` | `notchbase-<version>-macos-aarch64.zip` (the signed `.app`) |
| `x86_64-apple-darwin` | `macos-x86_64` | `notchbase-<version>-macos-x86_64.zip` (the signed `.app`) |

An Intel Mac build that is running under Rosetta on Apple silicon asks for `macos-aarch64`, so it moves onto the native build at its next update.

## How it is built

| Piece | Where |
| --- | --- |
| Version parsing, choosing the newest release, the platform name | `notchbase-release`: `crates/release/src/version.rs`, `crates/release/src/release.rs` |
| Reading `latest.json` and downloading with a checksum | `notchbase-release`: `crates/release/src/server/mod.rs`, on top of `notchbase-net` (`copy_hashed` and the HTTPS agent) |
| Unpacking, swapping the app and relaunching | `src/update/install.rs` |
| The background thread and its status | `src/update/mod.rs` |
| Idle detection, the live activity, the Settings row | `src/app/updates.rs` |
| The notes a release carries and showing them once | see [What's New](#the-pieces) |
| Swapping an app in place, relaunching and handing over | `src/installer/location.rs`, `src/installer/handoff.rs` |

Every build updates from `sleepdisrupter/notchbase`, or from the repository named in `NOTCHBASE_RELEASES_REPO` when it was built (see [Releasing a fork](#releasing-a-fork)). No keys are involved: the app only downloads public files.

Installing moves the running app aside (`notchbase.exe.old`, or `notchbase.app.old` on macOS) and puts the new one in its place. If placing the new copy fails, the old one is moved back. On macOS, a `tools` folder inside the old bundle is carried into the new one, and so is a `config.json` left there by a version older than 0.5.4, which the new version then moves to the data folder. The new process is started with `--replaces <pid>`. It waits for the old process to exit, then deletes the old copy. It then takes the single-instance lock the old process held, retrying for about 5 seconds if it is not free yet (see [Architecture](Architecture.md#one-running-copy-per-data-directory)). Downloads are staged in `<data dir>/updates`, which is cleared each time notchbase starts.

## Releases on GitHub

Each version is one GitHub release, tagged `v<version>`, with these assets:

| Asset | What it is |
| --- | --- |
| `notchbase-<version>-<platform>.zip` or `.exe` | The app for each platform in the table above, which the updater downloads |
| `notchbase-setup.dmg`, `notchbase-setup.exe` | The installers people download first |
| `notchbase-sdk.zip`, `notchbase-skill.zip` | The plugin SDK kit and the notchbase skill |
| `latest.json` | What installed copies read to find the release |

`latest.json` names the version and a build for each platform:

```json
{
  "version": "0.24.0",
  "notes": "One line about the release",
  "builds": {
    "macos-aarch64": {
      "url": "https://github.com/sleepdisrupter/notchbase/releases/download/v0.24.0/notchbase-0.24.0-macos-aarch64.zip",
      "sha256": "…",
      "size": 23456789
    }
  }
}
```

The app fetches it from `https://github.com/<repo>/releases/latest/download/latest.json`. That address always points at the newest release that isn't a draft or prerelease, and it is a plain download rather than a call to the GitHub API, so the checks every few minutes never run into GitHub's limit of 60 API requests an hour. A repository with no release yet, or a release without a build for this platform, means there is no update, not a failure. `notes` is for people reading the file; the app shows the What's New notes it was built with.

The Mac and Windows builds are made on their own systems, one after the other. Whichever goes second downloads the release's `latest.json`, adds its platform and uploads it again, so copies on the first platform can update as soon as their build is up.

Mac builds of notchbase itself are signed with the pinned release certificate (see [Platform: macOS](Platform-macOS.md#signing-and-permissions)). A build signed any other way makes every Mac user grant their permissions again after updating.

## Releasing a fork

A fork can ship its own releases, and its copies update from them:

1. Install the [GitHub CLI](https://cli.github.com) and log in with `gh auth login`.
2. Set `NOTCHBASE_RELEASES_REPO` to your fork, like `you/notchbase`. Without it, the scripts use the GitHub repository of your `origin` remote. The value is built into the app and the installer, so they update from your releases.
3. On a Mac, run `./scripts/release-github.sh --notes "What changed"`. Set `NOTCHBASE_SIGN_IDENTITY` to your signing certificate, or the build is signed ad hoc and macOS asks for its permissions again after every update.
4. On Windows, run `scripts\release-github.bat -Notes "What changed"`.

`--dry-run` (or `-DryRun`) builds everything and prints what it would upload without touching GitHub.

## Limits

- GitHub allows release files up to 2 GB. The app is well under that.
- macOS releases are signed with a self-signed certificate, not a Developer ID. The updater downloads over HTTPS without the quarantine flag, so the update still opens. The very first download from a browser still needs a Developer ID signature and notarization to open without a warning.
