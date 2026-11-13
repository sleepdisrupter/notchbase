# capture

Screen capture: the color sampler, the area pickers for screenshots and recordings, and the dim around a recording.

- `color/` is the color sampler. macOS shows the system's own, and on Windows we draw a loupe around the cursor.
- `region/` is the area picker drawn over every display. On macOS a wrapper takes the screenshot with `screencapture`.
- `dim.rs` shades everything outside the area being recorded.
- `overlay/` has what the overlays share: fades, frame timing and fitting a window to a monitor.
- `screenshot.rs` is a captured image and the thumbnail the notch shows.

The app's side, asking for an area and handing the result to the tool that wanted it, is in `src/app/capture`. See [wiki/Screen-Capture.md](../../wiki/Screen-Capture.md).

Run the tests with `cargo test capture::`.
