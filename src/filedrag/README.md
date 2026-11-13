# filedrag

Files going in and out of the notch: dragging Shelf files out to other apps, taking drops from anywhere, thumbnails and file icons, and showing a file in Finder or Explorer.

- `macos/` drags with `NSDraggingSession`, takes drops through the window delegate and makes thumbnails with Quick Look.
- `windows/` drags with `SHDoDragDrop`, takes drops through our own `IDropTarget` and asks the shell's image factory for pictures.
- `incoming.rs` queues what drop sites hand over, and posting one wakes the app.
- `links.rs` reads links out of a drag, and `save.rs` turns a drop that isn't a file yet, like a link or a picture from a page, into a file in Downloads.
- `other.rs` stands in on other systems, where none of this exists.

The Shelf itself is in `src/shelf`. See [wiki/Shelf.md](../../wiki/Shelf.md).

Run the tests with `cargo test filedrag::`. The `examples/drag_out` program tries a real drag by hand.
