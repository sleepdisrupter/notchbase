# Surfaces

A *surface* is a place in the notch where a tool can show something. There are three you describe from your model (tile, page and live activity) and two you trigger with a command (notice and message).

## Closed and open

notchbase has two states.

**Closed**, it hugs the camera housing on a notched MacBook display, or floats as a small island at the top of other displays. While closed it can grow to show:

- a **live activity**: something ongoing, for as long as it lasts (a recording timer, a countdown)
- a **notice**: a short pop-up with an element on each side (a copied color, a saved screenshot)
- a **message**: a short line of plain text (an error, a confirmation)

The closed notch is mostly not interactive. Controls you put in a live activity's leading or trailing element, or in a notice's leading, trailing or below element, are drawn but don't respond; hovering the notch opens it. There are two exceptions, both below: a live activity's `controls`, which appear when someone hovers the activity, and the controls a notice is sent with through `Command::notice_with_controls`. Both respond.

**Open**, it shows a row of tab icons along its top and one page under them: **Media**, **Tools**, **Shelf**, **Sound**, **Stats**, then any tool pages that asked for a tab of their own (such as **Calendar**), and a gear for **Settings**. Tiles live on the **Tools** tab, and a tool's page opens from there unless its manifest asks for a header tab with `.header_tab()`. Everything in the open notch is interactive.

| Surface | Closed or open | Interactive | Provided by |
| --- | --- | --- | --- |
| [Tile](#tile) | Open, Tools tab | Yes | `Tool::tile` |
| [Page](#page) | Open, from the Tools tab | Yes | `Tool::page` + `Manifest::page` |
| [Live activity](#live-activity) | Closed | Only its [controls](#live-activity) | `Tool::live` |
| [Notice](#notice) | Closed | Only its [controls](#controls) | `Command::notice`, `Command::notice_with_controls` |
| [Message](#message) | Closed | No | `Command::notify` |

## Tile

A card on the **Tools** tab. Most tools are one tile: click it to act, glance at it for status.

```
┌──────────────────────────────────────────────┐
│  (◉)  Title                        accessory │
│                                              │
│  Ctrl  Alt  R                         footer │
└──────────────────────────────────────────────┘
   │     │                                │
 icon   shortcut field (Tile::shortcut)  footer
 well
```

```rust
fn tile(&self) -> Option<Tile<Msg>> {
    Some(
        Tile::new("Record Screen")
            .icon(Icon::Record)
            .wide()
            .shortcut("record")
            .on_press(Msg::Toggle)
            .accessory(chip("Mic", self.mic).icons(Icon::Microphone, Icon::MicrophoneMuted).on_toggle(Msg::Mic))
            .footer(dropdown(self.inputs.iter().cloned(), None).placeholder("Default Microphone").on_select(Msg::Choose)),
    )
}
```

| Method | Effect |
| --- | --- |
| `Tile::new(title)` | A tile with a star in its icon well |
| `.title(text)` | Replace the title. It is also the caption under your icon, so keep it short; titles are shortened with "…" to fit |
| `.icon(Icon)` | A glyph in the icon well |
| `.symbol(element)` | Any element in the icon well instead, such as `dot(Tone::Red, 12.0)` or `icon(Icon::Stop).tone(Tone::White)` |
| `.well(Tone)` | Fill the icon well with a solid tone (it's glass by default) |
| `.tint(Tone)` | Tint the whole tile, for a state that should stand out (recording, running). A tinted tool also gets a ring round its icon and a row of its own at the top of the Tools tab |
| `.wide()` | Kept for older tools; it no longer changes the layout |
| `.on_press(msg)` | Clicking anywhere on the tile (outside its controls) sends `msg` |
| `.enabled(false)` | The tile ignores clicks, for example while saving |
| `.shortcut(id)` | Show one of your shortcuts as keycaps along the bottom; clicking them rebinds it |
| `.accessory(element)` | An element at the top right, beside the title |
| `.footer(element)` | An element at the bottom right, beside the shortcut field |

Tiles have a fixed height, so keep accessories and footers to one line: a value, a chip or two, a dropdown, a short caption.

### Layout on the Tools tab

The Tools tab is a launcher: your tool appears as its icon with its title underneath, four to a row (five when there are exactly five), in the order of `layout.tools` in [`config.json`](../Configuration.md#layout). notchbase adds each new tool to the end of that list, and people can reorder it. Clicking the icon does what `on_press` says (or opens your page if you have one and no `on_press`). The whole tile, with its accessory, footer and shortcut, is the card under the icons, which opens when someone lingers on your icon. So:

- choose an icon (or `symbol`) that says what the tool is on its own;
- put state people should see at a glance in `well` and `tint`, since those show on the icon itself, and put the details in the accessory and footer;
- while your tool is busy, a `tint` also gives it a slim row at the top with its title and footer, so a short footer (a timer, a count) reads well there.

### A tile that changes with state

A tile is rebuilt from the model every time it changes, so different states can look completely different. The built-in recorder:

```rust
fn tile(&self) -> Option<Tile<Msg>> {
    let tile = Tile::new("Record Screen").wide().shortcut("record").on_press(Msg::Toggle);
    Some(match self.since {
        Some(since) => tile
            .title("Recording")
            .symbol(icon(Icon::Stop).size(9.0).tone(Tone::White))
            .well(Tone::Red)
            .tint(Tone::Red)
            .accessory(timer(since).style(TextStyle::Display))
            .footer(text("Click to stop").subheadline().tertiary()),
        None if self.saving => tile.title("Saving…").enabled(false),
        None => tile.symbol(dot(Tone::Red, 12.0)),
    })
}
```

## Page

A tool with more to show gets a page: a full view inside the open notch that opens from the **Tools** tab. Declare it in the manifest and return the content from `page`:

```rust
fn manifest() -> Manifest {
    Manifest::new("snippets", "Snippets").page("Snippets").page_icon(Icon::Folder)
}

fn page(&self) -> Option<Element<Msg>> {
    let composer = row![
        text_input(&self.draft).placeholder("New snippet").on_change(Msg::Draft).on_submit(Msg::Add),
        button("Add").filled().on_press(Msg::Add),
    ];
    let saved = list(self.saved.iter().enumerate().map(|(index, snippet)| snippet_row(index, snippet)));
    Some(column![composer, saved].spacing(10.0).into())
}
```

- How people get to it: if your tool has a tile without its own `on_press`, clicking the tile opens the page. If it has no tile at all, notchbase makes one for it on the Tools tab, titled with the argument to `Manifest::page` and showing `Manifest::page_icon` (a star without one) and a chevron. If your tile's click already does something else, open the page from a button with `Command::open_page()`.
- A header tab: add `.header_tab()` to your manifest and your page also gets a tab in the row along the top of the open notch (at the end, unless the user places your tool id in `layout.tabs` in `config.json`), titled with the page title and shown with `page_icon`, like the bundled Calendar. Keep it for pages people come back to all day. With a tab, the automatic launcher tile is left out (a tile you draw yourself still shows). The header makes room for extra tabs by shortening names and then icons, so no tab ever hides.
- No page and no tile: a tool that only shows notices or a live activity can say so with `.background()` in its manifest. notchbase then lists it under **Settings › Indicators**, beside its own Battery and Camera & Microphone, instead of among the tools.
- While the page is open the **Tools** tab stays lit, and clicking Tools goes back to the tiles.
- Global shortcuts keep working whether or not the page is open.
- The page is as tall as its content, and the open notch resizes to fit it. A page taller than the notch can show scrolls inside it with a mouse wheel or two fingers on a trackpad.
- `Manifest::no_tab` is kept so older tools still compile, but it does nothing now: every page opens from the Tools tab.
- `Command::open_page()` opens the notch on your page from anywhere, for example from a shortcut or a button. See [Commands](Commands.md#open_page).
- [`Subscription::notch`](Subscriptions.md#notch) tells you whether the notch is open and which page is showing.

[Example: Snippets](Example-Snippets.md) is a complete tool built around a page.

## Live activity

Something ongoing, shown in the closed notch for as long as `live` returns `Some`. On a notched display the two elements sit on either side of the camera housing; on an island they sit at the two ends of a small capsule.

```rust
fn live(&self) -> Option<Live<Msg>> {
    let since = self.since?;
    Some(Live::new(dot(Tone::Red, 9.0), timer(since)))
}
```

`Live::new(leading, trailing)` takes two elements. Keep both small: a dot, an icon, a ring, and a short value or `timer`. The notch widens just enough to fit them.

Add `.controls(element)` to let people act on it without opening anything. The controls appear when the activity is shown as an ear beside another activity and someone hovers it: the ear swells into a small pill with your trailing element and the controls, and clicks reach your tool on the `Live` surface like any other handler.

```rust
fn live(&self) -> Option<Live<Msg>> {
    let since = self.since?;
    let stop = button("").icon(Icon::Stop).on_press(Msg::Stop);
    Some(Live::new(dot(Tone::Red, 9.0), timer(since)).controls(stop))
}
```

Keep controls to one or two icon-only buttons (`button("").icon(…)` draws a small round glass button). The built-in Timer offers pause or resume and cancel, the recorder offers stop.

- A `timer` in a live activity keeps counting on its own; you don't need to tick.
- notchbase shows up to three live activities: one in the middle and two ears beside it (a playing song takes the middle).
- A notice or message temporarily covers the live activity; it comes back when they expire.
- Hovering the notch opens it as usual, and the live activity fades out while the notch is open.
- Return `None` to end it. The notch shrinks back.

## Notice

A pop-up in the closed notch, for a moment. Build a `Notice` and send it with `Command::notice`.

```rust
let copied = Notice::new(swatch(color).size(18.0), text(hex).value()).pop().duration(Duration::from_millis(2200));

let exported = Notice::new(icon(Icon::Folder).tone(Tone::Blue), text("Exported").callout())
    .below(text("3 files in Downloads").caption().secondary())
    .size(NoticeSize::Large)
    .duration(Duration::from_secs(4));

Command::notice(copied)
```

| Method | Effect |
| --- | --- |
| `Notice::new(leading, trailing)` | Elements for the two sides, shown for 2.4 seconds |
| `.below(element)` | An extra element centered under the two sides; the notice grows taller to fit it. It gets about 304 points of width: wrapped text wraps within that, other text is shortened with "…". In a banner it goes under the trailing element instead |
| `.banner()` | Lay the notice out like a notification: see [Banners](#banners) |
| `.duration(Duration)` | How long it stays |
| `.sticky()` | Stay until the user clicks the notch or you send `Command::dismiss_notice()` |
| `.pop()` | The leading element springs in |
| `.shake()` | The notch jolts sideways with a springy shake to get attention; also pops |
| `.size(NoticeSize)` | `Small`, `Medium` (default) or `Large`: the minimum room the notice takes. The notch still grows if the content needs more |
| `.into_node()` | The wire form, `protocol::NoticeNode` |

The elements of a `Notice` (leading, trailing and below) are `Element<()>`: they can't carry handlers. Element builders are generic, so this happens without any extra code. For something to press, send the notice with [controls](#controls).

A new notice replaces the one showing. `Command::dismiss_notice()` removes your tool's notice at once, which is how a tool ends a sticky one; the user can also dismiss a sticky notice by clicking the notch.

### Controls

`Command::notice_with_controls(notice, controls)` shows a notice with one more element that does take presses: a button for the one thing people are likely to want next. The built-in Screenshot tool offers **Annotate** this way.

```rust
let shown = Notice::new(thumbnail, text("Copied").callout()).pop().duration(Duration::from_secs(5));
let annotate = button("Annotate").on_press(Msg::Annotate(capture.image));
Command::notice_with_controls(shown, annotate)
```

```rust
Msg::Annotate(image) => Command::dismiss_notice().and(Command::annotate(image)),
```

- `controls` is an `Element<Msg>`, with handlers like any element on a tile or page. Keep it to one short button, or two.
- **Where it goes.** On an island, a notice without `below` that isn't a banner gets the controls tucked into the trailing end of its capsule, which widens to fit (as long as they are no taller than 27 points, which a button is). Beside a camera housing, a single button (no `below`, not a banner) ends the right wing, level with the camera, and the leading and trailing elements share the left wing; when even wings would make the notice wider than the panel it opens into, it leans toward the button instead. Everywhere else (several controls beside a camera, under a `below` element, under a banner) they sit centred in a row under the content, and the notice grows downward to hold them. An `adaptive` button can give the notch a shorter label so it fits, as the app's "Updated" notice does with **Changes**.
- **Presses reach `update`** as your message, like any other handler. On the wire they are `ui` events on the `notice` surface, with handler numbers of their own.
- **They belong to that notice.** The handlers stay valid, however often your views are rebuilt, until your tool sends its next notice. A newer `notice_with_controls` takes over; a plain `Command::notice` leaves the old controls dead, and a late press on them is ignored.
- **A press doesn't dismiss the notice.** Send `Command::dismiss_notice()` yourself when the notice has done its job. It ends the notice straight away, even with the pointer still on the button, and the notch then waits for the pointer to leave and come back before it peeks or opens under it.
- **Give people time to reach it.** While the pointer is on the controls, or within 6 points of them, the notice stays up, and for a second after it leaves, but it has to still be there when the pointer arrives. A pointer resting anywhere else on the notice does not keep it. The Screenshot tool shows its notice for 5 seconds.
- Resting on the controls (or within those 6 points) never opens the notch; clicking or resting anywhere else on the notice still does. On a locked screen the controls don't respond.
- Controls need tool protocol 5. A tool built with this SDK is refused by a notchbase from before it (see [Plugins and distribution](Plugins-and-Distribution.md#compatibility)).

### Banners

By default a notice is laid out around the camera: leading on the left, trailing on the right, `below` centered underneath. For a title and a message, `.banner()` lays it out like a notification instead: the leading element on the left, the trailing element beside it, and the `below` element under the trailing one, both left-aligned.

```rust
let alert = Notice::new(badge(Icon::Bell, Tone::Orange, 30.0), text("Build failed").headline())
    .below(text("3 tests failed in core").callout().secondary().wrap())
    .banner()
    .shake()
    .sticky();
```

- On an island the capsule grows to fit the banner, and the leading element sits concentric with its rounded end.
- On a notched Mac the banner hangs below the camera.
- Text in a banner gets up to 260 points of width. Mark the message `.wrap()` so a long one wraps instead of being shortened.
- `.size()` doesn't apply: a banner is as big as its content.
- `.pop()` and `.shake()` work as usual; the leading element is the one that pops.

The Alerts tool sends every alert as a banner: a 30 point badge, a `headline` title and a `callout` secondary message, or a 22 point badge when there's no message.

### Fitting both kinds of display

A notched display has little room beside the camera, an island has more. `adaptive` lets a notice say less on a notch:

```rust
fn saved(seconds: u64) -> Notice {
    let length = clock(Duration::from_secs(seconds));
    let island = row![text("Saved").callout(), text(&length).value().secondary()].spacing(6.0);
    let notch = text(length).value().secondary();
    let check = adaptive(badge(Icon::Checkmark, Tone::Green, 16.0), badge(Icon::Checkmark, Tone::Green, 18.0));
    Notice::new(check, adaptive(notch, island)).pop().size(NoticeSize::Small)
}
```

## Message

```rust
Command::notify("Couldn't reach the server")
```

A line of plain text in the closed notch, wrapped and centered if it's long. It stays for `notices.message_ms` from [`config.json`](../Configuration.md#notices) (3.5 seconds by default). Use it for errors and one-off confirmations that don't need an icon.

## Which one to use

| You want to… | Use |
| --- | --- |
| Offer an action and a glance of status | Tile |
| Show a list, a form, or more controls than fit a tile | Page |
| Show something that's happening right now and will end | Live activity |
| Confirm that something happened, with a visual | Notice |
| Confirm it and offer the obvious next step | Notice with [controls](#controls) |
| Report an error or say something short in words | Message |
| Keep a prompt up until the user deals with it | Sticky notice + `Command::dismiss_notice()` |
