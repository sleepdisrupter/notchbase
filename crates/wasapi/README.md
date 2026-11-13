# notchbase-wasapi

Windows audio capture: WASAPI clients for the microphone and the desktop mix, a loopback reader for the music spectrum, and the COM apartment they run in. On any other system it builds to an empty library.

- `Apartment::join()` puts the calling thread in a COM apartment until it's dropped. Hold one on any thread that captures.
- `Audio::microphone(id)` and `Audio::desktop()` open capture clients, and `next_packet` hands back stereo 48 kHz PCM (`CHANNELS`, `RATE`, `FRAME_BYTES`) with a timestamp when the device gives one.
- `Loopback` reads the default output mixed down to mono, for the spectrum.

The app's screen recorder in `src/record/windows` records the microphone and desktop audio with it, and `src/media` gets its loopback through the recorder. The installer uses the loopback for its dance. The Mac side of all this, Core Audio process taps and ScreenCaptureKit, lives in the app.

Run the tests with `cargo test -p notchbase-wasapi`. The PCM tests run anywhere and the rest only on Windows. Real capture is covered by the recorder's ignored tests: `cargo test record::windows -- --ignored`.
