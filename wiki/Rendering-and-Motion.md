# Rendering and motion

notchbase uses egui for input handling, text layout and tessellation, but almost none of egui's widgets. The notch, the pages and the overlays are painted from a small set of primitives: a GL window per surface (`src/ui/gl_window`), a canvas that collects shapes, hand-built meshes for the notch and glass, analytic springs for every animation, system fonts, and vector glyphs. Everything but the GL window, the element renderer, motion presets, pictures, the segmented control, swipes, sheen and trace lives in the `notchbase-paint` crate (`crates/paint/src`), which the installer shares; `src/ui/mod.rs` re-exports its modules, so app code still says `ui::canvas`, `ui::glyph`, `ui::glass` and so on. This page walks through them, naming each module by its `ui::` path and its file.

## The GL window

*Code: `src/ui/gl_window/`.*

`GlWindow` wraps one `winit` window with its own OpenGL context, egui context, `egui_winit` state and `egui_glow` painter. The notch windows, the Windows color loupe, the capture overlays and the recording dim all use it.

**Creation.**

- `glutin_winit::DisplayBuilder` creates the window together with a GL config. `pick_config` scores the available configs, preferring 8-bit alpha, then transparency support, then no multisampling (edges are antialiased by the meshes themselves, see below).
- Transparency is requested explicitly from the GL config on macOS. On Windows the window is made transparent through `winit` and the compositor.
- A desktop GL context is tried first, with an OpenGL ES context as a fallback.
- The swap interval is "wait for vsync" on macOS and "don't wait" on Windows (see pacing below).
- The egui context is set to the dark theme with keyboard zoom disabled, and the system fonts are installed (see fonts).

**Painting.** `GlWindow::paint(run_ui)` runs one egui pass:

1. take the pending input, run the UI closure, and handle platform output (cursor icon, clipboard);
2. upload texture changes (artwork, thumbnails, icons);
3. tessellate the shapes, clear to fully transparent, paint the primitives, free textures;
4. swap buffers and remember that a frame was presented;
5. return egui's requested repaint delay, which the app folds into its wake-up schedule.

Input is normally fed from `winit` window events, but the app can push synthetic events (`push_input`), for example a pointer move computed from the global cursor position, a `PointerGone` when the window becomes click-through, or a button release after a drag-out that the window never saw.

**Frame pacing.** `pace_frame()` is called once at the end of each event-loop pass. If any window presented a frame, on Windows it sleeps until the display's next vertical blank, read from `DwmGetCompositionTimingInfo`, and never longer than two frames (`platform::until_vblank`). It does not use `DwmFlush`, which can block until something else on screen changes. Because Windows windows swap without waiting for vsync, this is what keeps multi-window animation smooth and in step with the display instead of tearing or racing ahead. On macOS vsync in the swap already provides the pacing and `pace_frame` does nothing.

**Profiling.** Set `NOTCHBASE_PROFILE=1` (`src/ui/gl_window/profile.rs`) and every `GlWindow` prints, every 120 frames, the average time per frame spent in the UI pass, tessellation, painting and swapping, labelled with the window's title. The lines look like this (the numbers here are only illustrative):

```
[profile] notchbase: ui 0.41 ms, tessellate 0.22 ms, paint 0.09 ms, swap 0.35 ms
```

## The canvas

*Code: `ui::canvas`, `crates/paint/src/canvas.rs`.*

`Canvas` is a list of egui `Shape`s plus the painter they will go to. Pages and notch layers draw into a canvas rather than directly into the painter, because the whole group is then presented with one transform:

```rust
canvas.paint(&painter, anchor, scale, offset, opacity);
```

scales the shapes about `anchor`, translates them by `offset`, and multiplies their opacity. This is how a page grows from 92% to full size as it is revealed, slides sideways when switching tabs, or fades with its layer.

Two more helpers make fine-grained effects cheap:

- `mark()` and `since(mark)` return the shapes added after a point, so a caller can transform or fade just those (for example the "pop" of a notice's leading element, or a status line that slides in).
- `fade(shape, alpha)` multiplies the colors of any shape kind, including meshes and text.

There are convenience methods for filled and stroked rounded rects, circles, rings, and single-line text anchored with an `Align2`, plus text measurement.

## Shapes

*Code: `ui::shape`, `crates/paint/src/shape.rs`.*

The notch body is not an egui rounded rect. `Blob` describes it: a rect, separate top and bottom radii, ears, a flag to bleed past the top edge, and an optional wave.

- **Continuous corners.** Corners are superellipse arcs rather than circular ones. A corner reaches 1.32 times its nominal radius and its exponent moves between 2 and about 3.1 depending on how much room the side has, which gives the soft "squircle" continuity of Apple's shapes and degrades gracefully to a capsule when the shape is fully rounded.
- **Ears.** When `ear > 0`, the top corners are replaced by concave quarter circles that flare outward, so a notch glued to the screen edge looks like it grows out of the bezel.
- **Bleed.** A shape attached to the top edge extends two points above it, so no seam can show at the top of the screen.
- **Sides.** `Blob::sides_end` is the height where the straight sides start rounding into the bottom corners, and `Blob::side_at` the point where a side crosses a given height, with the direction the outline runs on from there. The web between the notch and an [ear](Notch.md#several-activities-notchlivesidesrs) uses them to meet each shape on its own outline.
- **Wave.** `ui::wave` (`crates/paint/src/wave.rs`) describes up to seven Gaussian ripples along the bottom edge, tapered to zero at the corners. The music dance moves these ripples.

`shape::fill` builds a triangle fan from the outline with a **feathered edge**: each outline point gets an inner vertex with the fill color and an outer vertex half a physical pixel outward that is fully transparent. The result is a crisp, antialiased edge at any scale without multisampling. `fill_polygon_with` lets the color vary per vertex, which glass uses for its gradient.

`shape::band_around` lays a band of a given width just outside a closed outline, and `shape::band_along` lays one along an open stretch of an outline that runs clockwise on screen, on its outer side. Each point gets three vertices along its normal: one half a physical pixel inside the line, so the band tucks under the edge of what it rims without a seam, one half a pixel short of the band's width in the same color, and a transparent one half a pixel past it, the same feather a fill has. The normal at a corner is mitred and capped, so a sharp turn doesn't throw a spike. The island's [keyline](Notch.md#the-keyline-notchkeyliners) is drawn with them.

## Glass

*Code: `ui::glass`, `crates/paint/src/glass.rs`.*

Buttons, sliders and wells use a glass material painted from the same outline:

- a body tinted with the glass color plus a vertical **sheen** gradient that fades from the top down,
- a thin **rim** mesh around the edge, lit from the upper left: edges facing the light get a specular highlight, edges facing away get a soft dark rim.

**See-through glass (`ui::erase`, `crates/paint/src/erase/`, and `src/ui/gl_window/eraser.rs`).** Where the window has a blur behind it (macOS, see [Notch](Notch.md#black-into-glass-notchframers-platformbackdrop)), lit glass lets it show: `Glass::clear` says how much of what was painted underneath a piece of glass is taken away before its body goes on, and the body's tint is thinned to match, so a button reads as a pane of blurred wallpaper with a rim rather than a grey disc. `CONTROL`, `TILE_HOVER`, tinted glass and the page lens are clear; the flat `TILE` is not. Taking paint away is not something egui's blending can do, so `glass::paint` adds a **hole**: a mesh with the reserved texture `erase::TEXTURE` whose vertex alpha is the strength. Being an ordinary mesh it fades, scales and clips with the canvas like everything else. After tessellation `GlWindow::paint` hands the primitives to `eraser::through` (in the app's `gl_window`, since it needs the GL context), which turns each hole into a paint callback that draws it with a small shader of its own and the blend `(ZERO, ONE_MINUS_SRC_ALPHA)`, multiplying what is already in the framebuffer, color and alpha, by one minus the strength. The hole a piece of glass cuts is a **pane** (`erase::pane`, the reserved texture `erase::PANE`, read back in `erase/pane.rs`). It is drawn like any other hole, and `erase::panes` also reads it back out of the tessellated primitives: the rim of each mesh (the edges only one triangle has, walked in order) trimmed to the primitive's clip rect, with the mesh's strongest alpha as its strength. That is the pane as it lands in the window, after the canvas has scaled, slid, faded and clipped it. `GlWindow::paint` keeps the list (`GlWindow::panes`), and the notch window hands it to `platform::Backdrop`, which on macOS dims what shows through each pane (`erase::SMOKE`, see [smoked panes](Platform-macOS.md#the-blur-behind-the-panel-platformmacosbackdroprs)): what is dark behind a button is left as it is, what is bright is held to a dark grey, so a button over a pale wallpaper or a white window is smoked glass and not a bright hole in the black panel. The amount is scaled by `glass::set_seen_through`, which the notch sets every frame (`Notch::glass`: 1 where the window has a blur behind it), so glass stays solid in every other window and on Windows. Clear glass is drawn differently from solid glass so it reads as glass and not as plastic. The body keeps only 6% of its tint, with a 5.5% pearl wash and a faint sheen from the top. The wide lit-and-shadowed bevel gives way to a soft **edge** about two points wide that fades inward (`glass::hairline`): a faint pearl line all the way round, brighter where it faces the light at the top left and half as bright on the opposite side, with no dark line. Inside it a wider, very faint band (`glass::thickness`) lightens the top and shades the bottom, which gives the pane some depth, and a soft shadow sits under it wherever the panel behind is itself see-through. `Glass::lerp` between a solid and a clear glass blends the two rims. Clear glass also answers the pointer: the edge brightens where the pointer is near, with a tighter glint right beside it, the body picks up a soft glow under it, and a button swells by up to a point and a half as the pointer reaches it, so the light slides along a button's edge and across its neighbours as the pointer moves (`POINTER_RIM`, `POINTER_GLINT`, `POINTER_GLOW`, `SWELL`).

Cards are **flat**: `TILE` is a plain fill in `color::SURFACE` with no sheen and no rim (`Glass::flat`), so lists and cards sit quietly in the panel; hovering one blends toward the lit `TILE_HOVER`. `CONTROL` is the lit preset for buttons and wells, `Glass::tinted_by` tints glass toward a color (the drop well, a tool's tinted buttons and running tiles), and `Glass::lerp` blends two for hover transitions. `glass::ring` paints a circular glass ring, with rims on both edges when the glass is lit. `ui/sheen.rs` paints a soft moving highlight band clipped to a rounded rect (used on shelf tiles while an operation runs), and `ui/trace.rs` produces a progress path around a rounded rect.

## Springs

*Code: `ui::spring`, `crates/paint/src/spring.rs`.*

All animation uses one `Spring` type: `value`, `velocity`, `target`, a `SpringConfig { response, damping }` and a settle precision.

- **Response** is the period in seconds of the undamped oscillation (how fast the spring reacts). **Damping** is the damping ratio: below 1 it overshoots and wobbles, 1 is critically damped, above 1 it creeps in.
- `step(dt)` uses the exact analytic solution for the underdamped, critically damped and overdamped cases instead of numerical integration. That makes it stable at any frame rate and **frame-rate independent**: thirty 1/60 s steps land where sixty 1/120 s steps do (there is a test for it).
- A spring is settled when both its distance to the target and its velocity are within its precision. `UNIT_PRECISION` (0.001) is used for 0..1 fades, `POINT_PRECISION` (0.05) for sizes in points.
- `animate_to(target, config)` retargets without a jump, keeping the current velocity, so interrupted animations flow naturally. `snap_to(value)` jumps.

`ui/motion.rs` holds the shared presets:

| Preset | Response, damping | Use |
| --- | --- | --- |
| `HOVER` | 0.18, 1.0 | hover highlights and small fades |
| `SMOOTH` | 0.32, 1.0 | page switches, list heights |
| `BOUNCY` | 0.42, 0.78 | the tab pill |
| `VALUE` | 0.5, 0.88 | numeric readouts |
| `PRESS` | 0.2, 0.8 | press feedback |
| `FOLLOW` | 0.14, 1.0 | things that track the pointer |

It also has `Clock`, a wrapping time accumulator with `wave`, `pulse` and `phase` helpers for looping animations (the drop well's glow, the shelf's working shimmer). The notch's own mode transitions have their own spring table; see [Notch](Notch.md#motion).

Because springs settle, nothing animates forever. When every spring is settled and no continuous animation is running, the app stops requesting frames.

Three small helpers in `src/ui` keep things from swapping in one frame. `handover::Handover` hands one thing over to the next over 450 ms: the old one slides out the way things move and is gone by 40% of the way, while the new one slides in and shows from 25% to 85%, so the two barely overlap; a second change before the first one is seen keeps the old one leaving. `cover::Cover` shows a song's artwork through a change: a new song without artwork yet keeps the old cover for up to 1.2 s, and the new cover dissolves over the old one (the old one fades under a see-through placeholder). `rows::Rows` keeps a list's rows with a 0-to-1 spring each, so new rows grow in place and gone ones fold away where they were (their words wait until there is room); the Agents page, the header's tab icons, the Shelf's tiles, the Notifications page and a tool's settings use it.

## The liquid meter

*Code: `ui::meter`, `crates/paint/src/meter/`.*

Volume sliders and the volume indicator show a **liquid** fill: a `Meter` holds an underdamped "slosh" spring for the level and a ripple phase that runs faster when the level is higher. `meter::paint` draws the liquid surface with a slight lean in the direction it is moving and ripples along its head, clipped to the capsule track. `meter::loudness` maps a linear peak to a perceptual 0..1 level over a 30 dB range, so quiet audio still moves the meter. The notch's music wave (`ui::liquid`, `crates/paint/src/liquid.rs`, which the installer's dance uses too) is built on the same idea: one jiggly spring per spectrum band.

## Fonts

*Code: `ui::font`, `crates/paint/src/font/`.*

The app renders with the system UI fonts, loaded straight from disk at startup:

| | macOS | Windows |
| --- | --- | --- |
| Regular | SF Pro (`SFNS.ttf`, variable, weight 400), then Helvetica | Segoe UI (`segoeui.ttf`) |
| Semibold | SF Pro at weight 590, then Helvetica | Segoe UI Semibold (`seguisb.ttf`) |
| Monospace | SF Mono (`SFNSMono.ttf`, weight 560), then Menlo | Cascadia Mono (weight 600), then Consolas Bold, then Consolas |

`font::system::system_fonts` (`crates/paint/src/font/system.rs`) lists candidate files per role; the first that loads wins, and variable fonts are instantiated at the given weight. egui's bundled fonts stay behind them as fallbacks for missing glyphs. `ui::font` (`font/mod.rs`) installs them (`font::install`) and defines the text styles used everywhere: `headline` (semibold 13), `body` (13), `callout` (12), `subheadline` (11), `section` (semibold 11), `caption` (10) and `value` (monospace 12).

## Glyphs

*Code: `ui::glyph`, `crates/paint/src/glyph/`.*

No icon in the app is an image. `Glyph` is an enum of vector icons (speaker, muted speaker, headphones, display, microphone, device kinds such as keyboard, mouse, gamepad, phone and watch, Bluetooth, chevrons, checkmark, xmark, tray, the shelf operations, gear, note, transport controls, folder, stop, record, pipette, viewfinder, clock, bell, star, grid, gauge, timer, plus, minus, the undo and redo arrows and the brush and text tools of the [annotator](Annotate.md), and the file formats the shelf converts to: photo, camera, globe, layers, frames, app icon, document, film, waveform, bars). `glyph::paint(canvas, glyph, center, size, color)` draws any of them at any size from filled polygons and strokes, with stroke width proportional to the size.

The glyphs are split by category, one file each: `audio.rs` (speakers, headphones, microphones, note, transport), `devices.rs` (display and device kinds, Bluetooth), `chrome.rs` (checkmark, chevrons, xmark, exclamation, plus, minus), `actions.rs` (tray, the shelf operations, gear, folder), `time.rs` (clock, bell, timer, calendar), `system.rs` (bolt, battery, lock, unlock, download, cup, moon, search), `formats.rs` (the shelf's file formats), `edit.rs` (the annotator's undo, redo, brush and text) and `misc.rs` (sun, stop, record, pipette, viewfinder, star, grid, video, cloud, link, gauge, spark, and the `SPARKLES` shape). `glyph/mod.rs` only matches a `Glyph` to its category. Every category draws through one shared `Pen` (`glyph/pen.rs`): it holds the center, size, color and a stroke whose width is a fixed share of the size (never thinner than 1.2 points), maps unit coordinates to points (`p`, `at`), samples arcs (`arc`, 16 steps) and fills polygons at the canvas resolution, and with `arrow` / `arrow_head` draws arrows, so every glyph has the same line weight. `glyph/tests.rs` checks every glyph still draws the same shapes, points and bounds as before the `Pen` existed, paints inside its box, is named once in the table, and takes the color it is given. App icons in the mixer and file thumbnails on the shelf are real images loaded from the OS, and the only image that can be compiled into the app is the [picture](#pictures-uipicturers) a release's What's New notes may carry.

## Pictures (`ui/picture.rs`)

A `Picture` is a PNG compiled into the app with `include_bytes!`, for the few places that show what something looks like instead of drawing it: today the one picture a release's notes may carry at the top of the [What's New](Pages.md#whats-new-pageswhats_new) page, in two variants (an island's and a notch's) under `assets/whats-new/` (see [Updates](Updates.md#giving-a-release-a-picture) for when one earns its place and how it is made). 0.14.0's notes carry none, so no picture is in the app today.

- **Its size is known without decoding.** `Picture::new` is a `const fn` that reads the width and height from the PNG's header, the two big-endian words at bytes 16 and 20, so a `static` picture has its size at compile time and a page's `height` can use it every frame for free. A picture is authored at two pixels to the point: `size()` is its pixels halved, and `fitted(width)` narrows it to a width, keeping its shape and never showing it larger than life.
- **It is decoded once, when first painted.** An `Album` keeps one texture per picture and decodes with the `image` crate the first time `Album::paint` meets it. The album belongs to the page, never to a `static`: a texture belongs to one window's egui context, and every display's notch has its own. `Album::forget` gives the textures back, and a page calls it when the panel hides.
- **It is refused before it can do harm.** A picture larger than 2048 pixels a side (`LARGEST_SIDE`, the limit egui assumes for a texture until a window tells it the GPU's, and so the limit in tests) is not decoded, and one that does not decode is remembered as missing. Either shows as a quiet rounded block of 6% pearl, not a panic.
- **It is sharp at rest.** The frame is snapped to physical pixels before painting, so at 200% one texel covers one pixel, also on a page that has been scrolled or sits at a half-pixel position on Windows at 125%. The texture is filtered linearly with linear mipmaps, which is an exact two by two average at 100% and stays clean at the scales between and while the page is still growing in.
- **It has the app's corners.** egui's textured rect has circular corners, so the picture is a mesh of its own: the same continuous-corner outline and feathered fan as every other shape (`shape::outline`, `fill_polygon_with`), with each vertex's texture coordinate taken from its place in the frame, and the one point pearl hairline (`glass::rim_mesh`) that the app puts round every picture.

Being ordinary meshes on the canvas, pictures fade, scale, slide and clip with their page like everything else.

## Colors and metrics

`ui::color` (`crates/paint/src/color.rs`) holds the palette, and `ui::metrics` (`metrics.rs`) the shared sizes such as `CARD_RADIUS`; and everything notchbase paints uses only it: black for the notch itself, **pearl** `PEARL` (244, 243, 240) for text, icons and the accent, **garnet** `RED` (236, 84, 90), **honey** `YELLOW` (242, 193, 78) and **jade** `GREEN` (63, 196, 138) for status, plus `INK` (14, 14, 15), the near-black written on light fills. The label hierarchy is pearl at fixed alphas (`white(alpha)`; `label`, `secondary_label`, `tertiary_label`, `quaternary_label`), cards are the near-black `SURFACE` (24, 24, 25) and `GRAY` (140, 139, 136) is pearl dimmed. The older names point into the palette (`BLUE`, `PURPLE` and `ACCENT` are pearl, `ORANGE` is honey, `TEAL` is jade), and the SDK's `Tone` colors are the same values, so a tool's tones land in the palette too. `ink_on(fill)` picks ink or pearl for text, checkmarks and glyphs on a filled shape (a pearl button gets ink, a garnet badge gets pearl), and a tool's well in a light tone is drawn as tinted glass so its white symbol stays readable. Colors that come from outside (a calendar's own color on its events and day dots, a color-choice row, a tinted button such as a meeting service's Join) go through `color::harmonize`, which snaps reds and pinks to garnet, oranges and yellows to honey, greens and teals to jade, blues and purples to pearl and nearly grey colors to `GRAY`, keeping their alpha. Only Claude's and ChatGPT's own marks (`CLAUDE`, `CHATGPT`), album art, a swatch showing a picked color and the [pictures](#pictures-uipicturers) on the What's New page keep colors of their own.

## Controls

`ui::controls`, `ui::switch`, `ui::buttons` and `ui::list` (all in `crates/paint/src`) and the app's own `src/ui/segmented.rs` implement the few interactive controls the pages share: the glass slider with an optional live level meter and leading glyph, the switch (`switch.rs`), the blue and glass buttons and text links of the Ask prompts and the installer (`buttons.rs`, with `Tone`), the segmented control, device rows, popup buttons, keycaps for shortcuts, section headers, row highlights, and scroll-wheel notches. `list` paints the settings-style **cards and rows**: `card` paints a flat card and stacks rows of given heights in it, `divider` and `hairline` draw the inset rules between rows, `two_lines` draws a title over a detail (`Lines`, with a tone and a `Cut` for how long text is shortened), and `switch_row` puts a switch at the trailing edge of such a row. `ui::scroll` (`scroll.rs`) is the shared scroll state. They are thin functions over egui's `interact` plus canvas painting, animated with egui's short bool animations for hover. egui's own value animation is a straight line, so the switch's knob and the segmented control's pill ease it with `smoothstep` (the pill per segment it crosses) to start and stop softly, and the segmented control's hover fades like every other hover. A popup button's chevron flips from down to up through flat as its menu opens (`glyph::chevron_turning`), and a settings toggle that can't be used while the page is live (no assistant, updates unavailable) dims to 45%, title and switch together.

## Tool elements (`ui/element`)

Tools describe their tiles, pages, notices and live activities as trees of SDK `Node`s (rows, columns, text, buttons, toggles, sliders, lists, images, timers and so on). `src/ui/element` measures and paints those trees with the same canvas, glass, glyphs and fonts as the rest of the app, and turns interactions back into tool events. See [tools/Elements.md](tools/Elements.md) for the element vocabulary and [tools/Surfaces.md](tools/Surfaces.md) for where each surface appears.

| File | Role |
| --- | --- |
| `mod.rs` | `Env` (style, image store, shortcut views, the shortcut prompt, the time), `Scene` (everything needed to paint one surface), `SurfaceState` (a surface's dropdown and text focus) |
| `measure.rs` | `Measure`: the natural size of any node for a given width, row width distribution, and which nodes fill or flex |
| `layout.rs` | painting containers: `Row`, `Column`, `Frame`, `Card` (a glass card with padding), `List` (glass with divided rows at least 44 points tall), `Adaptive` (one tree for the Notch style, another for the Island), `Keyed` (a stable identity), `Spacer` |
| `leaves.rs` | text, icons, dividers, keycaps, progress bars, rings, swatches, dots, badges, images, and live **timers** that count up or down from a wall-clock time |
| `controls.rs` | buttons, toggles, checkboxes, chips, dropdowns, segmented controls and sliders |
| `drawer.rs` | the dropdown list: it opens **below the block** that holds the dropdown (the tile row, or the page block), pushes the rest of the surface down while it animates open, shows at most four 28 point rows at a time with wheel scrolling, and closes when you pick an option or click elsewhere |
| `input.rs` | single-line text input with focus, caret and placeholder; focusing sends `Action::Typing(true)`; typing sends change events, Enter submits, Escape cancels |
| `shortcut.rs` | a tool's shortcut field: keycaps for the current hotkey, "Add Shortcut", or the "Type shortcut…" prompt while rebinding (see [Pages](Pages.md#rebinding-shortcuts)) |
| `textures.rs` | GPU textures for image nodes, cached by source (up to 32) and refreshed when the image changes |
| `style.rs` | mapping SDK styles to host ones: `Tone` to label or accent colors, `TextStyle` to fonts (title, headline, body, callout, subheadline, section, caption, value, display), and every SDK `Icon` to a `Glyph` |

**Layout.** A row measures its children, lets text and dropdowns shrink when space runs out, and gives spare width to children that flex (sliders, text inputs, progress bars, spacers without a size). A column stacks its children, stretching the ones that fill (rows, columns, cards, lists, dividers, wrapping text). Alignment is start, center or end.

**Events.** A node's handler becomes `Action::Tool(slot, surface, handler, value)` with a `UiValue` (`Press`, `Bool`, `Index`, `Number` or `Text`), which the app delivers to the tool as an `Event::Ui`. Handlers only fire while the surface is interactive; notices and live activities are painted but never interactive.
