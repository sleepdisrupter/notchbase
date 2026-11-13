# notchbase-spectrum

The music spectrum the notch dances to. It runs an FFT over mono audio and boils it down to six loudness bands.

`Analyzer::new(rate)` makes one for a sample rate, and `push` takes mono samples and returns fresh `Bands` whenever a full window is ready. Each band runs from 0 to 1, together they cover 30 Hz to 10 kHz, and each is measured against its own slowly falling peak, so quiet and loud songs both fill the range.

The app uses it in `src/media`, `notchbase-paint` moves its liquid level to the bands, and the installer dances with it too. It's plain Rust with no dependencies. Capture lives elsewhere: `notchbase-wasapi` on Windows, and Core Audio in the app on a Mac.

Run the tests with `cargo test -p notchbase-spectrum`. They feed in tones, silence and steady input and check where the energy lands.
