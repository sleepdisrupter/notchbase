# Annotate

After a region screenshot, the Screenshot tool's notice carries an **Annotate** button. Pressing it opens the **annotator**: a standalone window that shows the screenshot and lets you draw on it with a round brush and write on it with a text tool. Every change is copied to the clipboard again, so what you paste is always the picture as it looks in the window. The brush's colour and size are remembered in `config.json`; the tool is not, and every window opens on the brush.

What is drawn and written reaches the clipboard only. Nothing in `src/annotate` writes a file: a file saved by the Screenshot tool's **Save screenshots** setting is written by the tool from the capture, and stays as captured whatever is added afterwards.

The annotator is app code, not a tool. A tool only asks for it, with the `Annotate` request and the `ImageId` of a capture.

| Path | Role |
| --- | --- |
| `src/annotate/window.rs`, `clock.rs` | `Annotator`: the window, its events, its repaint deadline and its frame clock |
| `src/annotate/editor/` | `Editor`: layout, the two tools, the pen, the block of words being typed (`typing/`), keys, the toolbar, the clipboard status, painting |
| `src/annotate/brush.rs`, `fit.rs`, `rect.rs` | the `Brush`, how the picture is placed and sized (`Fit`, `window_size`), and `PixelRect`, the dirty-rect type |
| `src/annotate/stroke/`, `lettering/`, `mark.rs` | a stroke (smoothed points, a colour, a width), a block of words (`Lettering`, set by the `Typesetter`), and `Mark`, which is one or the other |
| `src/annotate/raster/` | the rasterizer that inks a mark into pixels |
| `src/annotate/sheet/`, `composite.rs` | the picture in the window with its undo history, and the picture that goes to the clipboard |
| `src/annotate/copier/` | the `notchbase-annotate-copy` thread that writes to the clipboard |
| `src/annotate/sample.rs` | the sample screenshot the preview and the snapshot share |
| `src/app/annotate/` | the app's side: opening, replacing and closing the window, the copy debounce (`copying.rs`), the copier's reports (`reports.rs`), the copiers of closed windows (`retiring.rs`), and who has the keyboard (`keyboard.rs`) |
| `src/app/return_to.rs` | `ReturnTo`: what the notch, a capture or a text field hands the keyboard back to, an app or the annotator |
| `src/platform/{macos,windows}/annotate.rs`, `platform/other.rs` | the window attributes on each platform, and the height of the title bar over the content |
| `src/config/annotate.rs` | `AnnotateConfig`, the remembered brush |

## How it opens

1. **The notice.** When a capture arrives, the Screenshot tool (`tools/screenshot/src/lib.rs`) sends its notice with `Command::notice_with_controls`, the control being `button("Annotate").on_press(Msg::Annotate(capture.image))`. The notice stays for 5 seconds. The button is offered whether the capture was copied or only saved. See [Notch](Notch.md#tool-notices) for where the button sits and how the pointer treats it, and [tools/Surfaces.md](tools/Surfaces.md#controls) for the tool author's side.
2. **The press.** The notch returns `Action::Tool(slot, Surface::Notice, handler, UiValue::Press)`, which the app delivers to the tool as a `ui` event on the `notice` surface.
3. **The tool's answer.** `Msg::Annotate(image)` returns `Command::dismiss_notice().and(Command::annotate(image))`: the notice ends at once (see [Notch](Notch.md#tool-notices)) and the app receives `Request::Annotate { image }`.
4. **The app** (`app/tools/requests.rs`, `App::open_annotator` in `app/annotate/mod.rs`) looks the image up in the image store. The store keeps the full pixels of only the four most recent captures; an older one is answered with the message "That image is no longer available".

Then, in `App::annotate`:

- **The same screenshot again.** If the open window already shows that screenshot (the same `Arc<Screenshot>`), the window is un-minimized and focused instead of replaced, so what was drawn and written and the undo history stay. A second press before the notice has gone does no harm for the same reason.
- **Another screenshot.** An open window is retired first: what it had in progress is finished, a copy that was still waiting is handed to the clipboard at once, and the window is dropped. Its copier is kept until that last picture has been reported (see [Quitting and closed windows](#quitting-and-closed-windows)). Only one annotator exists at a time.
- **The monitor** is the active notch's, otherwise the primary monitor, otherwise the first one found.
- **The brush** is read from `config.annotate` (`Brush::from(&AnnotateConfig)`), after an open window has been retired, so a brush that window had still to report is the one the new window opens with. The tool is read from nowhere: a window opens on the brush.
- **The window** is opened with `Annotator::open`. If it cannot be, the notch says "Couldn't open the screenshot to draw on" and the reason goes to stderr. A screenshot with no pixels, or whose buffer is not `width x height x 4` bytes, is refused this way (`Sheet::new` returns `None`).
- **The copier** is started for this window (below). If its thread cannot start, the notch says "Couldn't copy the screenshot" once and the window reads "Not copied" each time a changed picture comes due for copying.

Once open, the window holds its own reference to the screenshot, so it does not matter that the image store later drops that capture.

## The window (`annotate/window.rs`)

`Annotator` is a `GlWindow` (see [Rendering and Motion](Rendering-and-Motion.md)) plus an `Editor`. Unlike the notch and the capture overlays it is an ordinary window: titled, resizable, and meant to hold the keyboard.

**Size and place.** `annotate::window_size` gives the content size in points: the screenshot at one image pixel per screen pixel plus the editor's chrome (`CHROME`, 56 x 160 points), shrunk in proportion until the window fits in 80% of the monitor, and never smaller than `SMALLEST_WINDOW` (660 x 440), which is also the window's minimum size. The window opens centred on its monitor (`Screen::centred`, in points on macOS and physical pixels elsewhere).

**Opening.** The window is created hidden, `platform::configure_annotate_window` runs, the first frame is painted, and only then is the window shown, configured once more and focused, so it never appears empty.

**Platform attributes** (`platform::annotate_window_attributes(size, least)` and `platform::configure_annotate_window(window)`, with identical signatures on all three platforms):

| | macOS (`platform/macos/annotate.rs`) | Windows (`platform/windows/annotate.rs`) | other (`platform/other.rs`) |
| --- | --- | --- | --- |
| Title | "Annotate", hidden; the title bar is transparent and the content view fills the whole window | "Annotate", in a normal title bar | "Annotate" |
| Look | transparent window with the dark appearance (`NSAppearanceNameDarkAqua`, unless one is already set) | dark theme, the executable's icon | defaults |
| Level | `WindowLevel::AlwaysOnTop` | normal | normal |
| Other | resizable; a click that only brings the window forward does not reach it (`with_accepts_first_mouse(false)`) | resizable; takes no file drops | resizable |
| `configure_annotate_window(window)` | dark appearance, the collection behaviour (`FullScreenAuxiliary \| MoveToActiveSpace`), the frosted body | nothing | nothing |
| `annotate_title_inset(window)` | the height of the title bar as the window reports it: the frame's height less the height of `contentLayoutRect`. 28 (`USUAL_TITLE_BAR`) when the window gives no usable answer | 0 | 0 |

**The frosted body on macOS.** `configure_annotate_window` puts an `NSVisualEffectView` with the `HUDWindow` material, blending behind the window and always active, into the window's frame view **below** the content view, sized to it and resizing with it. It is inserted once: a second call finds it (`is_frosted`) and leaves it alone. The editor then washes the whole window with the ink colour at 78% (`WASH` in `editor/paint.rs`) instead of filling it solid, so the blurred desktop shows through the body, and the toolbar is painted as see-through glass (`glass::set_seen_through`). Whether the body is frosted follows `platform::BLURS_BEHIND`; where it is false (Windows), the body is solid ink and the toolbar is an opaque capsule.

**Spaces and full-screen apps on macOS.** `open_on_the_space_in_use` sets the window's collection behaviour to exactly `FullScreenAuxiliary | MoveToActiveSpace`, and only when it differs from what the window already has. `FullScreenAuxiliary` lets the window sit in the Space of another app's full-screen window, so a screenshot taken over a full-screen app can be annotated there instead of on a desktop behind it. `MoveToActiveSpace` brings the window to the Space in use when it is focused again, which is what a second press on Annotate for the same screenshot does. The behaviour names neither `FullScreenPrimary` nor `FullScreenNone` (AppKit allows at most one full-screen behaviour), so the annotator is never a full-screen window of its own, and it does not carry `CanJoinAllSpaces`: it lives on one Space at a time.

**No colour space is set on macOS.** The notch, overlay and dim windows are tagged sRGB (`use_srgb` in `platform/macos/window.rs`); the annotator is not. A region capture on macOS is the PNG `/usr/sbin/screencapture` writes, read with `image::open(..).into_rgba8()` (`capture/region/macos.rs`), which takes the bytes as they are and applies no colour profile. Those bytes are display-space values, so a window that leaves them untagged shows the picture as the screen showed it.

**Why it floats on macOS.** notchbase is an accessory app (`ActivationPolicy::Accessory` in `platform/macos/system.rs`, `LSUIElement` in `macos/Info.plist`): it has no Dock icon and no place in the app switcher. A window at the normal level that another app covered could only be reached again by finding a visible piece of it to click. At the floating level it stays above other apps' windows until it is closed. On Windows the annotator is an ordinary decorated window and stays at the normal level.

**Moving it.** The top 44 points of the window are the **title strip**. A left press there starts a window drag (`Window::drag_window`) and is not passed on to the editor (`Editor::moves_the_window_from`). The text in the strip is written on the line of the title bar's buttons. Before every paint `Annotator::paint` asks `platform::annotate_title_inset(window)` how tall the title bar over the content is and passes it to the editor (`Editor::sits_under_a_title_bar`); `Layout::title_line(title_bar)` puts the line half that height from the top, and never lower than the middle of the strip, since the height counts for at most the strip's 44 points. Where there is no title bar over the content (0, on Windows and other platforms) the line is the middle of the strip. So on macOS the size label and the status sit level with the traffic lights whatever height the system gives the title bar. Because the editor never sees a press on the strip, such a press does not finish a block of words being typed.

**Events.** `Annotator::window_event` paints on `RedrawRequested`, answers `CloseRequested` by finishing what is in progress and then calling `Editor::close()` (so the outcomes are `Changed` and then `Close`), tracks focus and the pointer, and passes everything else to egui, asking for a redraw when egui wants one. When the window loses the keyboard (`Focused(false)`), `Annotator::hear` first calls `Annotator::finish_what_is_in_progress()`, which commits a line being drawn and places a block being typed without waiting for a frame and without closing. The app makes the same call before a window is retired or the app quits. The window hands the editor the events egui has queued and no frame has read yet (`GlWindow::input`), so what was typed a moment before is not lost (see [What no frame has read](#what-no-frame-has-read)). The window is never dropped inside its own event: the app drains `take_outcomes()` in its frame and closes it there.

**Pacing.** The annotator has its own frame clock (`annotate/clock.rs`): `dt` follows the wall clock up to 1/20 s, and the first frame, or a frame after more than 0.1 s of rest, takes a nominal 1/60 s. After each paint `repaint_at` is "now" while any spring is moving (`Editor::is_animating`), otherwise the delay egui asked for (ignored when it is an hour or more). The app's frame redraws the window when that time has come and includes it in its next wake-up, so an idle annotator paints nothing. That holds with a block of words open too: its caret is steady, and nothing is laid out or rasterized in a frame in which nothing changed.

## The editor (`annotate/editor`)

The editor is painted with the canvas and glass like the notch, not with egui widgets, and everything that moves is a spring.

### Layout (`editor/layout.rs`)

`Layout::of(window)` is pure geometry, in points:

- **Title strip**: the top 44 points. The screenshot's size ("1600 × 1000") is written in the middle, and the clipboard status at the right, 18 points from the edge.
- **Stage**: where the picture goes. 28 points in from the left and right, 8 below the title strip, and ending 28 above the toolbar.
- **Toolbar**: one capsule, 574 x 52, centred, 28 points above the bottom edge. Left to right: the tool control (brush, text), a divider, five colour swatches, a divider, the well (a dot showing the brush at its real size and colour, or a letter while the text tool is chosen), the size slider, a divider, undo, redo, and **Done**. The toolbar has one width: in the smallest window it keeps 43 points on either side.

The picture is placed by `Fit` (`annotate/fit.rs`): scaled down to fit the stage, never scaled up past one image pixel per screen pixel, centred, with its corner snapped to a whole screen pixel. It is drawn from one texture (linear, with mipmaps) with square corners and a soft shadow under it, so the picture is shown right into its four corners. A screenshot with a side longer than the graphics card's largest texture is not uploaded: the stage says "This screenshot is too large to draw on" with the limit, and nothing can be drawn.

The toolbar rises 18 points into place as the window opens. Layout tests hold that the title, stage and toolbar never overlap, that every control stays inside the toolbar, and that the toolbar keeps at least the picture's 28 point margin, down to the smallest window.

### The tools (`editor/tool.rs`, `toolbar/tools.rs`)

`Tool` is `Brush` or `Text`. The control at the left end of the toolbar has a segment for each: a squiggle and a serifed T (`Glyph::Brush`, `Glyph::Text`). A pearl thumb sits under the chosen one and glides to the other on a spring; the glyph on the thumb is drawn in ink, the other in dimmed pearl. <kbd>B</kbd> and <kbd>T</kbd> choose a tool while nothing is being typed.

Choosing the other tool finishes what this one had in progress: a line being drawn is committed, a block being typed is placed. The tool is not remembered. Every window opens on the brush.

### The brush (`annotate/brush.rs`)

A `Brush` is an opaque colour and a size in **view points**, from 2 to 32 (`Brush::THINNEST`, `Brush::THICKEST`), 5 by default. The palette is five colours from `ui::color`:

| Key | Colour | Hex |
| --- | --- | --- |
| 1 | `RED` (the default) | `#EC545A` |
| 2 | `YELLOW` | `#F2C14E` |
| 3 | `GREEN` | `#3FC48A` |
| 4 | `PEARL` | `#F4F3F0` |
| 5 | `INK` | `#0E0E0F` |

- **Swatches** (`toolbar/swatches.rs`). A ring marks the chosen colour and glides to a new one on a spring. A colour that is not in the palette (from a hand-edited `config.json`) draws in that colour and shows no ring.
- **Size** (`toolbar/size.rs`). The slider moves in half-point steps along a squared curve, so thin brushes get the roomier half of the track. The dot beside it, in the well, is the brush at its true size. While the text tool is chosen the dot gives way, on a spring, to the text glyph in the brush colour, which grows with the size until it fills the well.
- **A light edge for dark colours** (`toolbar/rim.rs`). A swatch, and the dot or the letter beside the slider, whose colour would sink into the dark toolbar gets a blurred white disc behind it, a little larger than itself. It goes by brightness, not by name: nothing above a luma of 0.2, growing stronger toward black. Of the palette only `INK` has it, and so does a dark colour typed into `config.json`.
- **On screen, not on the image.** The size is what you see: a stroke's width in image pixels is the brush size divided by the picture's scale (`Fit::stroke_width`), so a 5 point brush looks 5 points wide whether the screenshot is shown at full size or shrunk.
- **Haptics.** Picking another colour or the other tool, and the size reaching either end of its range, play `Feel::Tick`; an undo or redo that changed something plays `Feel::Snap` (`platform::haptic`, see [Notch](Notch.md#motion)).

### The text tool (`editor/typing/`)

With the text tool chosen, the pointer over the picture is the system's text cursor (`CursorIcon::Text`), there is no brush ring, and a press never starts a stroke.

**A block of words.** A press on the picture opens a **block**: an empty one that starts at the press with its first line centred on it, or, when the press lands on words already on the picture, those words again (see [Editing a placed block](#editing-a-placed-block)). What is typed goes in at the caret. One block is open at a time (`Editor::typing`, a `Typing`), and it is work in progress like a line being drawn: it is not in the `Sheet`, not in the undo history and not on the clipboard until it is **finished**.

- **Colour.** The brush colour at the moment the block is opened. A swatch pressed while it is open recolours it.
- **Size.** Letters follow the size control: `Brush::letter_size()` is `10 + 2 x size` points on screen, so 14 at the thinnest brush, 20 at the default 5 and 74 at the thickest. Like a stroke's width, that is divided by the picture's scale (`Fit::em`) to give the em in image pixels, so letters look as large as the control says whether the screenshot is shown at full size or shrunk. It is taken when the block is opened and again whenever the slider is used while it is open; resizing the window does not resize a block. Lines are 1.25 em apart.
- **Font.** The app's semibold face (`ui::font`: SF Pro on macOS, Segoe UI Semibold on Windows) with egui's bundled fonts behind it for the characters it lacks.
- **Lines.** Nothing wraps. <kbd>Shift</kbd>+<kbd>Enter</kbd> starts a new line and pasted text keeps its line breaks; every other control character is dropped, and so are the zero-width and direction marks that no font draws (`NEVER_DRAWN`), so a block cannot hold words that leave no ink. A block holds at most 10,000 letters (`LONGEST` in `typing/buffer.rs`).
- **The edge.** A block may hang over the edge of the picture; the part that hangs over is neither shown nor copied. Half an em of it always stays on the picture (`Typing::kept_on_the_picture`). That is checked every time the block is typeset, not only while it is dragged: a block at the edge slides back in as letters are erased from it or it is made smaller, and the block that is placed, with its empty last lines trimmed, still has its foothold and can be pressed again. A press closer than half an em to the right edge opens the block half an em in, so typing never moves it.

**What is shown while typing** (`typing/show.rs`). The open block is drawn from the very mask that will be inked. Whenever its words, colour, size or place change, it is typeset at image resolution (`typing/setter.rs`; see [From marks to pixels](#from-marks-to-pixels)), and the mask is uploaded as a small texture of its own and drawn where its pixels will land (`fit.to_view(area)`), with the picture's texture options and clipped to the picture. So what is typed is what is inked. Around it, in view space and never copied, are a thin rounded outline (the block's frame plus 6 x 3 points) and the caret. The outline is a dark halo under a pearl line like the brush ring, so it reads on any picture. The caret is a bar in the block's own colour over a faint halo of the opposite tone, dark round a light colour and light round a dark one (`halo_and_bar`): beside letters that can be read it is one bar of their colour, and on a picture that swallows the colour the halo still shows where it stands. Both are drawn up to 8 points beside the picture and no further. A caret that is further out than that to the left or right, after words that run off the picture, rests at that limit on its own line, so it can still be seen; a caret on a line above or below the picture is not shown. The caret does not blink.

**The pointer while a block is open** (`typing/scribe.rs`, which reads the frame's events in order as the pen does):

- A press inside the outline takes hold of the block. Let go without moving (within 3 points), it puts the caret where it was pressed; moved, it drags the block. Only the outline as it is drawn counts (`Typing::outline_as_drawn`): where a block hangs over the edge, a press beside the picture further out than the outline is shown finishes the block like any other press there.
- A press anywhere else on the picture finishes the block and then does what a press does with no block open. A press on the stage beside the picture only finishes it.
- A press on the toolbar is the control's: a swatch recolours the open block, the slider resizes it, choosing the brush finishes it, and **Done** finishes it and closes.

**Finishing.** Finishing a block **places** it: the words become a `Lettering`, committed to the `Sheet` as one undo step, reported as `Changed` and copied like any other change. White space after the last word is trimmed first. A block with no words, or with white space only, places nothing and leaves no undo step. A block is finished by:

- <kbd>Enter</kbd> or <kbd>Esc</kbd>; the window stays open;
- a press outside the block, as above;
- choosing the brush;
- undo and redo, which finish the block and then step. <kbd>Cmd</kbd>+<kbd>Z</kbd> while typing therefore takes the whole block away in one step, and redo brings it back;
- every way of closing: the frame reports `Changed` and then `Close`;
- the window losing the keyboard (see [Focus](#focus));
- the window being retired for another screenshot, and quitting (see [Quitting and closed windows](#quitting-and-closed-windows)).

Placing a block needs no frame. The typeset result is already held by the `Typing`, so `Editor::close()` and `Editor::finish_what_is_in_progress(unread)` place the block with no `egui::Ui` in hand.

#### What no frame has read

Key and pointer events wait in egui's queue until the next frame reads them. A block finished outside a frame (the keyboard leaving, the close button, a window retired, quitting) may therefore have keys waiting that were typed for it. Left alone they would be read by the next frame with no block open: the letters would be lost, a number key would pick a swatch, and an <kbd>Enter</kbd> meant to finish the block would close the window.

So `Editor::finish_what_is_in_progress(unread)` is given that queue, and while a block is open it reads from it first (`keys::typed_and_not_yet_read`):

- Everything from the keyboard (text, a paste, key presses) ahead of the first key that is a command while typing (<kbd>Enter</kbd>, <kbd>Esc</kbd>, undo, redo, the close shortcut) or the first press of a pointer button is taken out of the queue and written into the block, in order, exactly as a frame would have written it.
- If what stopped the reading is an <kbd>Enter</kbd> or an <kbd>Esc</kbd>, that strike is taken too: finishing the block is all it would have done.
- Everything else stays for the next frame: undo, redo and the close shortcut, which then act on the placed block as they would have; a press and whatever follows it; and every pointer event, so egui's own picture of the pointer is not disturbed.

With no block open nothing is taken. A line being drawn reads no keys.

#### Editing a placed block

With the text tool, a press on words already on the picture opens them again. The press is tested against each block's **frame**, the box of its lines (`Lettering::is_at`), not against its ink, and the topmost block under the press is the one opened (`Sheet::words_at`). A line drawn over the words does not keep a press from them.

- The block is **lifted** (`Sheet::lift`): the picture underneath is redrawn without it, and it is shown as the block being typed, with its own words, colour, size and place. Lifting is not a change: it adds no undo step and no revision, and the clipboard still holds the picture with the block on it.
- The press that opened it also holds it, so a drag moves it straight away; let go without moving, the press puts the caret where it landed.
- It keeps its own colour and size until a swatch is pressed or the slider is used, and then takes the brush's. That goes for the swatch already chosen too.
- Finishing **replaces** it at its place in the order of marks (`Sheet::replace`), so lines drawn after it stay on top of it. The replacement is one undo step: undo brings the old words back, where they were.
- Finished as it was opened, it is only put back (`Sheet::settle`): nothing is reported and nothing is copied.
- Finished with no words left, it is replaced by an empty `Lettering`, which inks nothing and is never found by a press, but keeps its place in the order so the undo history stays valid. Undo brings the words back.

#### Limits of the text tool

- **One colour, no outline.** A block is its letters in one colour, with no edge or plate behind them: pearl words on a white screenshot are as hard to see as a pearl stroke.
- **The size cap.** Letters stop growing at 320 image pixels per em (`TALLEST` in `lettering/typeset`) and are never smaller than 6. The cap is reached only by the largest sizes on a large screenshot shown small: at the thickest size a 5K screenshot in the smallest window would ask for more than twice as much.
- **No input method (IME).** The block reads egui's `Text` and `Paste` events, as every text field in the app does, and the input method is never switched on. Characters a keyboard layout types directly are written on both platforms. On macOS dead-key accents do not combine, and press-and-hold accents and the emoji picker insert nothing. Chinese, Japanese and Korean input methods work on neither platform.
- **Emoji in one colour.** Emoji come from egui's bundled outline font and are drawn in the block's colour.
- **No right-to-left reordering.** Arabic and Hebrew come out in the order typed. A character none of the fonts has is drawn as a box.
- **The letters differ between platforms**, since each uses its system font.
- **Paste rarely has anything to paste.** It inserts only text, and the clipboard here usually holds a picture: the Screenshot tool's or the annotator's own.
- **Copy and cut do nothing.** No text is ever written to the clipboard, which belongs to the picture.

### Keys (`editor/keys.rs`)

While nothing is being typed:

| Key | Does |
| --- | --- |
| <kbd>1</kbd> to <kbd>5</kbd> | pick a swatch |
| <kbd>[</kbd> / <kbd>]</kbd> | size one point down / up |
| <kbd>B</kbd> / <kbd>T</kbd> | choose the brush / the text tool |
| <kbd>Cmd</kbd>+<kbd>Z</kbd> (<kbd>Ctrl</kbd>+<kbd>Z</kbd> on Windows) | undo the last change |
| <kbd>Cmd</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd>) | redo |
| <kbd>Esc</kbd> | while a stroke is being drawn, drop that stroke and stay open; otherwise close |
| <kbd>Enter</kbd>, <kbd>Cmd</kbd>+<kbd>W</kbd> (<kbd>Ctrl</kbd>+<kbd>W</kbd>) | finish the stroke being drawn, if any, and close |

While a block of words is open:

| Key | Does |
| --- | --- |
| any key that types, and pasted text | goes into the block at the caret. The number keys, the brackets, <kbd>B</kbd> and <kbd>T</kbd> type their characters: they pick, size and choose nothing |
| <kbd>Shift</kbd>+<kbd>Enter</kbd> | a new line |
| <kbd>Enter</kbd> | finish the block and stay open; a second <kbd>Enter</kbd> then closes, as above |
| <kbd>Esc</kbd> | finish the block, keeping its words, and stay open; a second <kbd>Esc</kbd> then closes |
| <kbd>Backspace</kbd> / <kbd>Delete</kbd> | erase the letter before / after the caret; pressed in a block with no letters, drop the block. Held, they stop at the empty block: only a new press drops it |
| the arrow keys, <kbd>Home</kbd>, <kbd>End</kbd> | move the caret by a letter, by a line, or to an end of its line; on a Mac <kbd>Cmd</kbd>+<kbd>Left</kbd> and <kbd>Cmd</kbd>+<kbd>Right</kbd> do what <kbd>Home</kbd> and <kbd>End</kbd> do |
| <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>Z</kbd> | finish the block, then undo |
| <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | finish the block, then redo |
| <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>W</kbd> | finish the block and close |
| <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>C</kbd>, <kbd>X</kbd> | nothing |

There is no moving or erasing by word and no select-all.

Keys and toolbar controls both produce a `Command` (`editor/command.rs`), applied in `editor/act.rs`. The undo and redo buttons are dimmed and take no clicks while there is nothing to step to; undo is live as soon as an open block holds words. **Done** and the window's close button close it too.

**While a stroke is in progress.** <kbd>Esc</kbd> is `Command::Cancel`: the line is thrown away, nothing is committed and the window stays. Every way of closing is `Command::Close`, which first commits the line in progress as if the button had been let go, so the frame reports `Changed` and then `Close` and that last line reaches the clipboard. The window's own close request goes the same way, through `Editor::close()`.

**While a block is open.** `keys::commands(ui, busy)` is told what the editor is busy with (`Busy::Idle`, `Drawing` or `Typing`). While typing, <kbd>Esc</kbd> and <kbd>Enter</kbd> are `Command::Finish`, which places the block and keeps the window; `Command::Close`, undo and redo place it first. <kbd>Shift</kbd>+<kbd>Enter</kbd> is tested by its modifier and left for the block, because egui's key matching would otherwise take it for a plain <kbd>Enter</kbd>.

**Why the plain shortcuts are off while typing.** A key that types reaches the editor twice, as a key press and as its text, and taking the key press does not take the text. A shortcut left on would both act and type, so while a block is open the number keys, the brackets, <kbd>B</kbd> and <kbd>T</kbd> are not asked for at all. That also covers layouts whose letters sit on those keys: the Russian <kbd>х</kbd> is the <kbd>[</kbd> key by position, and it types `х`.

**Order within a frame.** While a block is open, whatever was typed ahead of the frame's first command key is written first (`keys::typed_before_any_command`), then the commands run, then the rest of the frame's events are read in order. A letter that arrives in the same frame as the <kbd>Enter</kbd> after it is therefore part of the block that <kbd>Enter</kbd> finishes. What the editor is busy with is decided once, at the start of the frame. A key that arrives in the very frame of the press that opens a block is therefore still read as it is with no block open: a number key picks its swatch (and its character is typed into the new block as well), and <kbd>Enter</kbd> closes.

**Held keys.** <kbd>Esc</kbd>, <kbd>Enter</kbd> and <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>W</kbd> count only when they are struck: the repeats of a held key are consumed and ignored (`consume_strikes`). Holding <kbd>Esc</kbd> to drop a line, or <kbd>Esc</kbd> or <kbd>Enter</kbd> to finish a block, therefore never goes on to close the window, and a held key asks to close once. A held <kbd>Shift</kbd>+<kbd>Enter</kbd> breaks the line once. The number keys, the brackets and undo and redo do repeat while held, and so do letters, <kbd>Backspace</kbd>, <kbd>Delete</kbd> and the arrows in a block. A held <kbd>Backspace</kbd> or <kbd>Delete</kbd> erases down to the empty block and stops there: dropping the block takes a new press (`Scribe::read` asks for a key that is not a repeat), so letting the key run on never closes the block under the words typed next. The window filters repeats a second time, before egui sees them: egui forgets which keys are held when a window loses the keyboard, so a key still held when the keyboard comes back (after a capture cancelled with <kbd>Esc</kbd>, for one) would look like a fresh strike. `Annotator::hear` reads winit's own repeat flag and drops the closing keys that event queued (`forget_repeated_strikes` in `editor/keys.rs`).

**No keyboard focus on controls.** Every control in the editor senses clicks, drags or hovering only, none of which egui lets the keyboard focus, so <kbd>Tab</kbd> and <kbd>Space</kbd> never press a swatch or a button. In an open block <kbd>Space</kbd> types a space and <kbd>Tab</kbd> does nothing.

### The pointer (`editor/pen.rs`)

With the brush (the text tool's pointer is described under [The text tool](#the-text-tool-editortyping)):

- A left press on the picture starts a stroke. A press anywhere else starts nothing.
- Every pointer position of a frame becomes a sample, not just the last one, so a fast drag keeps its shape. Positions outside the picture are clamped to its edge, so a line dragged off the picture stops there.
- Releasing ends the stroke. So does a frame in which the button is no longer down, which covers a release the window never saw.
- Where a press would draw, the system cursor is hidden and a ring the size of the brush follows the pointer instead (white, with a dark halo), fading in and out on a spring. The ring uses the pen's own test (`pen::starts_at`: the pointer is inside the picture's rectangle) on top of the picture being hovered, so it never promises ink just outside the picture's edge. It also stays while a stroke is in progress.
- While it is being drawn, the stroke is painted live with egui shapes in view space (discs joined by segments, thinned to one point every 0.75 points, clipped to the picture). On release it is committed to the `Sheet` and from then on it is pixels in the texture.

### What the editor reports

`Editor::ui(ui, dt)` returns `Outcome`s for the app:

| Outcome | When |
| --- | --- |
| `Changed(revision)` | the picture changed this frame (a stroke committed, a block of words placed or replaced, an undo or a redo); at most once a frame, with the newest revision, and ahead of the frame's other outcomes |
| `Remember(brush)` | the brush differs from the one last remembered: straight after a swatch or a bracket key, and for the slider only when it is let go |
| `Close` | any of the ways to close; a line or a block of words that was in progress has been committed and reported as `Changed` first |

`Editor::close()` and `Editor::finish_what_is_in_progress(unread)` return outcomes the same way, outside a frame. The second never reports `Close`.

## From marks to pixels

The picture is the screenshot with **marks** on it, in the order they were made. A `Mark` (`annotate/mark.rs`) is a line or a block of words: `Mark::Line(Stroke)` or `Mark::Text(Lettering)`.

**A stroke** (`annotate/stroke`) is built with `Stroke::from_samples(color, width, samples)` in image-pixel space. Samples that are not finite or lie absurdly far away are dropped, samples closer than 0.05 pixels to the last one are merged, and the rest are smoothed (`stroke/smooth.rs`): the line runs from midpoint to midpoint of neighbouring samples, bending toward the sample between them on a quadratic curve. Each curve is flattened by halving: it is cut in two again and again until every piece lies within 0.1 pixels of the true curve (`FLATNESS`), at most 16 halvings deep. A straight drag is therefore cut only where the pointer was seen, a tight bend is cut finely, and the number of segments does not depend on the brush's width. A corner is rounded instead of kinked, and the line still starts and ends exactly where the pointer did. The colour is made opaque, the width is kept between 0.5 and 512 pixels, and the stroke caches its bounds (padded by the radius plus 1 pixel) for itself and for each run of 4 segments (`RUN`). A single tap is one point; no usable sample gives no stroke.

**A block of words** (`annotate/lettering`) is a `Lettering`:

- an opaque **colour**;
- an 8-bit coverage **mask** over a rectangle of the image (`area`, already cut to the image's edges);
- the **frame**, the box of its lines, which is what a press is tested against;
- the **`Words`** it was set from: the text, the origin in whole image pixels and the em in image pixels. They are what lets a placed block be opened again.

The mask is made once, when the words are set, and kept (`Arc<[u8]>`). The Sheet and the composite blend those same bytes, so no font code runs on undo or on the copy thread, and a `Job` shares a block's mask instead of copying it.

**The typesetter** (`lettering/typeset`) sets words into a mask at image resolution, with no window and no graphics card. It owns an off-screen `epaint::Fonts`, built from the window's own font definitions cut down to the semibold family and its fallbacks, and lays the words out at one pixel per point with the font size equal to the em in image pixels. Hinting and sub-pixel positions are off, so every glyph has one bitmap at a whole-pixel place. The coverage is read straight out of that `Fonts`' atlas through each glyph's rectangle (`Proof::pulled` in `lettering/proof.rs`). It is stored as rendered (`FontColorTransferFunction::Off`), which makes it linear like the brush's soft edge, and where the boxes of two glyphs overlap the stronger coverage wins, as with the segments of a stroke.

`Typesetter::set(words, color, image)` returns a `Proof`: the `Lettering`, and the laid-out lines beside it, from which the editor takes the caret's place and the letter nearest a press (`Proof::caret`, `index_at`, `index_above`, `index_below`, `line_start`, `line_end`, all counted in characters). Words of white space alone still give a `Proof`, with no ink, a frame and a caret. Four guards keep a set from garbling or panicking:

- The em is kept between 6 and 320 pixels, and an origin that is not finite is refused.
- The atlas is 4096 pixels a side and holds one letter size at a time; a set at another size starts fresh fonts. If a set fills the atlas, the fonts are thrown away and the set is tried once more on fresh ones. A second overflow sets nothing.
- Words holding a glyph wider than half the atlas are refused.
- The whole set runs inside `catch_unwind`. After a caught panic the fonts are thrown away.

A refused set returns `None`; the editor then keeps the last good result on screen, and that is what gets placed. `Typesetter::rest()`, called each time a block is finished, lets the fonts go when their atlas has grown taller than 512 rows, so one very large block does not leave tens of megabytes behind.

**The rasterizer** (`annotate/raster`) inks one mark into an RGBA buffer inside a clip rectangle. Its entry is `apply(pixels, image, clip, mark)`, which hands a line to `ink` and a block of words to `stamp`.

`ink(pixels, image, clip, stroke)` inks one stroke:

1. It builds an 8-bit **coverage mask** over the stroke's bounds cut to the clip.
2. Every segment is a **capsule**: a pixel's coverage is how far its centre lies inside the brush radius plus a half-pixel soft edge, from 0 to 1. A tap is a capsule of no length, a round dot.
3. Each pixel keeps the **strongest** coverage any segment gave it. Going back over the same line therefore makes it no darker and no thicker.
4. The mask is blended into the buffer **once**: each of red, green and blue moves from what was there toward the ink by the coverage, in plain byte arithmetic. Alpha is left alone.

It only does the work the line needs, which follows the line and not its bounding box:

- Runs of segments whose bounds miss the clip are skipped, and so is a capsule that lies wholly to the left or right of it (`Capsule::is_beside`).
- A capsule visits only the rows it can reach, and in each row only the slice it can reach (`raster/row.rs`). The row's **solid core**, where the brush covers pixels completely, is worked out from the capsule's geometry (`Capsule::core`), checked at both ends by measuring, and filled without measuring the pixels between.
- From the core the row is walked outward a pixel at a time through the soft edge, keeping the stronger coverage, until a pixel gets none. A row with no core starts from the pixel nearest the segment instead.
- Each row remembers the span of columns it touched, and only those are blended.

`stamp(pixels, image, clip, lettering)` (`raster/stamp.rs`) inks a block of words. It walks the block's mask inside its area, cut to the clip and to the image, and moves each pixel's red, green and blue toward the block's colour by the coverage, with the arithmetic of step 4 (the brush's own `lerp`). A pixel with no coverage is not touched, alpha is left alone, and a mask or a buffer of the wrong size changes nothing.

Tests check every pixel of a line against a brute-force maximum over all segments, for a scribble and for 240 random long lines that include nearly level and nearly upright ones; that the number of pixels touched follows the line; that words are mixed exactly as a line is; and that a clip limits the ink of either without changing it. An ignored test in `sheet/timing.rs` prints how long the thickest brush takes to commit and undo on large screenshots, and one in `lettering/typeset/timing.rs` how long words take to set, commit and undo at 60, 200 and 320 pixels per em.

**The Sheet** (`annotate/sheet`) is what the window shows. It holds the untouched screenshot, a working copy of its pixels, the marks in order, a log of the steps done and of the steps undone (`sheet/history.rs`), and a **revision** that grows by one with every commit, replacement, undo and redo.

- `commit(mark)` forgets anything undone, puts the mark on top and inks just that mark.
- `replace(at, mark)` exchanges the mark at one place in the order for another, and repaints the rectangle the two cover: its pixels are restored from the screenshot and every mark is inked again in order, clipped to it. The new mark therefore stays under whatever was made after the old one. It forgets anything undone, like a commit.
- `undo()` takes the last step back and `redo()` puts it forward again. A step is a place in the order and the mark that was there before, or none. Turning a step exchanges the two and hands back its mirror image, so one kind of step undoes and redoes an added mark and a replaced one alike.
- `lift(at)` hides one mark while it is being edited and `settle()` shows it again. Both repaint, and neither is a change: no step is logged and the revision stays. Every change settles a lifted mark first, so a lift cannot outlive the edit it was for.
- `words_at(point)` is the topmost block of words whose frame holds a point, and `words(at)` the block at a place in the order.

A mark is never taken out of the middle of the order, because the steps name places in it. A block of words edited down to nothing is replaced by an empty `Lettering` instead.

Each change returns the `PixelRect` it redrew, and the editor uploads only that part of the texture (`Sheet::patch`, `TextureHandle::set_partial`); `Sheet::image` is the whole picture for the first upload.

**The composite** (`annotate/composite.rs`) is what the clipboard gets: `composite(base, marks)` clones the screenshot and inks every mark in order over the whole image.

**They are the same picture.** The Sheet changes pixels a rectangle at a time and the composite paints everything from scratch, but both go through `raster::apply`, mark by mark in the order made, and a clip never changes a pixel's value. Tests in `composite.rs` compare `Sheet::image()` with the composite after every commit, undo and redo of a scripted session, and through a random session of 240 steps that commits lines and blocks of words, replaces, lifts, settles, undoes and redoes, checking as well that nothing changed outside the rectangle each step reported. Editor tests compare the texture the window uploaded with the composite of the job it would copy: with lines (`screen_matches_clipboard`), with words through undo and redo, and through an edit and a move of a placed block.

While a block is lifted the window and the job differ on purpose: the job keeps describing the picture as committed, with the block on it, and the window shows the block as the one being typed.

## The clipboard

The window does not copy when it opens. The clipboard is first written after the first change.

**Debounce** (`app/annotate/copying.rs`). Each `Changed` outcome re-arms a 250 ms timer (`COPY_AFTER`). When the hand has rested that long, `App::sync_annotator` hands the picture over: `copier.copy(window.job())`. A `Job` is a snapshot of the Sheet: its revision, the screenshot and the marks. Closing the window, or replacing it with another screenshot, hands a waiting copy over at once.

**No wait when the keyboard is elsewhere.** `Copying::drew(revision, now, at_the_keyboard)` makes the copy due at once, not 250 ms later, when the picture changes while the window does not have the keyboard. That is the block of words, or the line, that was finished because the keyboard left: whoever took the keyboard may paste straight away. A copy that was still waiting out its 250 ms when the keyboard left is handed over at once too (`Copying::take_due(now, at_the_keyboard)`).

**A block being typed is not copied.** Like a line being drawn it is not in the Sheet, so it is not in a `Job`. It is copied once, when it is finished.

**The Copier** (`annotate/copier`) owns the `notchbase-annotate-copy` thread, one per open window, so compositing a large screenshot and writing it never stalls a frame:

- **Latest wins.** The worker takes a job and then drains the queue, keeping only the newest. Changes made while a write is in flight collapse into one more write.
- It composites the job and writes it with `arboard`'s `set_image`.
- It reports `done(revision)` or `failed(revision, problem)`: a failure names the picture it was a copy of, just as a success does. A writer that panics is caught and reported as "Couldn't copy the annotated screenshot" for that revision; the worker goes on.
- `Copier::finish` closes the queue and waits for the worker.

**Reports** (`app/annotate/reports.rs`). The worker files each report in a shared list, tagged with the window's session number, and wakes the event loop (see [Architecture](Architecture.md#waking-the-event-loop)). A report is `Copied(revision)` or `NotCopied(revision, problem)`. The next frame reads them (`hear_the_copier` in `app/annotate/frame.rs`):

- **Copied** from the current window updates its status; one from a window that is gone changes nothing on screen.
- **Not copied** from the current window is passed to its status with the revision. The notch says "Couldn't copy the screenshot" only when the picture that failed is still the one in the window: if the window has moved on to a newer picture, that one will report for itself and no notice is raised.
- **Not copied** from a window that has closed raises the notice unless that window's retiring copier still owes a newer picture (`Retiring::owes`): an older copy that failed is not the last word, and the newer one will report for itself.
- Every report is also passed to the retiring copiers (below).

**The status** (`editor/status.rs`) in the title strip never claims more than is true:

| Shows | When |
| --- | --- |
| nothing | nothing has been drawn and nothing copied yet; or a block of words is open that would change the picture when finished |
| "Copying…" | the picture is newer than what the clipboard holds |
| a green check and "Copied" | the clipboard holds exactly the picture in the window; it pops in on a spring |
| a red mark and "Not copied" | the copy of exactly this picture failed, until the picture changes |

The status keeps the newest thing it has heard, a revision the clipboard holds or a revision that was refused. A late "copied" for an older revision leaves the status at "Copying…", and a refusal older than what the status already knows is dropped (`Status::refused`), so a failed copy is blamed only on the picture it was of.

An open block would change the picture (`Typing::would_change`) when it is a new one that holds words, or a placed one that no longer reads as it was placed: other words, another colour, size or place, or no words at all. White space after the last word does not count, since it is trimmed when the block is finished. Then the window shows more than any copy holds, and the status says nothing until the block is finished. An empty new block, and a placed block opened and left as it was, leave the status alone.

### Quitting and closed windows

Closing a window never waits for the clipboard. When a window is closed, or retired for another screenshot, `Annotating::keep_copying` first has the window finish what is in progress (`Annotator::finish_what_is_in_progress`), so a block still being typed joins the picture. A change that comes of it counts as drawn there and then, and a brush the window had still to report is remembered. Then a waiting copy is handed over and, if a picture is still owed, its `Copier` moves to `Annotating::retiring` (`app/annotate/retiring.rs`) together with the session number and the revision awaited. A picture is owed while the last one drawn is newer than both the last one the clipboard took and the last one it refused (`Copying::awaited`), so a last copy that has already failed is not waited for. With nothing owed the copier is simply dropped, which lets its thread end.

A retiring copier is let go as soon as its session reports the awaited revision or a newer one, copied or not.

After the event loop returns, `App::finish_annotating` retires the open window the same way and then joins every copier still retiring (`Retiring::finish`, `Copier::finish`), so quitting right after closing a window, right after the last stroke, or in the middle of typing a block, does not lose that picture.

## The remembered brush

`config.annotate` (`src/config/annotate.rs`) holds the brush between windows and between runs:

```json
"annotate": {
  "color": "#EC545A",
  "size": 5.0
}
```

On `Outcome::Remember(brush)` the app writes `AnnotateConfig::from(brush)`: the colour as upper-case `#RRGGBB` and the size rounded to half a point. A failed save shows "Couldn't remember the brush". Reading is lenient, since people edit the file: the colour is read with or without `#` in any case and falls back to the default red when it is not six hex digits; the size is tidied both as it is read and as it is written (`read_size`, `write_size`): a finite size is pulled into 2 to 32, and one that is not finite becomes the default 5. A number too large for the field, such as `1e39`, reads as infinite and so loads as 5, and since nothing that is not finite can be written, such a value cannot break the next save of the file. The field itself is private; the app reads it through `AnnotateConfig::brush_size`. The brush is read when a window opens, so an edit applies to the next one. See [Configuration](Configuration.md#annotate).

The text tool has no keys of its own: its colour is the brush's and its letters follow the brush's size. Which tool was last used is not remembered.

## Focus

The annotator has to keep the keyboard while you draw, which runs against how the notch treats focus (see [Architecture](Architecture.md#input-click-through-and-keyboard-focus)): on macOS the notch remembers the app in front when the pointer enters it and hands focus back when the pointer leaves, and a capture or a tool text field does the same when it ends.

**Who has the keyboard** (`app/annotate/keyboard.rs`). Each open annotator has a `Keyboard`, fed by every window's `Focused` event (`App::window_event` in `app/handler.rs`) and read as a `Hold`:

| `Hold` | Means |
| --- | --- |
| `NoAnnotator` | no annotator is open |
| `Annotator` | the annotator has the keyboard |
| `Lent` | the annotator had it and another notchbase window has it now: the notch after a click, or a capture overlay after a hotkey |
| `Elsewhere` | the user is in another app (also a new annotator before its first focus) |

A loan is tracked by the borrowing window's id and can pass from one notchbase window to another. A notchbase window that takes the keyboard from another app borrows nothing. Once a frame the loan is settled (`settle_the_keyboard` in `app/annotate/frame.rs`): if the annotator lost the keyboard and no notchbase window took it, or the borrower lost it in turn, or the borrower is no longer open, the keyboard is `Elsewhere`, and the `Keyboard` remembers for good that the user left for another app.

**What focus goes back to** (`app/return_to.rs`). `App::front_app` (remembered when a capture, a shortcut being rebound or a tool text field begins) and each notch window's hover memory hold a `ReturnTo`: `App(FrontApp)` or `Annotator`.

- **Remembering** (`ReturnTo::remembered`). While the hold is `Annotator` or `Lent`, the way back is `Annotator`. Otherwise it is the app in front, exactly as with no annotator.
- **Handing back** (`ReturnTo::resolved`, `App::hand_focus_back`). A remembered app is restored whatever the annotator is doing. An `Annotator` target depends on the hold at that moment: `Lent` gives the keyboard back to the annotator (`Annotating::take_back_the_keyboard`); `Annotator` does nothing, because it already has it; `Elsewhere`, the user having moved to another app meanwhile, and `NoAnnotator`, the window having closed in between, both hand the keyboard to the app in front, so nobody is pulled back to a window they left.

The notch itself behaves the same with or without an annotator: it remembers on the way in (on macOS only) and hands back on the way out, except during a capture (`remembers_who_had_the_keyboard`, `hands_focus_back` in `app/frame.rs`). Only what is remembered differs.

- **Opening.** The press on Annotate happens in the notch, so the notch has just remembered the app that was in front. `App::annotate` turns every notch window's hover memory into `Annotator` (`NotchWindow::lend_hover_to_the_annotator`), so the pointer leaving the notch does not hand focus to that app and put it in front of the new window. A second press for a screenshot that is already open does the same before focusing the window. The app the annotator will return to (`returns_to`) is, in order: the one a replaced annotator would have gone back to (if that window had the keyboard or had lent it), the one a notch had remembered, or the app in front just before the window opened.
- **Clicking the notch, capturing or typing while it is open.** The keyboard is lent. When the pointer leaves the notch, the capture ends or the text field is done, the annotator is focused again. A keyboard that is handed back (`Focus::HandedBack` in `annotate/focus.rs`) never brings a minimized window out and is not given to one; only pressing Annotate again for the same screenshot (`Focus::Asked`) un-minimizes it.
- **Closing** (`App::close_annotator`, `Keyboard::farewell`).
  - With the keyboard: if the user never left for another app while the window was open, focus goes to `returns_to`, the app from opening. If they left and came back, it goes to the app in front at that moment (`platform::remember_front_app`).
  - While the keyboard is lent: nothing is restored at that moment, so typing in the notch is not cut off. The borrower hands the keyboard to the app in front when it finishes, the annotator being gone by then.
  - With the user in another app: nothing is touched.
- **Replacing** it with another screenshot carries the same choice of app over to the new window, also when the keyboard is lent.

Only the hover memory is specific to macOS, where the notch takes focus on a click; the rest runs on every platform.

**Losing the keyboard finishes what is in progress.** Nothing can be pasted anywhere without the annotator losing the keyboard first, so that is the moment a block being typed has to join the picture. On `Focused(false)` the window finishes it at once (`Annotator::hear`), with whatever was typed and not yet read by a frame, the app's next frame hears the `Changed`, and since the window has no keyboard the picture goes to the copier in that same frame (see [The clipboard](#the-clipboard)). Starting a capture, typing in the notch and switching to another app therefore all place the block, and so does a click on the notch on macOS, where the click takes the keyboard. On Windows a plain click on the notch leaves the keyboard with the annotator, so the block stays open until the keyboard really leaves or the block is finished another way. To go on with a placed block, press it again with the text tool.

## Preview and snapshot

`NOTCHBASE_PREVIEW=annotate` opens the annotator at start-up on a sample screenshot (`annotate::sample`, a 1600 x 1000 page of grey lines on a sky gradient), through the same `open_annotator` path as a real press. It is an [app state](Previews.md#app-states), not a pinned notch state: the window is the real one, it writes to the real clipboard, and brush changes are saved to whichever `config.json` is in use.

`annotate/editor/snapshot.rs` has an ignored test that paints the editor without a window and writes five PNGs to `target/snapshots` (or `NOTCHBASE_SNAPSHOTS`):

| File | Shows |
| --- | --- |
| `annotate.png` | a 1600 x 1000 screenshot with two strokes, "Copied", and the brush ring over the picture |
| `annotate-drawing.png` | an ink stroke in progress, the ring at its tip |
| `annotate-small.png` | a 600 x 300 screenshot in the smallest window, with a 14 point green brush and "Copying…" |
| `annotate-plain.png` | the same without the frost: the solid body and the opaque toolbar |
| `annotate-text.png` | the text tool chosen on a 1600 x 1000 screenshot: a placed garnet label, a two-line jade block being typed with its outline and caret, and a jade letter in the well |

```bash
cargo test -p notchbase annotate::editor::snapshot -- --ignored
```

It uses `ui::snapshot::Camera` (`src/ui/snapshot/`), which runs any `FnMut(&mut egui::Ui)` through headless frames and rasterizes the triangles with the same software rasterizer the [notch snapshots](Previews.md#looking-at-a-state-without-a-window) use. A frosted body is composited over a darkened version of the desk gradient standing in for the blur, and on macOS three dots are painted where the traffic lights sit and the editor is told the title bar is 32 points tall, so the pictures show the title line level with the lights.

## Tests

Everything except the real window and the real clipboard is tested headlessly.

- `annotate/stroke`, `raster`, `sheet`, `composite.rs`, `mark.rs`, `rect.rs`, `fit.rs` and `brush.rs` test the pipeline as pure functions: smoothing, flattening to a tenth of a pixel and bounds, coverage and blending, the rows against a brute-force rasterizer, words stamped with a line's arithmetic, undo and redo, replacing, lifting and settling (`sheet/tests/words.rs`), the Sheet and composite equality, clipping, placement and window sizes, the brush's limits and the size of its letters. They build blocks of words from hand-made masks (`testing::lettering`), so none of them needs a font.
- `annotate/lettering` tests the typesetter on egui's bundled fonts, which are the same on every machine: where words land, that twice the em is twice as tall, the line pitch, words cut at the picture's edge, a character no font has, the same words giving the same mask twice, the caret and the letter under a point, and each guard (an atlas too small, a letter too wide, fonts that cannot be used, a tall atlas let go at rest). They assert shapes, bounds and solid interiors, never the bytes of an anti-aliased edge.
- `annotate/editor/tests` drives the whole editor with pointer and key events through `editor/bench.rs` (a `Camera` plus an `Editor`): one press, drag and release draws exactly one line; the line is as wide as the brush at any zoom; swatches, number keys, brackets and the slider change the brush and ask to remember it once; undo and redo by key and by button; every way to close; the status, and a failed copy blamed only on its own picture; and that the window comes to rest. `tests/keys.rs` holds the key contract: Escape held after dropping a line never closes, a held key asks to close once, every way of closing finishes the line in progress, held brackets and undo repeat, and Tab and Space press nothing. `tests/picture.rs` checks that the picture is drawn into its four corners and that the brush ring shows only where a press would draw. `toolbar/tests.rs` renders the ink swatch and a thin ink brush and samples their light edge, and checks the tool control's thumb and the letter in the well. `layout.rs` checks that the title is written on the line of the title bar's buttons.
- `annotate/editor/tests/typing` holds the text tool's contract, driven the same way with `type_text`, `strike` (a key press together with its text, as the real window sends) and `paste`: one click and some typing place exactly one block, reported once when it is finished; digits, brackets and letters type and change neither brush nor tool, a Russian letter on a bracket key included; Shift+Enter breaks the line and Enter finishes; Enter and then Enter closes; Escape held after finishing a block never closes; every way of finishing, losing the keyboard and the calls without a frame among them; a letter typed in the same frame as the key that finishes its block is kept; copy and cut ask nothing of the clipboard; the block is painted from its own mask, only inside the picture, and typeset only when something changed; and the window comes to rest with a block open. `tests/typing/unread.rs` covers a block finished outside a frame with keys still waiting: letters, digits and a line break go into it, a waiting Enter or Escape is spent on it and never closes, a waiting undo still undoes, and the pointer's events are left alone. `tests/typing/reopening.rs` and `reopening/replacing.rs` cover a placed block: opening it with its own words, colour and size; editing it under a line drawn later; undo and redo of the replacement; moving it; emptying it; every way of finishing with and without a change; and the picture on screen against the clipboard through replacements, undo and redo.
- `annotate/copier/tests.rs` runs the worker with an injected writer: a burst writes only the last job, a refusing or panicking writer is reported with its revision and the next job still runs, a job queued before the copier is dropped is still written, and `finish` waits. One ignored test writes to the real clipboard.
- `app/annotate` tests the debounce, a change made while the keyboard is elsewhere being due at once, a picture finished as its window goes being owed at once with its brush kept, and what is still owed to the clipboard, the reports, how a frame's outcomes are read, when a failed copy is announced, the keyboard (borrowing, a loan passing between windows, leaving for another app) and the retiring copiers (quitting right after a close waits for the last copy). `app/return_to.rs` tests what is remembered and what is handed back for every hold, and `app/frame.rs` that the notch remembers and hands back whatever the annotator holds.
- `config/annotate.rs` tests loading old and partial files, the lenient colour, the size tidied as it is read and as it is written (with a real save and reload of a file holding `1e39`), and where the section is written.
- `platform/macos/annotate.rs` tests the collection behaviour and the title bar's height, including a window that reports none.
- `annotate/window.rs` and `clock.rs` test centring, repaint scheduling and the frame clock.
- `notch/notice/tests`, `sdk/src/tests/notice.rs` and the Screenshot tool's tests cover the button itself: see [Notch](Notch.md#tool-notices) and [tools/Testing.md](tools/Testing.md#clicking-things).
