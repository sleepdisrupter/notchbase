# Media and music

`src/media` answers two questions for the rest of the app: **what is playing** (title, artist, artwork, play state, position) and **how loud is each frequency band right now**. The first drives the player at the top of the Media page (the Output slider sits under it, with the rest of the [mixer](Sound-Mixer.md) behind the chevron at the end of that row) and the song shown in the notch; the second makes the notch dance to the music. It also carries media controls (play/pause, next, previous, seek) back to the player.

## The `Media` service

`media::Media` (`media/watcher.rs`) is started once by the app and runs two background threads:

- **`notchbase-media`** follows the player. It polls the platform source, runs the follower rules (below), fetches artwork, and executes media controls sent from the UI.
- **`notchbase-levels`** listens to the system output while something is playing and publishes six spectrum bands.

The app reads the latest state every frame on the main thread:

| Method | Returns |
| --- | --- |
| `now_playing()` | `Option<NowPlaying>`: title, artist, artwork (a 96 x 96 `ColorImage`), `playing`, and an optional `Timeline` |
| `levels()` | `Bands`: six values in 0..1 |
| `has_player()` | whether any player session exists (used to decide whether to claim the media keys) |
| `control(Control)` | sends `PlayPause`, `Next`, `Previous` or `Seek(seconds)` to the player |

In `src/app` (`app/now_playing.rs`), the now-playing sync passes both to every notch each frame (`Notch::set_now_playing`, `Notch::set_music_levels`). Tools that subscribe to media receive the song (title, artist, playing, position and duration) whenever it changes (`app/tools/feeds.rs`). Media controls from the Media page, the notch, a tool's `Media` request or the media keys go through one path (`control_media`) that first lets the notch anticipate the change and then sends it to the player.

### Polling

The follower thread polls every `media.poll_ms` (800 ms by default, at least 100 ms; see [Configuration](Configuration.md)). For 1.5 seconds after a control is sent it polls every 120 ms instead, so the UI confirms a skip or pause quickly.

### Timelines

`Timeline { position, duration, at }` records the position at a moment. `position_at(now, playing)` extrapolates while playing, so the scrubber and the notch's remaining-time counter move smoothly between polls. Two timelines "agree" if their durations match and their extrapolated positions are within 1.5 seconds, which keeps small reporting jitter from counting as a change.

### Follower rules (`media/follower.rs`)

Raw player reports are noisy, so the `Follower` decides what the UI actually shows:

- A song is only shown once it has been **heard playing**. A paused session left over from yesterday does not pop up the notch.
- After a pause, the song **lingers** for 3 seconds and then disappears, unless it resumes.
- A different song (title, artist or artwork key changed) resets all state.
- Artwork is fetched separately and retried up to four times per song. It is decoded, center-cropped to a square and resized to 96 x 96 (`media/artwork.rs`).
- If the player stops reporting a timeline, the last known one is extrapolated.

## Platform sources

### Windows: System Media Transport Controls (`media/windows/source.rs`)

Windows exposes every media app that integrates with the system media overlay (browsers, Spotify, Media Player and many more) through `GlobalSystemMediaTransportControlsSessionManager`.

- The source prefers **any session that is currently playing**; otherwise it uses the manager's current session.
- Title and artist come from the session's media properties. The artwork key combines the source app id, title and artist.
- Artwork is read from the thumbnail stream (up to 8 MB).
- The timeline comes from the session's timeline properties. If the session is playing, the position is advanced by the time since the player last updated it, converting from Windows 100 ns ticks.
- Controls call `TryTogglePlayPauseAsync`, `TrySkipNextAsync`, `TrySkipPreviousAsync` and `TryChangePlaybackPositionAsync` (the seek is offset by the timeline's start time).

The source joins a COM multithreaded apartment on its thread (`platform::Apartment`).

### macOS: Music and Spotify (`media/macos`)

macOS has no public API for other apps' now playing, so notchbase talks to the two big players directly (`media/macos/players.rs`):

| Player | Bundle id | Change notification | Artwork |
| --- | --- | --- | --- |
| Spotify | `com.spotify.client` | `com.spotify.client.PlaybackStateChanged` | an artwork URL, downloaded over HTTPS |
| Music | `com.apple.Music` | `com.apple.Music.playerInfo` | raw image data returned by AppleScript |

- The source subscribes to each player's **distributed notification** and marks it dirty when it fires. It pumps the run loop briefly on each poll so the notifications are delivered.
- A player is only queried if it is running (`NSRunningApplication`), so notchbase never launches Music or Spotify by asking about them.
- When dirty (or every 10 seconds while playing, to resync), it runs an AppleScript through `/usr/bin/osascript` that returns state, name, artist, album, position and duration in one line separated by a control character. `media/applescript.rs` parses the answer, converting Spotify's millisecond durations, and parses AppleScript's `«data ...»` hex blobs for Music artwork.
- A playing player wins over a paused one.
- Spotify artwork is downloaded with `ureq` (HTTPS only, native TLS with the platform verifier, 10 second timeout, 8 MB limit).
- Controls are AppleScript commands (`playpause`, `next track`, `previous track`, `set player position to`).

The first time notchbase scripts a player, macOS asks the user to allow automation. The bundle's `Info.plist` carries `NSAppleEventsUsageDescription` for that prompt; see [Platform: macOS](Platform-macOS.md).

## Levels: how the notch hears the music

### Capturing the output

The levels thread only runs capture while something is playing and a notch is dancing to it (`Notch::dances`: the song is revealed, or the notch is open, showing a prompt, a drop, a notice or a live activity), which the app passes on every frame with `Media::listen`. It keeps listening for 2 seconds after the dance stops, so a gap between two dancing moments doesn't reopen it, then closes the capture and publishes silence; it checks every 80 ms. On a Mac no tap exists the rest of the time, so the purple System Audio Recording dot in the menu bar only shows while the notch is actually dancing.

- **Windows** uses WASAPI **loopback** capture of the default output device, the same code the screen recorder uses for desktop audio (`record::Loopback`, see [Screen Recording](Screen-Recording.md#audio)). Samples arrive as 48 kHz stereo PCM and are mixed down to mono.
- **macOS** uses a Core Audio **global process tap**: `audio::Listener` creates a tap of all processes (unmuted, so nothing changes for the user), wraps it in a private aggregate device on the default output, and collects mono samples in an IO callback. This needs the "System Audio Recording" permission, the same one the per-app mixer uses; see [Sound Mixer](Sound-Mixer.md#macos-core-audio-process-taps).

If capture cannot be opened (no permission, no device, an unsupported format), the thread does not give up on the effect: it publishes a synthetic **groove** (`media/groove.rs`), a 118 BPM pattern of kick and hi-hat envelopes with slow sways, so the notch still dances in time-ish. It retries opening the capture every 2 seconds. If a working capture delivers nothing for 120 ms, the bands drop to zero.

### The spectrum analyzer

*Code: the `notchbase-spectrum` crate, `crates/spectrum/src/spectrum.rs` and `crates/spectrum/src/fft.rs`, shared with the installer's dance.*

`Analyzer` turns mono samples into six bands:

1. Keep the latest 1024 samples; analyze every 512 new samples (50% overlap).
2. If the RMS is below 1e-4, return silence.
3. Apply a Hann window and run a radix-2 FFT (`crates/spectrum/src/fft.rs`, an in-place iterative Cooley-Tukey implementation with bit-reversal) to get 512 magnitudes.
4. Sum the power in six bands with edges at **30, 110, 280, 650, 1500, 3500 and 10000 Hz**, converting the edges to bins for the actual sample rate.
5. Convert each band to dB relative to full scale, then normalise it against that band's own **decaying peak**: the peak follows new maxima instantly and falls by 6 dB per second, with a floor at -30 dB. The band value is its position within the 30 dB below the peak, raised to the power 1.7 for punch.

Per-band adaptive normalisation is what makes quiet songs and loud songs both dance: each band is judged against its own recent loudness, not an absolute level.

### From bands to the dance

The notch turns the six bands into motion with one underdamped spring per band and seven ripples along its bottom edge, so a drum hit overshoots and wobbles like liquid before settling. The dance is only applied while the song is playing, and it continues behind activities, recordings, prompts and drops. See [Notch](Notch.md#music) for the reveal, hug, beats and anticipation logic that sits on top of `NowPlaying`.

## Media keys

When the volume indicator is on and a player exists, notchbase claims the hardware play/pause, next and previous keys so the system's media overlay does not appear, and routes them through the same control path. See [Volume and Media Keys](Volume-and-Media-Keys.md).

## Tests

`media/track.rs`, `media/follower.rs`, `crates/spectrum/src/spectrum.rs`, `crates/spectrum/src/fft.rs`, `media/groove.rs`, `media/artwork.rs` and `media/applescript.rs` have unit tests for timelines, the follower rules, band analysis, the FFT, the groove, artwork decoding and AppleScript parsing. `media/windows/tests.rs` has an ignored test that reads the real media session and loopback output; run it on purpose with `cargo test -- --ignored` (see [Building and Testing](Building-and-Testing.md#ignored-tests)).
