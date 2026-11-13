# annotate

The annotate window: draw and write on a screenshot, and the picture is copied to the clipboard as it changes. It opens from the Annotate button on a Screenshot notice.

- `window.rs` opens the window centred on its monitor and passes events to the editor.
- `editor/` fills the window each frame: input, the toolbar, and the line or words in progress.
- `sheet/` holds the screenshot and its marks, and redraws only the area a change touched.
- `mark.rs`, `stroke/` and `lettering/` are the marks: lines, and blocks of words.
- `raster/` inks marks into pixels, and `composite.rs` inks them onto a copy for the clipboard.
- `copier/` does the copying on its own thread.

The app's side, opening, replacing and closing the window, is in `src/app/annotate`. [wiki/Annotate.md](../../wiki/Annotate.md) has the whole story, including how strokes and words become pixels.

Run the tests with `cargo test annotate::`. A few are ignored: the snapshots that write PNGs, the one that uses the real clipboard, and some timings.
