# media

What's playing, how loud each band of the music is right now, and the controls that go back to the player.

- `watcher.rs` is the service the app holds. One thread follows the player and carries controls to it, another listens to the output.
- `follower.rs` decides what to show from noisy player reports, so a short pause or a dropped report doesn't make the song flicker.
- `listener.rs` captures the output while a notch dances and publishes six bands. When it can't, `groove.rs` plays a made-up beat instead.
- `macos/` asks Music and Spotify directly, since there's no public API for other apps' songs.
- `windows/` reads the System Media Transport Controls and captures the output with WASAPI loopback.
- `track.rs` is a song with its play state and timeline, and `artwork.rs` crops and shrinks its cover.

The spectrum math is in `crates/spectrum`. See [wiki/Media-and-Music.md](../../wiki/Media-and-Music.md).

Run the tests with `cargo test media::`.
