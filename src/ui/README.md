# ui

The pieces the notch and its pages are drawn with. The basics, like the canvas, colors, glass and springs, come from `crates/paint` and are re-exported here, so the app writes `ui::glass` and `ui::spring`.

- `gl_window/` is the OpenGL window egui paints into.
- `element/` paints the UI that tools describe with the SDK, and sends what the user does back to the tool.
- `field/` is single-line text fields.
- The rest are small parts used across pages: `segmented.rs`, `menu.rs`, `rows.rs` (rows that grow in and fold away), `swipe.rs`, `handover.rs`, `cover.rs`, `picture.rs`, `sheen.rs`, `trace.rs`, the brand marks and the lock mark.
- `motion.rs` has the spring settings shared across the UI.

Draw with these and not with egui's own widgets, so everything looks the same. See [wiki/Rendering-and-Motion.md](../../wiki/Rendering-and-Motion.md).

Run the tests with `cargo test ui::`.
