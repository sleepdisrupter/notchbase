# Video downloads

Drag a browser tab onto the notch and the video on that page is downloaded. The notch opens into two halves, **Video** and **Audio**: let go on the left for the video, on the right for only its sound. The download shows as a live activity with a line of light running under the notch, and the finished file lands in Downloads and on the [Shelf](Shelf.md). A link or a thumbnail dragged off a page works the same way.

It works with YouTube, Instagram, Facebook, Vimeo, TikTok, X, Twitch, Reddit, Dailymotion and, through the downloader's own list of well over a thousand sites, most other pages with a video on them. Two things it will not do: a copy-protected (DRM) stream is refused with a message saying so, never worked around, and a site that only shows a video to a signed-in visitor needs you signed in to it in your browser (see [Signing in](#signing-in)).

The code is in three places: `src/webvideo` (recognising pages, the download engine, the downloads themselves), `src/webvideo/browser` (finding out what the dragged tab is showing) and `src/app/videos.rs` (following the drag). The notch's side is `src/notch/live/video.rs`, `src/notch/drop_zone/mod.rs` and `src/notch/drop_zone/layouts.rs`.

## Which pages (`webvideo/site.rs`)

There are two questions, and a function for each.

`recognize(text)` answers "is this a video page on a site whose video pages are known by their shape?" It reads every shape a YouTube address takes (`watch?v=`, `youtu.be/`, `/shorts/`, `/live/`, `/embed/`, the `m.` and `music.` hosts), Instagram's `/reel/`, `/reels/`, `/tv/` and `/p/`, and the video addresses of Facebook, Vimeo, TikTok, X, Twitch, Reddit and Dailymotion. A drag of one of these is a drag of a video however it is dragged, so it wins over the thumbnail the browser also offers as a picture. YouTube and Instagram addresses are made canonical, so the playlist, the timestamp and the tracking parameters are left behind and the same video is never fetched twice at once.

`recognize_any(text)` answers "could there be a video here?" It accepts those, and any other page on the web (`Site::Other`), which the downloader is left to search. The exceptions are YouTube and Instagram pages that are not one video (a feed, a channel, a playlist), where "the video on this page" would be whichever happens to be listed first, and addresses that are not the web at all (`localhost`, `about:`, `file:`, a browser's own pages). This is what a dragged tab is tested with.

In both, the scheme and `www.` may be missing, because that is how address bars show them.

## How a tab is caught

Browsers drag tabs in three different ways, and only one of them says what the tab is:

| Browser | What a tab drag is | How the address is found |
| --- | --- | --- |
| Safari | a real drag carrying the page's address | read off the drag |
| Firefox, Zen and other Gecko browsers | a real drag carrying an empty marker (`org.mozilla.MozillaWildcard` on a Mac; `text/x-moz-text-internal` on Windows, which does hold the address) | asked from the browser on a Mac, read off the drag on Windows |
| Chrome, Edge, Brave, Arc and other Chromium browsers | not a drag at all: the tab becomes a window that follows the pointer | asked from the browser |

**A drag that carries the address** goes through the ordinary drop target (see [Shelf](Shelf.md#dragging-things-in)). The drop target reports a drag of a page with `Arrival::Offer(link)` as it enters and drops it as `Dropped::Page`. A page counts when it is all the drag carries (a tab, an address dragged from the address bar), or when it is one the app asked for with `filedrag::prefer_pages` (a known video page), which also wins over the thumbnail or shortcut file the browser offers alongside. `App::take_videos` starts a download for each dropped page, unless its address plainly names a file (`.png`, `.pdf`, `.mp4` and so on), which goes to the Shelf as it always has, along with everything else.

**A drag that carries nothing** is caught by watching the mouse (`App::offer_videos`, called once a frame from `update_drags`). When the button is held and the pointer comes within the notch's reach, the app asks `browser::frontmost()` whether the app in front is a browser, and if so reads its front tab's address on a `notchbase-tab` thread, so the answer is ready by the time the tab arrives. When the pointer then comes up under the notch (`Notch::tab_zone`), the notch opens into the drop target, and letting go on it starts the download. If the button is let go on the notch before the answer arrives, the app asks again (three tries over about a second) and downloads the video when it knows.

Details that matter:

- **Where a tab counts as brought to the notch.** The drop target opens when the pointer is in a band that reaches about 80 points below the notch (`Notch::tab_zone`), so a tab never has to be pushed against the top of the screen, where macOS has window gestures of its own. That band covers a tab strip sitting right under the notch, and a tab is also dragged just to reorder it, so the pointer must also have risen at least 16 points since the press: reordering slides a tab sideways, bringing it to the notch lifts it. Once open, all of what the notch opened into counts, and dragging back out of it closes it with nothing downloaded.
- A press that starts inside the notch's reach counts as coming to the notch once it has travelled 40 points, so a plain click near the notch still goes through to the window under it.
- While the offer is showing, `filedrag::catch_everything(true)` makes the notch window accept a drop it cannot read. Firefox tears a tab that nobody took off into a new window; taking it leaves the tab where it was.
- A drag that carries something the notch takes as it is (a picture, a file) is never treated as a tab, so dragging a picture off a video page shelves the picture.
- A tab that is alone in its Chromium window is dragged as the whole window, which is also how a window is dragged to the notch for [layouts](Notch.md). The picker then grows two more cells, **Download video** and **Download audio only**, and only letting go on one of those downloads.

### Video or audio

The question is answered by where the tab is let go: `Notch::takes_video_at(cursor)` gives `Kind::Video` on the left half of the open notch and `Kind::Audio` on the right (or the cell under the pointer beside the layout picker), and the half under the pointer lights up as it moves. There is nothing to click and nothing to answer afterwards. A drop that arrives before the notch has opened is taken as the video.

### Asking the browser (`src/webvideo/browser`)

`frontmost()` returns the app in front when it is registered to open web links, so a browser nobody has heard of yet counts: on macOS that is `NSWorkspace.URLsForApplicationsToOpenURL`, on Windows the programs under `Clients\StartMenuInternet` in the registry. It also works out which cookie store the browser has (see [Signing in](#signing-in)).

`addresses(browser, wanted)` reads the front window through the accessibility API and returns what it finds, likeliest first, stopping as soon as one address is `wanted`:

- **macOS** (`webvideo/browser/macos.rs`): each engine keeps the address somewhere different. Chromium puts it on the window (`AXDocument`), Gecko on the page element (`AXWebArea`'s `AXURL`), and Safari only in its address field, so all three are read: the window's document, then a breadth-first walk of the window (at most 500 elements, 12 deep, 1.5 seconds, never into a page) collecting page elements and text fields. It needs the Accessibility permission the window layouts already use. It takes about 50 ms.
- **Windows** (`webvideo/browser/windows.rs`): UI Automation. Chromium and Gecko both give the address as the value of the window's first `Document`; the fallback is the edit fields outside any document, found with the control-view walker under the same limits.

## The engine (`webvideo/engine.rs`)

Downloads are done by three programs kept in `<data dir>/video-engine`, fetched the first time a video is dropped (about 110 MB, in parallel, shown as **Setup** with its own progress):

| Program | Why | From | Checked against |
| --- | --- | --- | --- |
| yt-dlp (the unpacked build, which starts in 0.2 s where the single file takes 6) | finds and fetches the video | its latest GitHub release | the release's `SHA2-256SUMS` |
| Deno | runs the page script YouTube hides its addresses behind | its latest GitHub release | the `.sha256sum` beside the download |
| ffmpeg | joins the picture to the sound | a pinned `ffmpeg-static` release | a SHA-256 pinned in the source |

Each download is hashed as it streams and thrown away if it doesn't match, unpacked into a staging folder, run once (`--version`: it proves the program starts here and gets the Mac's first-launch check out of the way), and only then moved into place. Whatever is already in place is not fetched again.

Sites change under yt-dlp every few weeks, so an old copy is the usual reason a download that used to work stops. `engine::refresh` compares the installed copy's checksum with the newest release's and replaces it when they differ. That runs after a successful download, once it already shows as saved, when the last look was more than 20 hours ago, and at once when a download fails for a reason the app doesn't recognise.

## A download (`webvideo/mod.rs`, `job.rs`, `progress.rs`)

`Videos::start(page, options)` gives each download a `notchbase-video` thread, or returns the download already fetching that page. The thread:

1. Makes sure the engine is there (`Stage::Preparing` while it is being fetched).
2. Runs yt-dlp (`job::arguments`) with the user's own yt-dlp config ignored, pointed at the bundled Deno and ffmpeg, into `<data dir>/video-engine/staging/<pid>-<id>`, fetching eight fragments at a time and only the first video of a page that has several. Live and upcoming videos are refused.
   - **Video** takes the biggest picture there is, up to `videos.quality` (8K by default, so in effect no limit), merged with its sound into an MP4. See [A file that always plays](#a-file-that-always-plays-webvideocodecsrs) for which pictures it will take.
   - **Audio** takes the best sound and keeps it as it is in an `.m4a` when the site sends AAC (nothing is re-encoded), or converts it to `videos.audio`.
3. Reads its output line by line (`progress::parse`). The app asks for three marked lines: the video's details as JSON before the download, a progress line per chunk, and the final path. Picture and sound arrive as separate files; `Tally` adds them into one share that never runs backwards.
4. Checks that a video has a picture this computer can show, converting it if not.
5. Moves the finished file into Downloads under a name nothing else has (`Me at the zoo (1).mp4`) and deletes the staging folder.

A download that fails is tried again the ways that are worth trying (`attempts`): signed in as the browser when the site asked for that, once more after a pause when the connection wobbled, and once more with a newer yt-dlp if one is out. What the notch says about a failure (`Failure::message`) names the site, never the tool; yt-dlp's own complaint goes to stderr.

The app reads `Videos::downloads()` whenever `version()` moves. An ended download stays in the list for a moment (2.6 seconds saved, 3.4 failed) so the notch can show how it ended, and `take_endings()` hands each ending over once: a saved file is added to the Shelf, a failure is shown as a notch message.

### A file that always plays (`webvideo/codecs.rs`)

Sites send the same video in several codecs, and the biggest picture is often in one the computer's own player cannot show: QuickTime plays a VP9 file's sound over a black frame. So the format choice only asks for codecs that play here (`codecs::native`): H.264 everywhere, HEVC on a Mac, and AV1 on a Mac whose chip decodes it (`VTIsHardwareDecodeSupported`), which is what makes 4K and 8K from YouTube possible there. Among those it takes the highest resolution, then the highest frame rate, and at the same size prefers H.264 because it plays on everything else too. On a computer without AV1, YouTube's H.264 stops at 1080p, so that is what it gets.

A format whose codec the site doesn't state is taken on trust. Once the file is here, `codecs::make_playable` reads its real codec with ffmpeg, and if it is one that doesn't play (which also covers a site that offers nothing else), re-encodes it once: HEVC through the graphics chip on a Mac, H.264 elsewhere.

### What it will not download

- **Copy-protected streams.** When yt-dlp reports a format as DRM protected the download ends with "That video is copy-protected and can't be downloaded". Nothing tries to get around it.
- **Live and upcoming videos**, which have no end to wait for.
- **A page with no video**, which ends with "There's no video on that page".

### Signing in

Some sites refuse a stranger: Instagram for most reels, Vimeo for its own pages, Facebook and X for much of what is on them. When a download fails that way and `videos.sign_in` is on, it is tried once more with yt-dlp's `--cookies-from-browser`, naming the browser the tab came from: Safari, Chrome, Chromium, Brave, Edge, Opera, Vivaldi or Firefox, and for the Firefox forks (Zen, LibreWolf, Floorp, Waterfox) their own profile folder. The cookies are read on this computer by yt-dlp and go nowhere but the site they belong to. On a Mac, reading a Chromium browser's cookies makes macOS ask for the login keychain, and reading Safari's needs Full Disk Access; a browser nobody has heard of has no known cookie store and is tried as a stranger only. YouTube is never signed in to, because it answers borrowed sign-ins by restricting the account. If you are not signed in to the site in that browser either, the download ends with "… wants you signed in for this one".

## In the notch (`notch/live/video.rs`, `notch/drop_zone/mod.rs`, `notch/drop_zone/layouts.rs`)

- **The drop target.** `Notch::set_drop_video(site)` puts the notch in `Drop` mode with two tiles, a film strip over "Video" and a waveform over "Audio"; the one under the pointer is lit in the site's colour (garnet for YouTube, honey for Instagram, pearl for the rest) and ticks under the finger as the pointer crosses over. Beside the layout picker it adds two download cells instead.
- **The live activity** (`Activity::Video`). A ring fills in the site's colour with a play mark inside (a note for audio) and the percentage beside it; while the page is being looked up the ring is a light chasing its tail, and at the end it turns jade with a tick that pops in. `Reel` eases each download's share with a spring so progress glides instead of ticking.
- **First in line.** Downloads lead every other live activity; only the padlock comes before them (`Notch::combine_activities`, `app/updates.rs` `in_order`). A song that starts or changes doesn't take the middle from a download, and a playing song's dance holds the bottom edge still while the download's light runs along it. The wings are sized for the widest word a download can show (`video::trailing_width`), so the notch keeps one width from Finding to Saved.
- **The light under the notch.** While a download leads the notch, its progress also runs as a thin glowing line just outside the body's lower edge, left to right, with a sheen travelling along it and a bright head. It is drawn below the body so the whole of it shows on a MacBook, where the camera housing leaves no room for more than a ring and a number. When the download leaves the notch, its line fades out with the rest of it instead of going out on the frame it ends.
- **As an ear.** A second or third download, or one on the lock screen, is a ring in a side bubble. Hovering it shows the video's name, the percentage, the speed and time left, and a × that cancels it (`Action::CancelVideo`). Clicking it opens the Shelf.
- **In the open panel** (`notch/live/downloading.rs`). Each download under way gets a row at the bottom of the open panel, under whatever page is shown: its cover, title, a progress bar in the site's colour, a status line and a × (a tick once saved). Rows grow and fold with `ui::rows`; the panel grows in step with them (the room is added straight onto the height spring), so the page above never moves. The cover is the page's thumbnail: when yt-dlp reports it, `webvideo/cover.rs` fetches it once over HTTPS on its own thread, cuts it to a 120-pixel square and hands it on with the download (`Download::cover`, `VideoLive::cover`).

## Settings

`videos.enabled`, `videos.quality`, `videos.audio` and `videos.sign_in` in `config.json`; see [Configuration](Configuration.md#videos).

## Tests

- `webvideo/site.rs`, `progress.rs`, `job.rs` and `engine.rs` are tested with real addresses, the lines yt-dlp really prints, its real complaints and real checksum files. `webvideo/mod.rs` tests the bookkeeping around a download.
- `webvideo/browser/mod.rs` tests the cookie stores and the registry command lines.
- `app/videos.rs` tests what a drag offers and how a download is shown; `notch/live/video.rs` and `notch/drop_zone/mod.rs` test the rim, the easing, the steady width and the drop target with and without the layout picker; `notch/live/mod.rs` tests that a download leads; `webvideo/cover.rs` tests the square cut (and, ignored, a real YouTube thumbnail).
- Ignored tests do the real thing: `webvideo::engine::tests::real_engine_installs_and_runs` installs the engine, `webvideo::codecs::tests::unplayable_video_is_converted` encodes a second of VP9 and converts it, and `webvideo::tests::real_video_downloads_to_folder` downloads a 19 second video from YouTube. That last one tries any other page with `NOTCHBASE_VIDEO_URL`, keeps only the sound with `NOTCHBASE_VIDEO_KIND=audio`, signs in as a browser with `NOTCHBASE_VIDEO_COOKIES`, and leaves the file behind with `NOTCHBASE_VIDEO_KEEP`, which makes it the quickest way to find out why a site fails. Run them with `NOTCHBASE_DATA_DIR` pointing at a scratch folder so the engine is not installed into your own.
- Previews: `video-drop`, `video-layouts`, `video-finding`, `video`, `video-saved` and `video-sides` (see [Previews](Previews.md)).
- The Windows side (UI Automation, the registry, the OLE drop target) is type-checked and linted from the Mac but has not been run on Windows.
