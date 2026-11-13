# snap

Window layouts: drag a window to the notch and drop it on a layout.

- `drag.rs` tells a dragged window from one being resized.
- `zones.rs` works out where windows go: each layout's zones, the biggest free space and a tiling of every window.
- `ghost.rs` is the overlay that shows where the window will land.
- `habits.rs` learns which layout each app usually gets, kept in `snap-habits.json`.
- `macos.rs` lists windows with Core Graphics and moves them with the accessibility API, which needs the Accessibility permission.
- `windows.rs` works in physical pixels with the frames DWM reports, and moves windows with `SetWindowPos`.

The app's side, and the layout picker in the notch, are in `src/app/snap.rs` and `src/notch/drop_zone/layouts.rs`. The settings are under [snap in wiki/Configuration.md](../../wiki/Configuration.md#snap).

Run the tests with `cargo test snap::`.
