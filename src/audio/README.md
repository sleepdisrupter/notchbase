# audio

The sound mixer: volume and mute for each app, the output and input levels, and picking the default devices.

- `mixer.rs` is the `Mixer` the app holds. Changes show at once, so a slider never snaps back.
- `driver/` decides when the platform side runs: only while a page needs it.
- `windows/` works through WASAPI audio sessions on the default output.
- `macos/` gives each app below full volume a Core Audio process tap and plays its audio back at the chosen gain.
- `core_audio/` is the part of the Mac code that needs no Mac, so its tests run everywhere.
- `grouping.rs`, `sorting.rs`, `activity.rs`, `labels.rs` and `keys.rs` turn sessions and processes into the rows the page shows.

The page itself is in `src/pages/mixer`. See [wiki/Sound-Mixer.md](../../wiki/Sound-Mixer.md).

Run the tests with `cargo test audio::`.
