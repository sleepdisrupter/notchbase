# Shelf

The Shelf is a place to park files. Drag files onto the notch to add them, drag them back out into any app, and run quick operations on them: convert and compress images, combine images and PDFs into one PDF, split a PDF into pages, convert and compress video, convert audio. Tools can put files on the shelf too (a `Shelve` request): the Record Screen tool shelves every recording it saves.

The model and operations live in `src/shelf`; dragging files out, thumbnails and "reveal in Finder / Explorer" live in `src/filedrag`; the page is described in [Pages](Pages.md#shelf-pagesshelf).

## The model (`shelf/model.rs`)

`Shelf` owns a list of `Item`s, a worker, and the last result notice.

- **`Item`** (`shelf/item.rs`): an id, the absolute path, the display name, the `Kind`, the uppercase extension, and facts the worker fills in later: size in bytes, pixel dimensions, PDF page count, duration in seconds, a thumbnail and a thumbnail revision number. `Origin` records whether it was dropped, restored from disk, or produced by an operation.
- **`Kind`** (`shelf/kind.rs`) is decided from the extension: `Image`, `Video`, `Audio`, `Pdf`, `Folder` or `Other`.
- **Adding** (from a drop, a tool's `Shelve` request, or `NOTCHBASE_SHELF_FILES` in a preview) resolves each path to an absolute path, skips paths that do not exist, and skips duplicates. Duplicates are detected with a **path key** (`shelf/path_key.rs`): the canonical path, with Windows verbatim prefixes removed, lowercased on Windows and macOS where file names are case-insensitive. The app shows "Already on the Shelf" or "Some were already on the Shelf" when a drop added fewer files than it carried.
- **Persistence** (`shelf/store.rs`): the list of paths is saved to `shelf.json` in the data directory after every change, written to a temporary file and renamed into place. On startup the saved paths are restored; any that no longer exist are dropped and the file is rewritten.
- **Pruning**: while the Shelf page is visible, the app checks about once a second for files that were deleted or moved and removes them. It also prunes after a drag-out, because dragging to the Trash or moving a file removes it from its original location.

`shelf::available_ops(items)` (`shelf/available.rs`) decides which operations make sense for a set of items:

| Items | Operations |
| --- | --- |
| only videos | Convert (to MP4, MOV, GIF or M4A), Compress |
| only audio | Convert (to M4A, MP3, WAV or FLAC) |
| images and/or PDFs | Convert (only when every item is an image: to PNG, JPEG, WebP, TIFF, BMP, GIF, ICO or PDF), Compress (if every item is compressible), Combine (two or more items), Split (a single PDF with more than one page) |
| anything else | none |

The Convert button opens a row of target formats. For video and audio, only formats the platform can actually write are offered (`Clip::writable`).

## Sending with AirDrop (Mac)

On a Mac the Shelf page's action row ends with an **AirDrop** button whenever the shelf has files. It sends the selection, or every file when nothing is selected, folders included: the tiles lift off towards the notch, the files stay on your shelf, the notch closes and the system's AirDrop window opens with them (`shelf/airdrop`, `app/shelf/airdrop.rs`). When the window closes, the Shelf shows "AirDropped photo.png" or "Couldn't AirDrop photo.png"; closing it without sending says nothing. Windows has no AirDrop, so the Shelf there has no send button. See [Pages](Pages.md#shelf-pagesshelf).

## The worker (`shelf/worker`)

Anything slow runs on one background thread, `notchbase-shelf`, spawned on first use and respawned if it ever dies.

**Jobs** arrive over a channel and are sorted into three queues with a fixed priority (`worker/queues.rs`):

1. **operations** the user started,
2. **info probes** for new items (size, dimensions, page count, duration),
3. **thumbnails**, queued after each item's info.

So a Compress starts right away even while fifty freshly dropped files are still being thumbnailed. Probes are skipped for items that were removed meanwhile.

**Probing** (`worker/probe.rs`):

- images: dimensions from the decoder, honouring EXIF orientation;
- PDFs: the number of distinct pages;
- videos: dimensions and duration from the platform media framework;
- audio: duration from the decoder, falling back to the video probe;
- thumbnails: the OS thumbnailer first (`filedrag::thumbnail`: Quick Look on macOS, the shell image factory on Windows), falling back to decoding the image ourselves, at 128 px.

**Operations** (`worker/thread.rs`) run one at a time. `Shelf::run` refuses to start a second one while one is busy. Progress is reported through a `Task` (`shelf/task.rs`) that maps each file's 0..1 progress into its share of the whole, and checks a shared cancel flag. Each operation runs inside `catch_unwind`, so a malformed file produces "Couldn't compress these files" rather than a crash.

**Handing results back.** Finished probes, thumbnails and operation outcomes are pushed into a shared list and the worker calls the app's `Wake` closure. On the main thread `Shelf::sync` applies them: updating items, adding produced files to the shelf (as `Origin::Produced`), and storing the notice. The app shows the notice on the Shelf page for `notices.shelf_ms` (4 seconds by default), and also as a notch message if the Shelf page is not visible. See [Architecture](Architecture.md#waking-the-event-loop).

## Where outputs go (`ops/outputs.rs`, `ops/unique.rs`)

Outputs are written **beside the source file**. If that folder is not writable, they go to a `Shelf` folder in the data directory instead.

Names never overwrite anything. `claim_free_name` tries `name.ext`, then `name 2.ext`, `name 3.ext` and so on, creating each candidate with "create new" semantics so two writers can never take the same name. Long-running outputs (video and audio) are **staged**: they are written to a hidden temporary file (`.notchbase-<pid>-<n>.<ext>`) beside the target and only moved to their final free name when complete, using a hard link (or a rename) that cannot replace an existing file. A cancelled or failed operation leaves nothing behind.

Operations that process several files report a combined notice (`ops/tally.rs`): "Saved photo.png", "Saved 4 files", "photo compressed.jpg · 38% smaller", "Compressed 3 files · 41% smaller", "Already compact", with " · 1 failed" appended when some files failed, and "Cancelled · 2 files saved" when cancelled partway. Files already written before a cancel are kept and added to the shelf.

## Images (`shelf/images`, `shelf/png_opt`)

**Decoding** (`images/decode.rs`) uses the `image` crate for PNG, JPEG, WebP, TIFF, BMP, GIF and ICO. It applies EXIF orientation so results are upright, keeps the embedded ICC color profile, and refuses images that would decode to more than 512 MB. On macOS, HEIC, HEIF and AVIF are decoded by running the system's `/usr/bin/sips` to convert to PNG first (`images/sips_macos.rs`); on Windows those formats are not treated as images.

**Convert** (`images/transform.rs`, `images/encode.rs`) re-encodes to the chosen format, carrying the ICC profile over when the target format supports it and the profile matches the image's color model (`shelf/icc.rs`). Format details: JPEG at quality 90, flattened onto white if the image has transparency; PNG with the standard encoder and adaptive filtering (Compress runs the optimizer below); lossless WebP; ICO fitted within 256 px; BMP and GIF as 8-bit. Converting an image to PDF produces a one-page PDF, built the same way as Combine.

**Compress** depends on the source:

| Source | Result |
| --- | --- |
| JPEG | re-encoded at quality 78 |
| PNG | losslessly optimized with `png_opt` |
| BMP, TIFF | converted to an optimized PNG (lossless) |
| HEIC, HEIF, AVIF (macOS) | converted to JPEG at quality 78 |

A result is only kept if it is at least 3% smaller than the original; otherwise the file counts as "Already compact".

**The PNG optimizer** (`shelf/png_opt`) is a small lossless optimizer written for the shelf:

1. Decode the PNG, keeping the rendering chunks that affect appearance (sRGB intent, ICC profile, gamma, chromaticities, physical pixel size, EXIF). Animated PNGs are refused. An ICC profile that does not match the color model is dropped.
2. **Reduce the pixels losslessly** (`png_opt/pixels.rs`): drop the low byte of 16-bit samples when it is redundant, drop an alpha channel that is fully opaque, and collapse RGB to gray when all channels are equal (not when an ICC profile says the image is color).
3. Build candidate **layouts** (`png_opt/layout.rs`): the plain layout at the smallest bit depth (gray images can go down to 1, 2 or 4 bits), and a **palette** layout when there are at most 256 distinct colors, with translucent palette entries ordered first so the `tRNS` chunk stays short, packed at 1, 2, 4 or 8 bits.
4. Encode every layout with the **filters worth trying** at the best deflate level (`png_opt/write.rs`): palette and sub-8-bit layouts try no filtering and adaptive filtering, other layouts try adaptive and minimum-entropy filtering. Above 4 megapixels only the first strategy is tried, to keep large images fast.
5. Keep the smallest result.

## PDFs (`shelf/pdf`)

PDF handling is built on `lopdf`:

- **Loading** (`pdf/mod.rs`) limits decompressed structure streams to 64 MB, refuses password-protected files ("… is password-protected"), and nulls out dangling references so damaged PDFs still open.
- **Combine** (`pdf/combine.rs`) builds a new PDF 1.7 document. Each PDF's pages are moved in with their objects renumbered and inherited attributes flattened onto each page; each image becomes a page sized at 0.75 points per pixel (capped at 14,400 points), with baseline and progressive JPEGs embedded as-is without re-encoding (`pdf/jpeg.rs`) and other images embedded losslessly, with their ICC profile and orientation respected (`pdf/image_page.rs`). The output is named after the image when combining a single image, otherwise "Combined".
- **Split** (`pdf/split.rs`) writes each page to its own PDF, copying only the objects that page can reach, into a new folder named "<name> pages".
- **Compress** (`pdf/compress.rs`) re-deflates every plain Flate stream at the best level and keeps the new version only if it is smaller.
- **Saving** (`pdf/save.rs`) keeps only the trailer keys that stay valid, prunes unreachable objects, deflates streams that had no filter, and writes with object streams and cross-reference streams.

`shelf/tests/damaged_pdfs.rs` feeds deliberately broken PDFs through every operation.

## Audio (`shelf/audio`)

Audio is decoded with `symphonia` (MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF, CAF, Ogg Vorbis and more). Converting to **WAV** writes PCM with `hound`; converting to **FLAC** encodes with `flacenc`. Both stream samples through a pump that reports progress and checks for cancel. **M4A** and **MP3** are produced by the platform's media framework (below); if the system cannot read the source directly, it is first decoded to a temporary WAV and then transcoded.

## Video (`shelf/video`)

Video uses each OS's media framework, behind one interface: `probe`, `can_write(container)`, `transcode(source, target, container, quality, task)` and `frames(source, spec, task, each)`.

- **Windows** (`video/windows`): Media Foundation. A source reader decodes, a sink writer encodes to MP4 or MOV with H.264 (High profile) video and AAC audio, or to M4A (AAC) or MP3 audio-only. The plan (`plan.rs`, `settings.rs`) picks frame size, frame rate and bitrate: `Original` quality targets about 0.2 bits per pixel per frame (1 to 80 Mbps); `Compact` scales down to fit 1920 px and targets about 0.07 bits per pixel per frame (0.25 to 12 Mbps). Rotation metadata is honoured.
- **macOS** (`video/macos`): AVFoundation. `AVAssetExportSession` presets do the transcoding: highest quality for `Original`, HEVC 1080p, then H.264 1080p, then medium quality for `Compact`, and Apple M4A for audio. There is no MP3 writer on macOS, so MP3 is not offered there. `AVAssetImageGenerator` extracts frames.

**GIF** export extracts frames at up to 12 fps, fitted within 480 px, for at most 30 seconds, and encodes them with the `image` crate's GIF encoder, looping forever.

Video **Compress** transcodes to MP4 at `Compact` quality and keeps the result only if it is at least 3% smaller.

## Dragging things in

Anything you can drag onto the notch lands on the shelf: files from Finder or Explorer, and also images (or any file) dragged out of a web browser, a chat app, a mail attachment or an archive. Things that aren't files yet are saved to your **Downloads** folder first, named the way a browser would name the download ("images.jpeg", "cat (1).png"), and the saved file is what goes on the shelf.

It works with every major browser, because notchbase reads all the ways they hand over a dragged image:

| What the drag carries | Who sends it | What notchbase does |
| --- | --- | --- |
| Real files (`CF_HDROP`, file URLs) | Finder, Explorer, most apps | Shelves them where they are |
| Real files in the temporary folder | Firefox on Windows, 7-Zip, screenshot previews | Copies them into Downloads the moment they are dropped, since the sender deletes them soon after |
| Virtual files (`FileGroupDescriptorW` + `FileContents`) | Chrome, Edge, Brave, Opera, Vivaldi, Firefox, Outlook, zip folders in Explorer | Writes the bytes into Downloads under the name the sender gives |
| File promises (`NSFilePromiseReceiver`) | Safari, Chrome and Firefox on macOS, Mail, Photos | Asks the sender to write the file straight into Downloads |
| Raw image data (PNG, JPEG, GIF, WebP, HEIC, TIFF) | Safari and some apps on macOS | Saves it into Downloads; TIFF is converted to PNG |
| A link to an image (`<img src>` in dragged HTML, a URL) | Anything else | Downloads it into Downloads |

The last row is the fallback:

- **Downloads.** Links are fetched over `http`/`https` with a 30 second timeout and a 512 MB limit. `data:` links are decoded in place, and `file:` links are treated as files.
- **Search results.** Google and Bing result links (`imgres?imgurl=…`, `mediaurl=…`) are unwrapped to the image itself.
- **Web pages are refused.** If a link turns out to be a web page, not a file, the notch says so instead of saving HTML.
- **Pages are searched for a video.** A dragged tab or address, and a link or thumbnail of a known video page (the app names those with `filedrag::prefer_pages`), is not refused as a page or saved as a picture: the drop target reports it with `Arrival::Offer`, drops it as `Dropped::Page`, and the video on it is fetched. An address that plainly names a file is still saved as that file. See [Video downloads](Video-Downloads.md).
- **File types.** A file's extension is fixed from its first bytes when the sender got it wrong (`photo.php` holding PNG data becomes `photo.png`). A missing extension is added from the bytes or the `Content-Type`.

Hovering turns the notch into its Drop mode (see [Notch](Notch.md#drop-zone)) and dropping opens the Shelf page. On macOS the app additionally watches the system drag pasteboard (`filedrag/macos/sensor.rs`) while the mouse button is down, so the notch can react to a file drag that has not reached its window yet; Windows senses only that the primary button is held.

### How drops are received (`filedrag/incoming.rs`, `save.rs`, `links.rs`)

Each notch window gets a **drop site** from `filedrag::accept_drops`, which posts `Arrival`s (`Hover(count)`, `Leave`, `Drop(items)`, `Saved`) into the app's `Inbox` and wakes the event loop. A dropped item is a `Dropped::File`, `Dropped::Data { name, bytes }`, `Dropped::Link` or `Dropped::Page` (a link dragged as a page rather than as the address of a picture or file). Real files outside the temporary folder go on the shelf at once; everything else is saved by `filedrag::save` on a `notchbase-drop` thread, which posts `Saved { paths, problems }` back, and the paths are shelved like any other drop. The inbox also carries the Downloads folder, so the platform code never has to look it up.

- **Windows** (`filedrag/windows/drop_target.rs`, `data_object.rs`): the notch window is created with winit's drag and drop turned off, and notchbase registers its own `IDropTarget` with `RegisterDragDrop` (after `OleInitialize` on the UI thread). It answers `DROPEFFECT_COPY` (or `LINK` when that's all the source allows) and never `MOVE`, so a source can't delete what it dragged. It passes the drag image through `IDropTargetHelper` so browsers' drag previews stay visible over the notch. On drop it reads, in order, `CF_HDROP`, then virtual files (as `IStream` or `HGLOBAL`), then a link from `HTML Format`, `UniformResourceLocatorW`, `text/x-moz-url`, `UniformResourceLocator` or plain text that is a single link. The site revokes the registration when its window closes.
- **macOS** (`filedrag/macos/drop_target.rs`, `pasteboard.rs`): winit makes the window's delegate the drag destination, so notchbase replaces the delegate's `draggingEntered:`, `draggingUpdated:`, `draggingExited:` and `performDragOperation:` with its own (once, with `class_replaceMethod`) and registers the window for file URLs, URLs, strings, image types and `NSFilePromiseReceiver.readableDraggedTypes`. Drops read file URLs first (resolving file reference URLs), then file promises (received into Downloads on an operation queue, each file posting `Saved` when it lands), then image data, then a link. The window also registers for `org.mozilla.MozillaWildcard`, the only type on a tab dragged out of Firefox or Zen, so it can be asked about such a drag at all; it takes one only while `filedrag::catch_everything` is on (see [Video downloads](Video-Downloads.md#how-a-tab-is-caught)).

## Dragging files out (`src/filedrag`)

Dragging a shelf tile starts a real OS drag with the file (or all selected files), so it can be dropped into Finder, Explorer, a browser, a chat app or an editor. The drag always offers a copy.

- **Windows** (`filedrag/windows`): the paths are turned into shell item ID lists, bound to a shell data object (`SHCreateShellItemArrayFromIDLists` + `BHID_DataObject`) and dragged with `SHDoDragDrop`, which runs its own modal loop until the drop. If the shell did not provide a drag image, the file's thumbnail is attached through `IDragSourceHelper`. OLE is initialised for the duration. During the drag the notch window's hit testing is suspended so the notch itself is not a drop target, and afterwards the app releases egui's pointer button, since the window never saw the mouse come up.
- **macOS** (`filedrag/macos`): an `NSDraggingSession` is started from the notch's content view with one dragging item per file (file URL pasteboard items with the file's icon), using the current mouse event or a synthesized one. The session animates back if the drop is cancelled.

`filedrag::thumbnail` uses Quick Look (`QLThumbnailGenerator`, with a 2 second timeout, falling back to the Finder icon) on macOS and `IShellItemImageFactory` on Windows, capped at 1024 px. `filedrag::reveal` selects the file in a Finder window (`activateFileViewerSelectingURLs`) or an Explorer window (`SHOpenFolderAndSelectItems`, falling back to `explorer.exe /select,`).

`examples/drag_out` is a standalone harness for this code: it creates two temporary files and lets you drag them out of a plain window while printing every drag event, and on Windows it has a `--self-test` that scripts a drag onto a notch-styled window. See [Building and Testing](Building-and-Testing.md).

## Tests

The shelf has the largest test suite in the app: `shelf/tests` (model, adding, errors, background work, convert, compress, combine, split, audio, damaged PDFs), `shelf/ops/tests.rs` (naming and staging), `shelf/png_opt/tests.rs` (every reduction and layout, round-tripped through a decoder), and `shelf/video/windows/tests` (probing, frames, orientation, transcoding and cancel with generated fixtures). Tests use temporary folders and never touch the user's real shelf.
