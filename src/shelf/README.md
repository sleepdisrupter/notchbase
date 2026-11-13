# shelf

The Shelf, where files dropped on the notch wait to be dragged out, shared or worked on. It can convert and compress images, video and audio, combine images and PDFs, and split a PDF into pages, always writing new files beside the originals.

- `model.rs` is the `Shelf` the notch shows. It keeps the list in `shelf.json` (through `store.rs`) and hands work to the worker.
- `worker/` is the shelf's thread. It reads each new item's size, pages, length and thumbnail, and runs one operation at a time.
- `ops/` runs an operation on the selected items, and `available.rs` decides which operations a selection gets.
- `images/`, `png_opt/`, `pdf/`, `audio/` and `video/` do the work for each kind of file. Video goes through AVFoundation on a Mac and Media Foundation on Windows.
- `airdrop/` sends with the system's AirDrop window, on a Mac.
- `item.rs`, `kind.rs`, `format.rs`, `task.rs` and `notice.rs` are the items, their kinds, the output formats, progress, and the message shown afterwards.

The page is in `src/pages/shelf`, the app's side in `src/app/shelf`, and dragging files in and out in `src/filedrag`. See [wiki/Shelf.md](../../wiki/Shelf.md).

Run the tests with `cargo test shelf::`.
