# webvideo

Video downloads from a dragged browser tab or link. Each download runs yt-dlp on its own `notchbase-video` thread, and the finished file moves into Downloads under a name nothing else has.

- `site.rs` recognizes video pages, with or without the scheme an address bar hides.
- `browser/` reads the addresses the browser in front is showing, and where its cookies are.
- `engine.rs` keeps yt-dlp, Deno and ffmpeg in the data folder, fetched and checked against their SHA-256 before they're used.
- `job.rs` runs yt-dlp once, and `progress.rs` reads its output.
- `codecs.rs` asks only for codecs the system can play, and converts a video that came in another.
- `cover.rs` fetches the page's thumbnail for the cover beside a download.

The app's side is in `src/app/videos.rs`, and the notch's in `src/notch/live/video.rs` and `downloading.rs`. See [wiki/Video-Downloads.md](../../wiki/Video-Downloads.md).

Run the tests with `cargo test webvideo::`. The ignored ones do the real thing, installing the engine and downloading a short video.
