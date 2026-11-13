# notch

The notch itself. A `Notch` (in `mod.rs`) holds everything one notch window shows, from the closed shape to the open panel. The app ticks it with the time and the pointer, pushes events into it, and calls `ui` to draw a frame, which returns the `Action`s that were pressed.

- `geometry.rs`, `frame.rs` and `keyline.rs` are the shape: its sizes, its body and glass, and the rim on a floating island.
- `pointer.rs` and `zones.rs` are hover, peek and open, and where the notch listens.
- `moves/` is how it moves: leaning, fleeing, dodging menus, hugging the top and the springs between modes.
- `activity/`, `notice/` and `live/` are what the closed notch shows: short messages, tool notices, and live activities.
- `music/`, `ask/` and `drop_zone/` are the song, assistant questions and the drop well.
- `panel/` is the open panel: the header, the page under it and switching between pages.
- `state.rs` is the notch's state as tools see it.
- `preview/` has the sample states for `NOTCHBASE_PREVIEW`, and `tests/` drives whole notches through their frames.

The notch never owns a service. It reads `PanelData` and answers with `Action`s, both in `src/bridge`. [wiki/Notch.md](../../wiki/Notch.md) covers every mode and motion, and [wiki/Previews.md](../../wiki/Previews.md) how to look at any state without a real window.

Run the tests with `cargo test notch::`. They run full frames without a window.
