# Screen capture

`src/capture` contains the interactive capture surfaces: the **color sampler** (a loupe that picks a color from anywhere on screen), the **region picker** for screenshots, the **area picker** that chooses what to record, and the **recording dim** that darkens everything outside a recorded area. Tools drive them through requests to the app: the Pick Color tool sends `PickColor`, the Screenshot tool `CaptureScreen`, the Record Screen tool `StartRecording` (see [tools/Built-in-Tools.md](tools/Built-in-Tools.md) and [tools/Commands.md](tools/Commands.md)). This page explains how the app runs each surface and answers. The recording itself is on [Screen Recording](Screen-Recording.md).

## How a capture starts (`app/capture/mod.rs`)

Capture surfaces must not have the notch in the way, and they must not capture the notch.

1. A tool's `PickColor { reply }` or `CaptureScreen { reply }` request (`app/tools/requests.rs`) records a `CaptureReply` (the tool's slot, the reply id and the mode) and requests the capture. If another capture is already running or pending, the tool is answered at once with an empty reply instead.
2. Any tool text field focus is released, and every notch is told to **collapse**. If a notch was open, the capture waits until all notches have collapsed, or at most `notch.collapse_before_capture_ms` (260 ms by default), so the panel is not frozen into a screenshot.
3. For screenshots and recordings the app checks **screen capture access**. On macOS this is the Screen Recording permission: the first time it asks with `CGRequestScreenCaptureAccess`, and after that it opens the Screen Recording pane of System Settings, while the notch shows "Allow Screen Recording for notchbase in System Settings, then reopen it" and the capture is cancelled. Windows needs no permission.
4. The app remembers which app was frontmost, starts the surface, and for the area picker (and the region picker on Windows) registers **Escape** as a temporary global hotkey so it cancels even if the overlay does not have keyboard focus. If the surface fails to start, its error is shown and the capture is cancelled.
5. When the surface finishes, the Escape hotkey is released, focus is handed back to the app that had it, and the waiting tool gets its **reply**:

| Outcome | Reply to the tool |
| --- | --- |
| a picked color | `Color { color: Some(rgba) }` in sRGB |
| a region screenshot | `Capture { capture: Some(Capture { image, width, height, on_clipboard }) }` |
| cancelled, failed, or a capture already running | `Color { color: None }` or `Capture { capture: None }` |

A captured image is stored in the **image store** (`src/tools/images.rs`) and the tool receives only its `ImageId`. The tool can then show it in a notice (as an image element with the capture as its source), copy it (`CopyImage`), save it (`SaveImage`) or open it to draw on (`Annotate`, see [Annotate](Annotate.md)), all by id. The store keeps the full pixels of the four most recent captures and 192 px thumbnails of the last 64. `on_clipboard` is always `false` (`App::captured` in `app/capture/mod.rs`): on both platforms the picker hands the pixels to the app and nothing is put on the clipboard until the tool asks for it.

Only one capture runs at a time. A tool can cancel the active capture with `CancelCapture` (the Record Screen tool does this when its tile is pressed again while you are choosing an area), and Escape cancels it too.

Each surface exposes the same small interface to the app: `start`, `cancel`, `is_active`, `owns(window_id)`, `window_event`, `about_to_wait` (advance animations and fades) and `take_outcome`.

Fades in and out are handled by `capture/overlay/session.rs`: a `Session` is `Idle`, `Open` (fading in with an ease-out over about 0.12 s) or `Closing` (holding briefly, then fading out) and carries the outcome until the fade has finished, so the result is delivered only after the overlay is gone from the screen.

## Color sampler (`capture/color`)

The sampler returns the picked color (in sRGB) to the app, which replies to the tool that asked. The Pick Color tool then copies it as text in the format chosen in its settings (hex like `#3A7BFF`, `rgb(...)` or `hsl(...)`) and shows a swatch notice in the notch.

### macOS: the system sampler

On macOS, notchbase uses `NSColorSampler`, the system's own magnifying loupe (`capture/color/macos.rs`). The selection handler converts the chosen `NSColor` to sRGB and sends it back through a channel. Cancelling from notchbase stops listening for the result.

### Windows: a custom loupe

Windows has no built-in sampler, so notchbase draws one (`capture/color/windows`, `capture/color/loupe`):

- **Grabbing pixels.** `Grabber` keeps a memory DC and a 31 x 31 pixel bitmap, and on every cursor move copies the pixels around the cursor from the screen with `BitBlt` (including layered windows, `CAPTUREBLT`), converting BGRA to opaque RGBA.
- **The window.** A 240 x 224 transparent, layered, non-activating tool window that is **excluded from screen capture** with `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. Because the loupe is invisible to screen capture, it can sit directly **on top of** the cursor without seeing itself. On Windows versions that cannot exclude it, the loupe sits beside the cursor instead, flipping sides near screen edges.
- **Input.** While the loupe is open, a dedicated thread (`notchbase sampler input`) installs low-level mouse and keyboard hooks (`WH_MOUSE_LL`, `WH_KEYBOARD_LL`) and **swallows** the relevant input so the click that picks a color does not also click the app underneath:

  | Input | Effect |
  | --- | --- |
  | left click, Enter, Space | pick the color under the cursor |
  | right click, Escape | cancel |
  | mouse wheel | zoom |
  | arrow keys | move the cursor by one pixel (ten with Shift), with key repeat |

- **Drawing** (`capture/color/loupe`). The loupe is a round lens (60 point radius) inside a glass bezel with a soft shadow. The grabbed pixels are drawn as a magnified grid of cells, snapped to whole physical pixels so every cell is crisp, with a faint grid and a marker on the center pixel. Zoom changes the cell size between 6 and 20 points (default 10) along a logarithmic scale, animated with a spring. A pill under (or, near the bottom of the screen, above) the lens shows a swatch and the hex value. The ring and pill tints ease toward the sampled color. Opening pops the loupe in from 60% scale; picking presses it slightly before it fades.

Setting `NOTCHBASE_LOUPE_IN_CAPTURES=1` disables the capture exclusion, which is useful for taking screenshots of the loupe itself.

## Region screenshots (`capture/region`)

A region screenshot produces an image of the selected area. The Screenshot tool decides what happens to it: with its default settings it copies the image to the clipboard, and it can also (or instead) save a file; it always shows a thumbnail notice with the size and "Copied" or "Saved", and an **Annotate** button that opens the screenshot in a window to draw and write on. What is added there reaches the clipboard only: a file saved by **Save screenshots** stays as captured. See [tools/Built-in-Tools.md](tools/Built-in-Tools.md) and [Annotate](Annotate.md).

The app performs the tool's follow-up requests (`app/tools/requests.rs`):

- **`CopyImage`** copies the full image from the image store to the clipboard on a background thread (`notchbase-clipboard`), because copying a large image can take a noticeable moment. A failure is reported as "Couldn't copy the screenshot".
- **`Annotate`** opens the full image from the image store in the annotator window; see [Annotate](Annotate.md). The image in the store is not changed by what is drawn, so `CopyImage` and `SaveImage` still give the capture.
- **`SaveImage`** writes a PNG, or a JPEG (without alpha), on the `notchbase-save-image` thread into the tool's chosen folder, or `Pictures/Screenshots` by default, creating the folder if needed. The file is named with `record::unique_file`: the prefix, the local date and time, and a number if the name is taken ("Screenshot 2026-09-28 at 14.03.27.png", "Screenshot 2026-09-28 at 14.03.27 2.png"). The tool gets a `Saved` reply with the path or the error.

### The live picker

Screenshots use notchbase's own picker on every platform, the same one that chooses what to record. `capture/region/live` is one full-screen overlay window per monitor:

- **Overlays** (`live/monitor.rs`) are transparent, borderless, always-on-top `GlWindow`s covering each monitor exactly (on Windows one pixel taller than the monitor) and excluded from screen capture. They are created once and reused; if the monitor layout changed they are rebuilt. The cursor is a crosshair.
- **The whole display.** Before you press, the display under the cursor is highlighted and the hint reads "Click to capture this display, or drag to capture part of it". A click takes the whole display, exactly as it does when choosing what to record.
- **Selecting.** Press and drag to select; hold **Shift** for a square. The selection is kept in physical pixels. A size label ("1280 x 720") follows the selection, placed below it, above it, or inside it, whichever fits. Arrow keys nudge the cursor by one pixel, ten with Shift. Right-click or Escape cancels (Escape is also grabbed as a global hotkey while the picker is open).
- **Drawing** (`live/draw.rs`). The screen is dimmed around the selection, with corner ticks on the selection.
- **Grabbing on Windows.** On release the selection is cut out of a fresh capture of that monitor (`platform::capture_monitor`), taken after the overlays are already excluded from capture so they never appear in the image. A selection smaller than 8 pixels on either side counts as a click and takes the whole display.
- **Grabbing on macOS** (`capture/region/macos.rs`). The picker is wrapped: when a selection is made, the overlays fade out first, then `/usr/sbin/screencapture -x -tpng -R<x,y,w,h>` captures exactly that rectangle into a temporary PNG. The rectangle is the selection converted to global points (the display's origin plus the selection, divided by the display's scale). The image is read back, the file removed, and the result goes to the Screenshot tool like on Windows, so its copy and save settings apply the same way.

`platform::capture_monitor` on Windows (`platform/windows/capture`) first tries **DXGI Desktop Duplication**: find the adapter output whose desktop rectangle matches the monitor (rotated outputs are not supported), duplicate it, acquire a fresh frame (up to four 120 ms waits), copy it to a CPU-readable staging texture and read the rows. If that fails, or returns an all-black image (which happens with some protected content and drivers), it falls back to a **GDI** `BitBlt` of the screen, and uses whichever result is not blank.

Set `NOTCHBASE_OVERLAY_IN_CAPTURES=1` to let the overlays appear in screen captures (for documentation screenshots), and `NOTCHBASE_PREVIEW_SELECTION=x0,y0,x1,y1` to start the picker with a selection already drawn (see [Previews](Previews.md)).

## The area picker

Choosing what to record uses the same live picker in **area mode**, on every platform including macOS (`capture::region::AreaPicker`):

- Hovering a display highlights the whole display and shows the hint "Click to record this display, or drag to record part of it".
- Clicking records the whole display; dragging selects an area (at least 32 pixels on each side, otherwise the whole display is used).
- The outcome is a `record::Region`: the display (its native id, origin, size in physical pixels and scale) plus the area's origin and size, clamped to the display and rounded down to even dimensions, which video encoders require.

The region is handed to the recorder together with the options from the tool's `StartRecording` request; see [Screen Recording](Screen-Recording.md). While the area picker is open, the recorder's state is reported to tools as `Choosing`.

## The recording dim (`capture/dim.rs`)

While part of a display is being recorded, notchbase darkens everything outside the recorded area so it is obvious what is in frame. It is shown when the recording's options ask for it (`dim_outside`, the Record Screen tool's "Dim around recordings" setting, on by default), once recording has actually started:

- a transparent overlay window covers the recorded display, is **click-through** (you keep working normally), and on Windows is excluded from capture and never takes focus;
- it paints four quads around the area in translucent black, plus a thin light edge around the area;
- recording a whole display shows no dim at all;
- after the dim window opens, the notch windows are raised above it so the notch and its timer stay visible.

On macOS the dim sits one window level below the notch; on Windows both are topmost and the notches are re-raised. Since the recording on macOS excludes notchbase's own windows and the Windows overlays are excluded from capture, the dim never ends up in the recording.

## Tests

`app/tools/requests.rs` tests that saved images are written as PNG and JPEG without overwriting each other. `capture/overlay` tests the session fades, glides and the frame clock. The live picker's geometry (selection, square constraint, clamping, pixel/point conversion), the size label placement, the dim's cut-out, screenshot thumbnails, and the loupe's zoom, lens snapping, placement, pill layout, tint easing and input gate have unit tests, and the loupe and picker drawing are painted headlessly. An ignored test on Windows briefly shows a window to check that a capture-excluded window is invisible to the grabber.
