# Screen recording

`src/record` records a display, or an area of one, to a video file, optionally with the microphone and desktop audio. On Windows it is a hand-built pipeline on top of DXGI Desktop Duplication, a multi-core BGRA to NV12 converter, WASAPI and a Media Foundation H.264 writer. On macOS it uses ScreenCaptureKit, which does capture, encoding and writing in one.

The Record Screen tool (`tools/screen-recorder`) is the user-facing part: its wide tile with Mic and Desktop chips and a microphone dropdown, the red tile with a running timer while recording, the live activity in the notch (a red dot and the elapsed time), and its settings. See [tools/Built-in-Tools.md](tools/Built-in-Tools.md). The area picker and recording dim are on [Screen Capture](Screen-Capture.md). The app side is `app/capture/recording.rs`.

## The flow

1. The tool sends `StartRecording { options }`. The SDK's `RecordingOptions` carry `microphone`, `desktop_audio`, `microphone_id`, `folder`, `max_fps` and `dim_outside`, filled from the tool's settings.
2. `App::record_for` ignores the request if a recording is already running or an area is being chosen; otherwise it keeps the options and runs the **area picker** (click a display or drag an area). The state reported to tools becomes `Choosing`.
3. When the picker returns a `Region`, `begin_screen_recording` converts the options to `record::Options { microphone, desktop, microphone_id, folder, max_fps }` and calls `Recorder::start(region, &options)`, which picks the output path and starts a platform `Session`. If `dim_outside` is set, the **recording dim** is shown around a partial area once recording has actually started. If starting fails, tools receive `RecordingUpdate::Failed { error }`.
4. Every frame and wake-up the app derives the `RecordingState` and feeds it to the tools that subscribed, whenever it changes: `Idle`, `Choosing`, `Starting` (the session exists but has not reported its start), `Recording { since }` (the start time as a Unix time in milliseconds, so tools can show a wall-clock timer), and `Saving`.
5. Stopping (the tool's `StopRecording` request, from its tile or its shortcut) calls `Recorder::stop`. The session finishes writing in the background; `Recorder::poll` returns `Saved { path, seconds, note }` or an error.
6. The app announces the result as `RecordingUpdate::Saved { path, seconds, note }` or `RecordingUpdate::Failed { error }`. The Record Screen tool then puts the file on the [Shelf](Shelf.md) (a `Shelve` request) and shows a "Saved" notice with the length, or the note if something went wrong partway (for example "Audio stopped partway: …"); on failure it shows the error.

## `Recorder` (`record/recorder.rs`)

- **States**: idle; busy and recording (`since()` gives the start time once known); busy and stopping (saving).
- **Output folder**: `Options::folder` if the tool set one (the Record Screen tool's hidden `folder` setting), otherwise the user's Videos folder on Windows (the known folder) or `~/Movies` on macOS (`platform::videos_dir`). It is created if missing.
- **File name**: `record::unique_file(folder, "Screen Recording", local_time, extension)` (`record/naming.rs`) gives `Screen Recording 2026-09-28 at 14.03.27.mp4` on Windows or `.mov` on macOS, with " 2", " 3" … added if the name is taken. The same function names saved screenshots.
- **Frame rate**: `Options::max_fps` overrides the automatic frame rate on both platforms, clamped to 1 to 120. The Record Screen tool's "Frame rate" setting offers Auto, 60 and 30.
- **Region** (`record/region.rs`): the display's native id, origin, physical size and scale, and the recorded area's origin and size in physical pixels. `Region::new` clamps the area into the display and rounds width and height down to even numbers (encoders need even dimensions), with a minimum of 2.
- Dropping the recorder stops and waits for any recording, so quitting mid-recording still produces a playable file.

Each platform provides a `Session` with `start(job, wake)`, `started()`, `stop()`, `finished()` and `wait()`, plus the file extension. `record/other.rs` reports "Screen recording isn't available on this system".

## Windows pipeline (`record/windows`)

Everything runs on one thread, `notchbase-record` (`session.rs`), which initialises COM (multithreaded) and Media Foundation for its lifetime (`runtime.rs`).

### Planning (`recording.rs`)

| Decision | Rule |
| --- | --- |
| Output size | the area's size; if a side exceeds 4096 or the area exceeds 4096 x 2304 pixels, both sides are halved (kept even) |
| Frame rate | `max_fps` if given (1 to 120); otherwise 60 fps up to 2560 x 1600 pixels of output, 30 fps above |
| Bitrate | 0.1 bits per pixel per frame, clamped to 2 to 40 Mbps |
| Audio | 48 kHz stereo, if the microphone or desktop audio is on |

### Screen capture (`screen.rs`, `dxgi.rs`, `gdi.rs`, `cursor.rs`)

**DXGI Desktop Duplication** is the primary path (`dxgi.rs`):

- Find the DXGI output whose desktop coordinates match the recorded display, create a D3D11 device on its adapter, and duplicate the output. Rotated outputs fall back to GDI.
- Each frame, `AcquireNextFrame` waits until the next frame deadline. When the desktop changed, the recorded area (a `D3D11_BOX` crop) is copied into a small CPU-readable **staging texture**, so only the area crosses to the CPU, not the whole desktop.
- The **mouse pointer** is not part of duplicated frames, so its position and shape are tracked from the duplication's frame info, and the pointer is composited in software onto a copy of the clean frame (`cursor.rs`). Monochrome, color and masked-color pointer shapes are all supported, including the XOR behaviour of inverting cursors.
- If the duplication is lost (a UAC prompt, a mode change, a full-screen app switching modes), the duplicator reconnects every 250 ms and keeps repeating the last frame meanwhile.

If duplication cannot start at all, the **GDI fallback** (`gdi.rs`) copies the area from the screen DC into a DIB section with `BitBlt` (`CAPTUREBLT`, so layered windows are included) at each frame deadline and draws the cursor with `DrawIconEx` at its hotspot.

### BGRA to NV12 (`nv12.rs`)

H.264 encoders want NV12 (a full-resolution luma plane plus a half-resolution interleaved chroma plane). The converter does the conversion on the CPU with integer BT.709 limited-range math:

- If the output is half size, it first box-filters 2 x 2 pixels into one.
- The frame is split into horizontal **bands** of row pairs, one per worker (up to 8, based on the available cores), and converted in parallel with `std::thread::scope`. Each band writes its own slice of the luma plane and its own slice of the chroma plane, so no synchronisation is needed.
- Chroma is averaged over each 2 x 2 block.

### Timing (`timeline.rs`)

- **Video frames** are scheduled on an ideal grid from the start time: frame *n* is due at *n / fps* seconds. When a frame is late, the next frame index is computed from the wall clock, so the video skips ahead instead of drifting. Timestamps and durations are written in 100 ns units.
- **Audio** is tracked per source in samples. A source's device position (converted to the same clock as the start time via `QueryPerformanceCounter`) tells where each packet belongs.

### Audio

The capture side lives in the shared `notchbase-wasapi` crate (`crates/wasapi/src`: `audio.rs`, `pcm.rs`, `loopback.rs`, `apartment.rs`), which the installer's dance also uses; the mixing stays in `src/record/windows/mixer.rs`.

- **Sources** use WASAPI in shared mode: the **microphone** is the chosen capture endpoint (by id) or the default one; **desktop audio** is a **loopback** capture of the default output device.
- Each source first asks WASAPI to deliver 48 kHz stereo 16-bit PCM with automatic conversion. If the device refuses, it opens the device's own mix format and converts itself (`crates/wasapi/src/pcm.rs`, the only part of the crate that also compiles off Windows, for its tests): 16, 24 and 32-bit integer or 32-bit float input, mono duplicated to both channels, center and rear channels folded into left and right at 0.7, and a linear resampler to 48 kHz.
- The **mixer** (`mixer.rs`) keeps one lane per source. Missing time is filled with silence: a gap before a packet's reported position (beyond 30 ms), and a lane that falls more than 250 ms behind the wall clock. This matters for loopback, which delivers nothing at all while the system is silent. The lanes are summed with clipping and emitted in chunks of 4800 frames (100 ms), always up to the slowest lane, so audio and video stay aligned.
- If a source fails partway, recording continues without it and the error becomes the recording's note.
- `crates/wasapi/src/loopback.rs` exposes the desktop-audio source on its own (`notchbase_wasapi::Loopback`, re-exported as `record::Loopback`), mixed down to mono floats; the music visualizer uses it on Windows (see [Media and Music](Media-and-Music.md#capturing-the-output)).

### Writing (`writer.rs`)

A Media Foundation **sink writer** writes an MPEG-4 file:

- video: **H.264 High profile**, unconstrained VBR at the planned mean bitrate, fed NV12 frames, tagged BT.709 (primaries, transfer and matrix) and limited range;
- audio: **AAC-LC** at 48 kHz stereo, 192 kbps, fed 16-bit PCM.

It first tries with hardware encoders enabled and falls back to software if the hardware transform cannot be set up. Samples are copied into Media Foundation memory buffers with their time and duration.

### The loop and stopping

Until the stop flag is set, the loop waits for the next frame or deadline, pumps audio into the writer, and writes a frame whenever one is due. When the flag is set, the loop makes one last pass without waiting, then pads each audio lane to the video's end time, flushes the mixer and finalizes the file. If not a single frame was recorded, the file is deleted and the error is reported ("Nothing was recorded").

## macOS: ScreenCaptureKit (`record/macos`)

On macOS the whole pipeline is ScreenCaptureKit's `SCStream` with an `SCRecordingOutput` (the recording output API Apple added in macOS 15), which captures, encodes and writes the movie in the system's own process.

1. `SCShareableContent` is fetched asynchronously. If it fails, the system's error description is reported, or "Allow Screen Recording for notchbase in System Settings, then reopen it" when there is none (usually the Screen Recording permission is missing).
2. `stream.rs` finds the `SCDisplay` with the recorded display's id ("That display is no longer connected" if it is gone) and builds an `SCContentFilter` for that display that **excludes notchbase's own windows**, so the notch, the timer and the dim never appear in the video.
3. The `SCStreamConfiguration`:
   - output size in pixels equal to the area; for a partial area, a source rectangle in points (converted with the display's scale);
   - no scaling to fit, the cursor shown, a minimum frame interval of 1/60 s (or 1/`max_fps`, clamped to 1 to 120), queue depth 6, best capture resolution;
   - **desktop audio** (`capturesAudio`) if requested, excluding notchbase's own sounds, at 48 kHz stereo;
   - the **microphone** (`captureMicrophone`) if requested, with the chosen device id.
4. The `SCRecordingOutputConfiguration` writes a QuickTime movie (`.mov`) with H.264 video to the output path.
5. A delegate class, `NotchbaseRecordingDelegate` (`delegate.rs`), receives "did start recording" (which sets the start time the timer uses), "did finish recording" (with the recorded duration) and failures from both the recording output and the stream.

Stopping calls `stopCapture`. If the finish callback does not arrive within 10 seconds, the recording is reported as "The recording didn't finish saving". The microphone permission prompt uses the bundle's `NSMicrophoneUsageDescription`; see [Platform: macOS](Platform-macOS.md).

## Tests

`record/region.rs` and `record/naming.rs` have unit tests, and `app/capture/recording.rs` tests that the recording start time maps to a stable wall-clock time. On Windows the module tests cover the recording plan (including the frame rate override), NV12 conversion (solid colors, chroma averaging, identical output across worker counts, halving), PCM conversion and resampling, the frame and sample clocks, the audio mixer's summing and waiting for the slowest source, loopback downmixing, and cursor compositing. Ignored tests record the real screen and desktop audio to a temporary file and read the real screen; run them deliberately (see [Building and Testing](Building-and-Testing.md#ignored-tests)).
