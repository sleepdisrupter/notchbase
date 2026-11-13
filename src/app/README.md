# app

The running app. `App` (in `mod.rs`) owns every service and every notch window. winit hands it events, it builds the `PanelData` the notch draws each frame, and it carries out the `Action`s the notch sends back.

Each file adds one `impl App` block for its area, so it's easy to find where something is handled:

- `handler.rs` is winit's event handler, and `frame.rs` is one pass of the event loop.
- `actions.rs` carries out each `Action`.
- `events.rs` has the events other threads send, and the wake that sends them.
- `panel.rs` builds the `PanelData`.
- `config.rs` follows `config.json` and applies what changed.
- The rest are one area each: `hotkeys.rs`, `clipboard.rs`, `now_playing.rs`, `updates.rs`, `videos.rs`, `snap.rs`, `welcome.rs`, `whats_new.rs` and so on.

The bigger features have folders: `notches/` (one window per display), `tools/` (loading tools, feeding them, their requests and shortcuts), `find/`, `shelf/`, `capture/`, `annotate/`, `assistant/` and `system/`.

Slow work stays off this thread. Services run on threads of their own and wake the loop when they have something new. [wiki/Architecture.md](../../wiki/Architecture.md#the-app-srcapp) walks through start-up, a frame and the data flow.
