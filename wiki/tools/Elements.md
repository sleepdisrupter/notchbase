# Elements

Views are trees of elements. You build them with small functions (`text`, `button`, `row!`…) and builder methods, and return an `Element<Msg>`, a `Tile<Msg>` or a `Live<Msg>`. Everything on this page comes from `notchbase_sdk::prelude::*`, except the `column!` macro, which you import by name:

```rust
use notchbase_sdk::column;
use notchbase_sdk::prelude::*;
```

(`std` has a `column!` macro of its own, so the prelude's glob import can't bring the SDK's one in unambiguously. Importing it by name wins over both.)

Builders turn into `Element<Msg>` with `.into()` or `Element::from(...)`, and any function that takes `impl Into<Element<Msg>>` accepts them directly.

## Styling is notchbase's job

Elements say *what* to show, never exact colors, fonts or sizes of chrome. notchbase draws glass cards, picks fonts, animates hover and press, and adapts everything to the platform and display. You choose among:

### Text styles

| `TextStyle` | Builder shorthand | Use for |
| --- | --- | --- |
| `Title` | `.title()` | Page titles and large headings |
| `Headline` | `.headline()` | Emphasized labels and row titles (tile titles use this) |
| `Body` | default | Ordinary text |
| `Callout` | `.callout()` | Short labels next to controls, and notice text |
| `Subheadline` | `.subheadline()` | Secondary lines, tile footers |
| `Section` | `.section()` | Small section labels |
| `Caption` | `.caption()` | Fine print and hints |
| `Value` | `.value()` | Numbers, sizes and codes such as `#3A7BFF` |
| `Display` | `.display()` | Large monospaced figures such as a running timer |
| `Hero` | `.hero()` | One big figure that is the point of the view, such as the time left inside a timer ring |

### Tones

`Tone` is the palette. `Primary`, `Secondary`, `Tertiary` and `Quaternary` are label levels that follow notchbase's text colors; the rest are fixed colors. The palette is deliberately small, so several names share one color.

| Tone | Meaning |
| --- | --- |
| `Primary` | Normal label color (the default) |
| `Secondary` | Dimmer label |
| `Tertiary` | Faint label |
| `Quaternary` | Fainter still |
| `White`, `Blue`, `Purple` | The same off-white (`#F4F3F0`) |
| `Green`, `Teal` | The same green (`#3FC48A`) |
| `Orange`, `Yellow` | The same yellow (`#F2C14E`) |
| `Red`, `Pink` | The same red (`#EC545A`) |
| `Gray` | Gray (`#8C8B88`) |

So `palette([Tone::Blue, Tone::Purple, Tone::White])` is three identical circles. To tell things apart by color, take one tone from each row, or use raw colors (`Color::rgb(r, g, b)`).

`tone.color()` (or `Color::from(tone)`) gives a tone's RGBA value.

### Colors

`Color` is for things that show an actual color: swatches and dots. Anything that takes `impl Into<Color>` also accepts a `Tone`.

```rust
Color::rgb(58, 123, 255)
Color::rgba(255, 255, 255, 128)
Color::white(0.5)
Color::rgb(58, 123, 255).hex()
```

`hex()` returns `"#3A7BFF"`. The fields `r`, `g`, `b`, `a` are public `u8`s.

### Icons

`Icon` is a fixed set of glyphs drawn by notchbase:

`Speaker`, `SpeakerMuted`, `Headphones`, `Display`, `Microphone`, `MicrophoneMuted`, `Device`, `Checkmark`, `ChevronDown`, `ChevronUp`, `ChevronLeft`, `ChevronRight`, `Xmark`, `Plus`, `Minus`, `Tray`, `Folder`, `Exclamation`, `Convert`, `Compress`, `Combine`, `Split`, `Gear`, `Note`, `Play`, `Pause`, `Stop`, `Record`, `Backward`, `Forward`, `Keyboard`, `Mouse`, `Gamepad`, `Phone`, `Watch`, `Bluetooth`, `Pipette`, `Viewfinder`, `Clock`, `Bell`, `Star`, `Grid`, `Gauge`, `Timer`, `Calendar`, `Video`, `Cloud`, `Link`, `Spark`, `Bolt`, `Battery`, `Lock`, `Unlock`, `Download`, `Cup`, `Moon`

## Where controls respond

Controls respond on a **tile**, a **page**, in a live activity's [`controls`](Surfaces.md#live-activity) and in a notice's [controls](Surfaces.md#controls). Elsewhere in a **live activity** or a **notice** they are drawn but don't respond: the closed notch opens when hovered instead. A control without a handler is drawn but inert; buttons and checkboxes are also dimmed.

Scrolling follows the pointer and the direction, so controls work the same with a mouse wheel or a trackpad: sliders turn with a sideways trackpad swipe or a wheel, picker wheels and dropdown lists with scrolling up and down, and a [month](#month) or [week](#week) with `on_scroll` turns one page per swipe. A page taller than the notch scrolls with whatever no control took, and a sideways swipe elsewhere switches tabs. See [Notch](../Notch.md#scrolling).

---

## Text

```rust
text("Now playing").headline()
text("12 files").subheadline().secondary()
text("#3A7BFF").value()
text("12:30").display()
text("Warning").tone(Tone::Orange)
text("A long description that wraps onto several lines").wrap().align(Align::Center)
text("Explicit style").style(TextStyle::Body)
```

| Method | Effect |
| --- | --- |
| `text(impl Into<String>)` | Body text in the primary tone, left aligned, on one line |
| `.style(TextStyle)` | Set the style |
| `.title()` `.headline()` `.callout()` `.subheadline()` `.section()` `.caption()` `.value()` `.display()` | Style shorthands |
| `.tone(Tone)` | Set the color |
| `.secondary()` `.tertiary()` | Tone shorthands |
| `.wrap()` | Wrap onto as many lines as needed, filling the available width |
| `.align(Align)` | `Start` (default), `Center` or `End` |

Unwrapped text stays on one line and is shortened with "…" when it doesn't fit.

## Icon

```rust
icon(Icon::Bell)
icon(Icon::Record).size(12.0).tone(Tone::Red)
```

| Method | Effect |
| --- | --- |
| `icon(Icon)` | A 15 pt glyph in the primary tone |
| `.size(f32)` | Glyph size |
| `.tone(Tone)` | Glyph color |

A bare `Icon` also converts into an element: `row![Icon::Folder, text("Downloads")]`.

## Button

```rust
button("Save").filled().on_press(Msg::Save)
button("Refresh").icon(Icon::Convert).on_press(Msg::Refresh)
button("Details").plain().on_press(Msg::Details)
button("Delete").destructive().on_press(Msg::Delete)
button("Send").enabled(self.ready).on_press(Msg::Send)
```

| Method | Effect |
| --- | --- |
| `button(label)` | A glass capsule button |
| `.icon(Icon)` | Glyph before the label. An empty label gives an icon-only button: `button("").icon(Icon::Xmark)` |
| `.style(ButtonStyle)` | `Glass` (default), `Filled`, `Plain`, `Destructive` or `Tinted(Color)` |
| `.filled()` | Accent-tinted, for the main action |
| `.plain()` | No background, accent-colored text; for secondary actions in rows and lists |
| `.destructive()` | Red-tinted |
| `.tinted(impl Into<Color>)` | Tinted with any color, white label; for an action with its own color, such as a Zoom link in Zoom blue |
| `.on_press(Msg)` | Message sent on click |
| `.enabled(bool)` | `false` dims the button and ignores clicks |

A button without `on_press` is dimmed too.

## Toggle

```rust
row![text("Wi-Fi"), spacer(), toggle(self.wifi).on_toggle(Msg::Wifi)]
```

| Method | Effect |
| --- | --- |
| `toggle(on: bool)` | A switch |
| `.on_toggle(Fn(bool) -> Msg)` | Called with the new state when clicked |

The switch shows what you pass in. Store the new state in `update`, or the switch flips back.

## Checkbox

```rust
checkbox("Remember this folder", self.remember).on_toggle(Msg::Remember)
```

| Method | Effect |
| --- | --- |
| `checkbox(label, checked: bool)` | A checkbox with a label |
| `.on_toggle(Fn(bool) -> Msg)` | Called with the new state when clicked |

## Chip

A capsule that lights up when on, for compact on/off options.

```rust
row![
    chip("Mic", self.mic).icons(Icon::Microphone, Icon::MicrophoneMuted).on_toggle(Msg::Mic),
    chip("Sound", self.sound).icon(Icon::Speaker).on_toggle(Msg::Sound),
]
.spacing(6.0)
```

| Method | Effect |
| --- | --- |
| `chip(label, on: bool)` | A chip |
| `.icon(Icon)` | Glyph shown in both states |
| `.icons(on: Icon, off: Icon)` | Different glyphs for on and off |
| `.on_toggle(Fn(bool) -> Msg)` | Called with the new state when clicked |

A chip with an empty label and an icon is a small round button showing just the icon.

## Dropdown

```rust
dropdown(["Small", "Medium", "Large"], Some(1)).on_select(Msg::Size)

let names = self.inputs.iter().map(|input| (Icon::Microphone, input.name.clone()));
dropdown(names, self.selected).placeholder("Default Microphone").on_select(Msg::Choose)
```

| Method | Effect |
| --- | --- |
| `dropdown(options, selected: Option<usize>)` | A pop-up button. Options can be `&str`, `String`, or `(Icon, label)` pairs |
| `.placeholder(text)` | Shown when nothing is selected |
| `.on_select(Fn(usize) -> Msg)` | Called with the chosen option's index |
| `.compact()` | A small round button showing only the chosen option's icon, lit while something is chosen (the first option's icon, unlit, while nothing is) |

The button shows the selected option's label, or the placeholder. The options open in a drawer below it. A dropdown with no options or no `on_select` doesn't open.

## Segmented

```rust
segmented(["Day", "Week", "Month"], self.range).on_select(Msg::Range)
```

| Method | Effect |
| --- | --- |
| `segmented(options, selected: usize)` | Equal-width segments, one selected |
| `.on_select(Fn(usize) -> Msg)` | Called with the chosen segment's index |

## Slider

```rust
slider(self.volume).icon(Icon::Speaker).on_change(Msg::Volume)
slider(self.percent).range(0.0, 100.0).step(5.0).on_change(Msg::Percent)
```

| Method | Effect |
| --- | --- |
| `slider(value: f32)` | A slider over `0.0..=1.0` |
| `.range(min, max)` | A different range |
| `.step(f32)` | Snap values to multiples of `step` from `min` |
| `.icon(Icon)` | Glyph at the leading end |
| `.on_change(Fn(f32) -> Msg)` | Called while dragging, whenever the value changes |

In a row, a slider stretches to take the spare width.

## Picker

Wheels you spin, like the time picker on a phone.

```rust
picker([
    wheel(hours, self.hours).unit("hours").looping(),
    wheel(minutes, self.minutes).unit("min").looping(),
])
.on_change(Msg::Pick)
```

| Method | Effect |
| --- | --- |
| `picker(columns)` | Side-by-side wheels sharing one selection band, 68 points per wheel and five rows tall |
| `wheel(options, selected: usize)` | One wheel of options |
| `.unit(text)` | A label that sits in the band beside the chosen option, such as "min" |
| `.looping()` | The wheel wraps around from the last option to the first |
| `.on_change(Fn(usize, usize) -> Msg)` | Called with the wheel's index and the chosen option's index |

notchbase does the physics. Dragging moves the wheel under the pointer, a flick carries on with momentum and springs onto the nearest option, pressing a moving wheel catches it, the scroll wheel steps one option per notch, and clicking an option above or below the band rolls to it. Options curve away on a drum and fade toward the edges, and the band flashes softly as each option passes, with a light tick on a Mac trackpad for each one (while a finger is on it, and unless Haptics is off in Settings). A wheel that doesn't loop stretches a little past its ends and springs back. You hear about a choice once the wheel lands on it, not while it spins, and the wheel waits for your view to report the new `selected` instead of springing back.

## Month

```rust
month(2026, 9)
    .today(30)
    .selected(self.day)
    .mark(14, Tone::Blue)
    .mark(14, Color::rgb(255, 45, 85))
    .monday_first(true)
    .on_select(|year, month, day| Msg::Pick(year, month, day))
```

A month calendar: a row of weekday initials over six weeks of days, taking the full width. Days from the months before and after fill the first and last weeks, dimmed; they can be picked too, and report their own month, so your tool can turn to it. Today is a red circle with a white number, the selected day a soft light circle, and other days light up under the pointer. It moves like liquid: when the selection changes, its circle flows to the new day, its front arriving first while the back stretches out behind it into a droplet and pulls in; when the month changes, both months are on screen for a moment: the old one slides away, shrinks a little and fades, while the new one glides in from the other side on a spring, settling with the faintest overshoot. Each day shows up to three colored dots below its number, one per `mark`, for example one per calendar with an event that day. The Calendar tool uses it for its page.

| Method | Effect |
| --- | --- |
| `month(year, month)` | The month to show, `month` from 1 to 12 |
| `.today(day)` | Mark this day of the month as today. Leave it out when today isn't in this month |
| `.selected(day)` | Highlight this day |
| `.mark(day, color)` | Add a dot under a day; call it again for more (three at most are drawn) |
| `.monday_first(bool)` | Start the week on Monday; Sunday by default |
| `.on_scroll(Fn(i32) -> Msg)` | Called with `1` or `-1` when the pointer scrolls over it: down or a swipe to the left is `1`. A trackpad swipe counts once however long it glides, and each notch of a mouse wheel counts once. Turn to the next or previous month |
| `.on_select(Fn(i32, u8, u8) -> Msg)` | Called with the year, month and day that was clicked, which may be in the month before or after |

Moving between months is up to your tool, for example with chevron buttons beside a title. The [test harness](Testing.md) clicks a day of the shown month with `pick_day(day)`, or any date with `pick_date(year, month, day)`.

## Week

```rust
week(days.map(|date| WeekDay {
    year: 2026,
    month: 9,
    day: date,
    weekday: "Wed".into(),
    today: date == 30,
    selected: date == self.day,
    events: vec![WeekEvent { id: "standup".into(), time: "10a".into(), title: "Standup".into(), color: None }],
}))
.on_day(|year, month, day| Msg::Pick(year, month, day))
.on_event(Msg::Open)
```

A week as columns, one per day from left to right, taking the full width: the weekday and date at the top (today in red), then that day's events stacked below as small rounded blocks, tinted with their color, showing the time you give (keep it short, like `10a`) over a title of up to two lines (three for an event without a time). Columns share the width evenly, so pass as many days as leave room to read (the Calendar passes four); each fits four blocks, and a busier day shows three and "+2". The element is as tall as its busiest day needs, two to four blocks. The selected day has a soft highlight that flows between columns like liquid, stretching as it travels, and columns light up under the pointer. When the days change to a later or earlier set (compared by the first day), the old ones slide away and the new ones glide in, like a [month](#month) turning. The element is at least two blocks tall, so paging between quiet and busy days moves the notch less.

| Method | Effect |
| --- | --- |
| `week(days)` | The days to show, left to right, each a `WeekDay { year, month, day, weekday, today, selected, events }` |
| `.on_day(Fn(i32, u8, u8) -> Msg)` | Called with the date of the column that was clicked |
| `.on_event(Fn(String) -> Msg)` | Called with the `id` of the block that was clicked |
| `.on_scroll(Fn(i32) -> Msg)` | Called with `1` or `-1` when the pointer scrolls over it, as for [month](#month); page to the next or previous days |

The Calendar tool uses it for its default Days view. The [test harness](Testing.md) taps a column with `tap_week_day(year, month, day)` and a block with `tap_week_event(id)`.

## Round button

```rust
round_button("Start").tone(Tone::Green).on_press(Msg::Start)
round_button("Cancel").enabled(self.running).on_press(Msg::Cancel)
```

| Method | Effect |
| --- | --- |
| `round_button(label)` | A 64 point circular button in gray, with a thin ring around it |
| `.tone(Tone)` | Tints the glass and the label; gray and the label tones keep a white label |
| `.size(f32)` | Diameter |
| `.on_press(Msg)` | Sent when clicked |
| `.enabled(bool)` | A disabled button dims and can't be pressed |

Pressing squashes the button, and letting go springs it back past its size and settles.

## Text input

```rust
text_input(&self.name).placeholder("Name").on_change(Msg::Rename).on_submit(Msg::Save)
```

| Method | Effect |
| --- | --- |
| `text_input(value)` | A one-line text field showing `value` |
| `.placeholder(text)` | Shown while empty |
| `.on_change(Fn(String) -> Msg)` | Called with the whole text after every edit (typing, pasting, deleting) |
| `.on_submit(Msg)` | Sent when the user presses Enter; editing ends |

Clicking the field starts editing. While editing, notchbase keeps its own copy of the text, starting from `value`. Escape or a click elsewhere ends editing. The typed text only reaches your tool through `on_change`, so store it in the model and pass it back as `value`; `on_submit` carries no text.

In a row, a text field stretches to take the spare width.

## Shortcut

Shows one of your tool's shortcuts as keycaps, and lets the user rebind it.

```rust
row![text("Start or stop").callout(), spacer(), shortcut("toggle")]
```

`shortcut(id)` takes a shortcut id from your manifest. Clicking the keycaps records a new combination; while recording, a × button clears the binding. An unbound shortcut shows **Add Shortcut**, and an id that isn't in your manifest shows **Unknown shortcut**. `Tile::shortcut(id)` puts the same field at the bottom of a tile, where it rests as a small caption such as `Ctrl+Shift+C` and turns into the keycaps when the tile is hovered. See [Settings and Shortcuts](Settings-and-Shortcuts.md#shortcuts).

## Keycaps

Static keycaps for hints. They don't respond to clicks.

```rust
row![text("Press").caption(), keycaps(["Ctrl", "Alt", "F"])].spacing(6.0)
```

## Progress

```rust
progress(0.4)
progress(self.done).tone(Tone::Green)
```

| Method | Effect |
| --- | --- |
| `progress(value: f32)` | A thin bar, `value` from `0.0` to `1.0` (clamped). White by default |
| `.tone(Tone)` | Fill color |

In a row, a progress bar stretches to take the spare width.

## Ring

```rust
ring(0.72)
ring(self.progress).size(20.0).tone(Tone::Orange)
```

| Method | Effect |
| --- | --- |
| `ring(value: f32)` | A circular progress ring, 13 pt, green |
| `.size(f32)` | Diameter |
| `.tone(Tone)` | Arc color |
| `.stroke(f32)` | Line width; wider lines get rounded ends |
| `.counting_down(start: u64, end: u64)` | Ignore `value` and drain smoothly from full at `start` to empty at `end` (Unix milliseconds), redrawn every frame by notchbase |
| `.center(element)` | Something to show in the middle, such as a `timer` |

```rust
ring(1.0).size(128.0).stroke(6.0).tone(Tone::Orange).counting_down(started, ends).center(timer(ends).remaining())
```

## Swatch

A color sample with a glass rim.

```rust
swatch(self.picked)
swatch(Color::rgb(58, 123, 255)).size(28.0)
swatch(Tone::Purple)
```

| Method | Effect |
| --- | --- |
| `swatch(impl Into<Color>)` | A 20 pt circle of that color |
| `.size(f32)` | Diameter |

## Palette

```rust
palette([Tone::Red, Tone::Orange, Tone::Green, Tone::Blue])
    .selected(Some(self.color))
    .on_select(Msg::Color)
```

A row of color circles to choose from, with a white ring around the chosen one that glides to the next choice. Circles grow a little under the pointer. Each takes 28 points, so nine fit beside a label on a page.

| Method | Effect |
| --- | --- |
| `palette(colors)` | Anything that turns into [`Color`](#swatch): tones or exact colors |
| `.selected(Option<usize>)` | The ringed circle; `None` rings nothing |
| `.on_select(Fn(usize) -> Msg)` | Called with the index of the circle that was clicked |

The [test harness](Testing.md) picks one with `pick_color(index)`. The Calendar tool uses it for an event's color.

## Dot

A plain filled circle, such as a recording light.

```rust
dot(Tone::Red, 9.0)
dot(Color::rgba(255, 255, 255, 128), 6.0)
```

`dot(color: impl Into<Color>, size: f32)`

## Badge

A filled circle in a tone with a white glyph inside.

```rust
badge(Icon::Checkmark, Tone::Green, 18.0)
```

`badge(icon: Icon, tone: Tone, size: f32)`

## Image

```rust
image(ImageSource::Capture(capture.image)).size(48.0, 30.0).radius(6.0)
image(ImageSource::File("C:/Users/me/Pictures/cat.png".into()))
```

| Method | Effect |
| --- | --- |
| `image(ImageSource)` | A 30×20 pt image with 5 pt corners |
| `.size(width, height)` | Displayed size |
| `.radius(f32)` | Corner radius |

`ImageSource::Capture(ImageId)` shows a screen capture you received from [`Command::capture_screen`](Commands.md#capture_screen). `ImageSource::File(path)` loads an image file. notchbase shows images as small thumbnails, so use them for previews rather than full-size pictures. While an image can't be shown (a missing file, an old capture), a placeholder takes its place.

## Timer

A clock that notchbase keeps running on its own.

```rust
timer(self.started_at)
timer(self.ends_at).remaining().style(TextStyle::Display).tone(Tone::Orange)
```

| Method | Effect |
| --- | --- |
| `timer(at: u64)` | Time elapsed since `at` (Unix milliseconds), counting up |
| `.remaining()` | Time left until `at`, counting down and stopping at `0:00` |
| `.clock()` | The local time of day at `at`, such as `5:42 PM` or `17:42`, following the system's 12 or 24 hour setting |
| `.style(TextStyle)` | Text style, `Value` by default |
| `.tone(Tone)` | Color |

It reads like a stopwatch: `0:42`, `12:05`, `1:02:05`. Because notchbase redraws it every second by itself, you don't need a ticking subscription to keep it current. Get timestamps with `now_ms()`.

## Spacer, space, divider

```rust
row![text("Left"), spacer(), text("Right")]
row![text("A"), space(20.0), text("B")]
column![text("Above"), divider(), text("Below")]
```

| Function | Effect |
| --- | --- |
| `spacer()` | Takes all spare room in a row, pushing its neighbors apart |
| `space(size)` | A fixed gap of `size` points |
| `divider()` | A hairline across the available width |

## Rows and columns

```rust
let header = row![icon(Icon::Folder), text("Downloads").headline()].spacing(8.0);
let body = column![text("12 files").secondary(), text("1.2 GB").value()].spacing(2.0);
column![header, body].spacing(10.0).push(divider()).push(text("Footer").caption())
```

| Item | Effect |
| --- | --- |
| `row![a, b, c]` | Children side by side. Accepts anything that converts into an element, mixed freely |
| `column![a, b, c]` | Children stacked top to bottom. Import with `use notchbase_sdk::column;` |
| `row(children)` / `column(children)` | Same, from an iterator of `Element<Msg>` |
| `.spacing(f32)` | Gap between children (notchbase's standard gap by default) |
| `.align(Align)` | Row: vertical alignment, `Center` by default. Column: horizontal alignment, `Start` by default |
| `.push(child)` | Append a child |

The function forms take elements, so convert each item:

```rust
let lines = self.items.iter().map(|item| text(item).into());
column(lines).spacing(4.0)
```

### How space is shared

- **In a row**, children get their natural width. `spacer()`, sliders, text inputs and progress bars stretch to share what's left. When the children don't fit, text and dropdowns shrink first, shortening their labels with "…".
- **In a column**, rows, columns, cards, lists, pans, stretched canvases, dividers, sliders, text inputs, progress bars, wrapped text, `spacer()` and frames without a fixed width take the full width. Everything else keeps its natural width and is placed by the column's alignment.

## Card

A glass card around its content, filling the available width.

```rust
card(column![text("Today").section(), text("3 sessions").headline()]).padding(16.0)
```

`card(child)` returns an `Element`. `.padding(f32)` sets the inner padding (12 pt by default).

## List

A grouped list: one glass background, rows separated by hairlines. Good for collections and settings-style rows.

```rust
let rows = self.items.iter().enumerate().map(|(index, name)| {
    keyed(name.clone(), row![text(name), spacer(), button("Open").plain().on_press(Msg::Open(index))])
});
list(rows)
```

`list(children)` takes an iterator of elements. Each child is one row, inset from the edges and at least a standard row tall.

## Adaptive

notchbase is drawn two ways: as a **notch** around the camera housing on displays that have one, and as a floating **island** everywhere else (Windows, external displays). The two have different room, especially for live activities and notices.

```rust
adaptive(swatch(color).size(18.0), row![swatch(color).size(20.0), text("Copied").callout()].spacing(6.0))
```

`adaptive(notch, island)` shows `notch` on a notched display and `island` otherwise.

## Keyed

```rust
keyed(item.id.clone(), row![text(&item.name), spacer(), toggle(item.on).on_toggle(move |on| Msg::Item(index, on))])
```

`keyed(key, child)` gives an element a stable identity. notchbase keeps some state per element: hover and press animations, an open dropdown, the text being typed. It normally ties that state to the element's position, so when items in a list are added, removed or reordered the state can end up on the wrong item. A unique key per item keeps it attached to the right one.

## Pan

```rust
let hours = row(self.hours.iter().map(|hour| text(hour).width(CELL))).spacing(0.0);
pan(hours).offset(self.slid).on_pan(Msg::Dragged).on_tap(Msg::Clicked)
```

A strip that can be wider than the page: it takes the full width, keeps its child at the child's own width, centers it, and cuts off whatever sticks out at either side. Dragging it, swiping sideways on a trackpad or turning a mouse wheel over it tells your tool how far; your tool decides what that means and moves the child with `.offset`.

| Method | Effect |
| --- | --- |
| `pan(child)` | The child keeps its natural width, so give it fixed-width parts (no `spacer()`, and a width on any `card`) |
| `.offset(f32)` | Shifts the child sideways, in points. Positive moves it right |
| `.on_pan(Fn(f32) -> Msg)` | Called while it is dragged or scrolled, with the points moved since the last call. Positive means the content should move right |
| `.on_tap(Fn(f32) -> Msg)` | Called on a click that wasn't a drag, with how far the click was from the child's left edge, in points |

For an endless strip, keep a position in your model, draw a few more cells than fit either side of it, and put the leftover fraction of a cell in `.offset`. Several pans given the same offset move as one. An upward swipe is left to the page.

The [test harness](Testing.md) drags the `n`th one with `pan(n, by)` and clicks it with `tap_pan(n, at)`.

## Canvas

When the ready-made elements can't show what you want, draw it yourself. A canvas is a blank area with its own coordinates, in points from its top left corner, that you fill with shapes. Shapes are painted in the order you add them, and anything past the edge is cut off.

```rust
canvas(340.0, 120.0)
    .push(draw::rect(0.0, 0.0, 340.0, 120.0).radius(18.0).fill(draw::linear(NIGHT, DAWN, 90.0)))
    .push(draw::circle(280.0, 34.0, 16.0).fill(draw::radial(Tone::White, Tone::Yellow)))
    .push(draw::line(draw::curve((0.0, 90.0), (90.0, 50.0), (220.0, 130.0), (340.0, 80.0))).stroke(1.5, Tone::White))
    .push(draw::text("12:06", 20.0, 22.0).size(28.0).semibold())
    .stretch()
    .on_pointer(Msg::Pointer)
```

| Method | Effect |
| --- | --- |
| `canvas(width, height)` | The size of the drawing, and of the element unless it stretches |
| `.push(shape)` / `.extend(shapes)` | Add shapes, painted in order |
| `.stretch()` | Scale the whole drawing to fill the available width, keeping its proportions. Draw for one width (a page is 340 points wide) and it fits wider notches too |
| `.on_pointer(Fn(Pointer) -> Msg)` | Hear the pointer over the canvas |
| `.on_scroll(Fn(Pointer) -> Msg)` | Hear scrolling over it, in both directions. Only set it if you use it: the page under a canvas that listens no longer scrolls there |

### Shapes

Shapes come from the `draw` module, which the prelude brings in. Each returns a `Shape` you refine with methods; a method that doesn't apply to a shape leaves it alone.

| Shape | Draws | Refine with |
| --- | --- | --- |
| `draw::rect(x, y, width, height)` | A rectangle | `.radius` `.fill` `.stroke` |
| `draw::circle(x, y, radius)` / `draw::ellipse(x, y, across, down)` | A circle or ellipse around a center | `.fill` `.stroke` |
| `draw::line(points)` | A line through `(x, y)` points | `.stroke` |
| `draw::polygon(points)` | A closed shape through the points, of any outline | `.fill` `.stroke` |
| `draw::text("…", x, y)` | Text at any size | `.size` `.semibold()` `.mono()` `.color` `.anchor` `.centered()` `.wrap(width)` |
| `draw::run(parts, x, y)` | Texts and icons side by side as one line, each at its own size, so you never need to know how wide a word is | `.anchor` `.gap` |
| `draw::icon(Icon, x, y)` | One of notchbase's [icons](#icons), centered on the point | `.size` `.color` |
| `draw::image(source, x, y, width, height)` | A picture, as [`image`](#image) takes it | `.radius` |
| `draw::glass(x, y, width, height)` | notchbase's own card surface | `.radius` |
| `draw::shadow(x, y, width, height)` | A soft blur of color: a drop shadow, or a glow in a bright color | `.radius` `.blur` `.color` |
| `draw::group(shapes)` | Several shapes treated as one | `.opacity` `.offset` `.scale` `.clip` |

`.fill` takes a color (a `Tone` or an exact `Color`) or a gradient: `draw::linear(from, to, angle)` runs across the shape (0 is left to right, 90 top to bottom), `draw::radial(inner, outer)` runs from its middle to its edge. `.stroke(width, color)` outlines. `.anchor(Anchor::…)` says which point of a text sits at its `x, y`: `TopLeft` by default, or any edge, corner or `Center`.

`.opacity(0..1)`, `.offset(x, y)`, `.scale(by)` and `.clip(x, y, width, height)` work on any shape by wrapping it in a group, so `draw::text("Hi", 0.0, 0.0).opacity(0.5).offset(10.0, 4.0)` is one faded, moved label. Inside a group, shapes are scaled first and moved second, and the clip is in the group's own coordinates.

A tool can't measure text, so to put a small label right after a big one use a run: `draw::run([draw::text("10:57", 0.0, 0.0).size(17.0).semibold(), draw::text("pm", 0.0, 0.0).size(10.0)], 340.0, 11.0).anchor(Anchor::Right)` lays the parts out left to right, centered on one line, and places the whole line by its anchor. The parts' own positions are ignored.

For curves, make the points with `draw::arc(x, y, radius, from, to)` (degrees, clockwise from the right) or `draw::curve(from, pull, push, to)` (a Bézier curve), then draw through them with `line`, or close them with `polygon`.

### Pointer

`on_pointer` gets a `Pointer { kind, x, y, dx, dy }` in the canvas's coordinates, whatever size it is drawn at.

| `PointerKind` | When |
| --- | --- |
| `Hover` | The pointer came over the canvas, or moved over it with no button down |
| `Leave` | It left |
| `Press` | The button went down |
| `Drag` | It moved with the button down; `dx`, `dy` are how far since the last report |
| `Release` | The button came up |
| `Click` | A press and release without a drag, after its `Release` |
| `Scroll` | Sent to `on_scroll`: `dx`, `dy` are how far the content should move |

`pointer.within(x, y, width, height)` and `pointer.near(x, y, radius)` answer "is it over this?", so hit testing is a line per target.

### Motion

A canvas is drawn again whenever your view changes. To animate, keep the time in your model and tick while something moves:

```rust
fn subscriptions(&self) -> Subscription<Msg> {
    if self.moving { Subscription::every(Duration::from_millis(16), Msg::Frame) } else { Subscription::none() }
}
```

On each `Msg::Frame` read `now_ms()`, move what should move, and stop ticking once it rests. Never tick while nothing changes.

Colors here are yours to choose, and nothing stops a canvas looking out of place. Tones, the `glass` shape and notchbase's icons keep it at home on the notch. The [test harness](Testing.md) sends pointers with `pointer(n, Pointer::at(PointerKind::Click, x, y))`, and `shows` and `texts` read a canvas's text.

## Sizing

The `Sizing` trait (in the prelude) works on anything that converts into an element:

| Method | Effect |
| --- | --- |
| `.width(w)` | Fix the width; the content is centered inside |
| `.height(h)` | Fix the height |
| `.frame(w, h)` | Fix both |
| `.padding(p)` | Inner padding of a `card`. It has no effect on other elements |

```rust
chip("Mic", self.mic).on_toggle(Msg::Mic).width(81.0)
text("Label").height(30.0)
icon(Icon::Star).size(24.0).frame(32.0, 32.0)
```

`icon`, `swatch`, `ring` and `image` have their own `.size()` for how big they draw; `.frame(w, h)` sets the box they sit in, so the two never clash.

### Mapping a child view

`Element::map` turns a child component's view into the parent's message type, the same way `Command::map` and `Subscription::map` do:

```rust
fn page(&self) -> Option<Element<Msg>> {
    Some(self.timer.view().map(Msg::Timer))
}
```

## View helpers

Element builders are generic over the message type, so helper functions can be too:

```rust
fn heading<Msg: Clone + 'static>(title: &str, on_open: Msg) -> Element<Msg> {
    row![text(title).headline(), spacer(), button("Open").plain().on_press(on_open)].into()
}
```

## Raw nodes

Every element becomes a `protocol::Node` on its way to notchbase. If you ever need to, you can build one yourself and wrap it with `Element::node(node)` (or `.into()`):

```rust
use notchbase_sdk::protocol::{Node, TextNode};

Element::node(Node::Text(TextNode {
    text: "Built by hand".into(),
    style: TextStyle::Caption,
    tone: Tone::Secondary,
    wrap: false,
    align: Align::Start,
}))
```

Raw nodes can't carry handlers. See [Protocol and ABI](Protocol-and-ABI.md#nodes) for the node types.
