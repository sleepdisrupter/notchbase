# notchbase-paint

The hand-painted UI kit the app and the installer share. notchbase draws almost nothing with egui's own widgets. Shapes, glass, glyphs, controls and motion are painted onto a `Canvas` with the pieces in here, so the installer looks and moves just like the app.

The app uses it through `src/ui`, which re-exports all of it, and the installer for its window, cards, switches, buttons, progress bar and dance. It depends on `egui`, and on `notchbase-spectrum` for the `Bands` the liquid level moves to.

What's in it:

- `canvas`: the shape list everything paints into, with clipping.
- `shape`: the notch body, a squircle with its own top and bottom radii, ears and an optional wave.
- `glass`: lit glass surfaces, and `erase` for the see-through panes behind them.
- `color`, `font` and `metrics`: the palette and its maths, the text styles, and shared spacing and radii.
- `glyph`: vector icons, one file per category, all drawn with one shared `Pen`.
- `buttons`, `controls`, `switch` and `list`: buttons, sliders, rows, the on/off switch and grouped cards.
- `meter`, `liquid` and `wave`: level meters, the liquid music level and the ripples along the notch's bottom edge.
- `spring` and `scroll`: interruptible springs, and wheel and trackpad scrolling shared between nested areas.

Painting is the same on every system. Only `font` differs: it loads the system's faces from `%WINDIR%\Fonts` on Windows and `/System/Library/Fonts` on a Mac, and falls back to egui's own fonts anywhere else. [wiki/Rendering-and-Motion.md](../../wiki/Rendering-and-Motion.md) shows how the app uses the kit.

Run the tests with `cargo test -p notchbase-paint`. They run headless on every system.
