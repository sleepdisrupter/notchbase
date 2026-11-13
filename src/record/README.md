# record

Screen recording: a display or part of one to a video file, with the microphone and desktop audio if asked.

- `recorder.rs` is what the app holds. It picks the file name, starts a session and reports when the file is saved. Dropping it waits for the recording, so quitting in the middle still leaves a file that plays.
- `job.rs`, `options.rs` and `region.rs` are what a session is asked to record and what it hands back.
- `naming.rs` makes names like `Screen Recording 2026-09-28 at 14.03.27.mp4`. Screenshots use it too.
- `macos/` uses ScreenCaptureKit, which captures, encodes and writes the movie itself.
- `windows/` is a pipeline of our own: desktop duplication (or GDI when that can't start), a BGRA to NV12 converter, WASAPI audio, a mixer and a Media Foundation H.264 writer.

The Record Screen tool in `tools/screen-recorder` is what starts it. See [wiki/Screen-Recording.md](../../wiki/Screen-Recording.md).

Run the tests with `cargo test record::`. The tests that record the real screen are ignored: `cargo test record::windows -- --ignored`.
