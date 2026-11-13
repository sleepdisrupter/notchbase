# Protocol and ABI

You don't need this page to write a tool. It describes what the SDK does underneath: the `Driver` interface notchbase uses to run a tool, the JSON messages that carry it, and the small C ABI that carries those messages across a plugin boundary. Read it if you're debugging a plugin, writing tests at the wire level, or curious.

## Layers

```
 your Tool                   model, update, views, subscriptions
    │
 Runtime<T>                  SDK: turns events into messages, runs commands,
    │                        turns elements into nodes, tracks handler and reply ids
    │ implements Driver      manifest() · start(Start) · handle(Vec<Event>) · views()
    │
    ├── bundled tool         notchbase holds a Box<dyn Driver> from notchbase_sdk::boxed::<T>(waker)
    │
    └── plugin               ffi: C functions exchanging JSON
                             Call  ──►  {"call": "manifest" | "start" | "handle" | "views"}
                             Answer ◄── {"answer": "manifest" | "effects" | "views" | "failed"}
```

Bundled tools and plugins go through the same `Driver` calls with the same data. For a plugin, each call is serialized to JSON and passed through the C ABI; for a bundled tool it's a direct Rust call.

## The Driver

```rust
pub trait Driver {
    fn manifest(&self) -> Manifest;
    fn start(&mut self, start: Start) -> Effects;
    fn handle(&mut self, events: Vec<Event>) -> Effects;
    fn views(&mut self) -> Views;
}
```

| Call | When notchbase makes it | Returns |
| --- | --- | --- |
| `manifest` | Once, when loading | The tool's `Manifest` |
| `start` | Once, with the stored settings, folders and platform | `Effects` from `init` |
| `handle` | Whenever there are events for the tool, a timer is due, or the tool's waker fired | `Effects` from the resulting updates |
| `views` | When `Effects::revision` has changed since the last views | `Views`: tile, page and live nodes |

`Runtime<T>` implements `Driver` for any `T: Tool`. `notchbase_sdk::boxed::<T>(waker)` returns one as a `Box<dyn Driver>`, which is how the host creates bundled tools. Tests use `Runtime` directly; see [Testing](Testing.md).

`Runtime<T>` also has a few conveniences of its own: `send(msg)` runs a message through `update` as if it had come from an event, `tool()` returns the model, and `settings()` returns the current merged settings.

### Waking

Background work (`Command::perform`, `Command::spawn`) produces messages on other threads. The runtime queues them and calls its `Waker`, which tells notchbase to make a `handle` call soon, with no events if there are none. Queued messages are processed during that call, after its events.

## The C ABI

A plugin library exports one function:

```rust
pub type Entry = unsafe extern "C" fn() -> *const Plugin;
```

named `notchbase_tool_<crate name>` (`ffi::ENTRY_PREFIX` is `"notchbase_tool_"`). The `export!` macro writes it. It returns a pointer to a static table:

```rust
#[repr(C)]
pub struct Plugin {
    pub abi: u32,
    pub create: unsafe extern "C" fn(HostWaker) -> *mut c_void,
    pub call: unsafe extern "C" fn(*mut c_void, *const u8, usize) -> Buffer,
    pub release: unsafe extern "C" fn(Buffer),
    pub destroy: unsafe extern "C" fn(*mut c_void),
}

#[repr(C)]
pub struct HostWaker {
    pub context: *const c_void,
    pub wake: unsafe extern "C" fn(*const c_void),
}

#[repr(C)]
pub struct Buffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}
```

| Member | Contract |
| --- | --- |
| `abi` | `ffi::ABI_VERSION`, currently `1`. notchbase refuses any other value |
| `create(waker)` | Makes a tool instance and returns an opaque pointer to it. `waker.wake(waker.context)` may be called from any thread, any number of times, to ask for a `handle` call |
| `call(instance, bytes, len)` | Takes a UTF-8 JSON `Call` and returns a `Buffer` holding a JSON `Answer`. Never unwinds: panics and unreadable input come back as a `failed` answer |
| `release(buffer)` | Frees a buffer returned by `call`. The plugin allocated it, so only the plugin may free it |
| `destroy(instance)` | Drops the instance |

Calls to one instance come from one thread at a time. Only the waker is used from other threads.

`Plugin::of::<T>()` builds the table for a tool type; `ffi::entry_symbol(file_stem)` computes the entry name from a library's file name (dropping a leading `lib` and turning `-` into `_`); `ffi::serve(driver, bytes)` answers one JSON call on any `Driver`; and `ffi::panic_message` turns a panic payload into text.

### The WebAssembly ABI

A [portable tool](Plugins-and-Distribution.md#portable-tools) is a WebAssembly module (`wasm32-unknown-unknown`) that speaks the same JSON calls through four exports and one import, with a fifth export that is optional. `notchbase_sdk::export!` generates them.

| Export | Contract |
| --- | --- |
| `memory` | The module's linear memory |
| `nb_abi() -> i32` | `ffi::ABI_VERSION` |
| `nb_alloc(len: i32) -> i32` | A buffer of `len` bytes for notchbase to write a call into |
| `nb_call(ptr: i32, len: i32) -> i64` | Takes a JSON `Call` from that buffer and returns the JSON `Answer`, its address in the high 32 bits and its length in the low 32 |
| `nb_free(ptr: i32, len: i32)` | Frees a buffer from `nb_alloc` or `nb_call`. notchbase frees both after each call |
| `nb_panic() -> i64` (optional) | What the tool's last panic said, as text packed like an answer (address high, length low), or `0` when it has not panicked. notchbase asks only after a call failed, reads at most 2048 bytes and never frees it |

| Import (module `notchbase`) | Meaning |
| --- | --- |
| `now_ms() -> i64` | Unix time in milliseconds |

The SDK's side of these exports (`sdk/src/ffi.rs`) is `unsafe` and leans on two invariants the host keeps: `nb_free` (`ffi::portable::free`) may only be given a pointer and length that `nb_alloc` or `nb_call` handed out and that has not been freed yet, since it rebuilds the boxed slice from them; and `nb_call` (`ffi::portable::call`) may only be given a readable buffer holding the call, laid out as `nb_alloc` hands it out. A null pointer is treated as an empty buffer by both.

One module is one tool instance, created on its first call. There is no waker: a portable tool has no threads, so everything it does happens inside a call. A trap (a panic, `unreachable`, running out of the call's budget) fails that call and every later one. A panic in WebAssembly cannot be caught, so the SDK keeps a panic hook that writes down the message and its `file:line` before the trap, and notchbase fetches it through `nb_panic` to report "it crashed: <message> (<file>:<line>)" instead of the bare trap. A module without that export, or one that has nothing to say, is reported by its trap alone ("it crashed: wasm `unreachable` instruction executed"). Because the export is optional in both directions, it needed no new ABI or protocol version: tools built before it load as before, and tools built with it load in a notchbase that never asks.

### Lifecycle

1. notchbase loads the library, finds `notchbase_tool_<name>`, calls it, and checks `abi`.
2. `create` with a waker.
3. `call` with `{"call":"manifest"}`.
4. `call` with `{"call":"start", …}`.
5. `call` with `{"call":"views"}`, and again each time the revision in an answer changes.
6. `call` with `{"call":"handle", …}` whenever there is something to handle.
7. A `failed` answer stops the tool for the rest of the run.
8. Plugins are never unloaded during a run.

## Calls and answers

Every call is a JSON object tagged by `"call"`; every answer is tagged by `"answer"`.

```json
{"call":"manifest"}
```

```json
{"answer":"manifest","manifest":{"id":"picker","name":"Picker","description":"","version":"","page":null,"page_icon":null,"tab":true,
 "header_tab":false,"background":false,
 "shortcuts":[{"id":"pick","title":"Pick","default":"Ctrl+Shift+P"}],
 "settings":[{"key":"loud","title":"Loud","detail":"","kind":{"type":"toggle"},"default":false,"visible":true}],
 "protocol":5}}
```

```json
{"call":"start","start":{"settings":{"loud":true},
 "dirs":{"data":"C:/Users/me/AppData/Roaming/notchbase","home":"C:/Users/me","pictures":"C:/Users/me/Pictures",
         "videos":null,"desktop":null,"downloads":null,"documents":"C:/Users/me/Documents"},
 "platform":"windows","host_version":"0.1.0","clock_24h":true,"time_zone":"Europe/Paris"}}
```

```json
{"answer":"effects","effects":{"requests":[],
 "wants":{"every_ms":[1000],"settings":true,"recording":false,"audio_inputs":false,"media":false,
          "bluetooth":false,"volume":false,"notch":false,"calendar":false,"calendar_range":null,
          "power":false,"screen_lock":false,"sensors":false,"downloads":false,"focus":false},
 "revision":1}}
```

```json
{"call":"handle","events":[{"type":"shortcut","id":"pick"}]}
```

```json
{"call":"views"}
```

```json
{"answer":"views","views":{"tile":{"title":"Picker","icon":{"type":"icon","icon":"pipette","size":15.0,"tone":"blue"},
 "well":null,"tint":null,"wide":false,"enabled":true,"on_press":0,"shortcut":"pick",
 "accessory":{"type":"row","children":[{"type":"toggle","on":true,"on_toggle":1}],"spacing":null,"align":"center"},
 "footer":null},"page":null,"live":null}}
```

```json
{"answer":"failed","error":"the tool panicked"}
```

### Start

| Field | Meaning |
| --- | --- |
| `settings` | The values stored for this tool in `config.json`. The runtime lays them over the manifest's defaults |
| `dirs` | `data` plus the user's `home`, `pictures`, `videos`, `desktop`, `downloads`, `documents` (each may be `null`) |
| `platform` | `"windows"`, `"macos"` or `"other"` |
| `host_version` | notchbase's version |
| `clock_24h` | Whether the system shows times on a 24-hour clock |
| `time_zone` | The user's time zone as an IANA name such as `"Europe/Paris"`, or `null` when notchbase doesn't know it |

### Effects

| Field | Meaning |
| --- | --- |
| `requests` | [Requests](#requests) for notchbase, in order |
| `wants` | What the tool's current subscriptions need: `every_ms` (sorted, distinct intervals), a flag each for `settings`, `recording`, `audio_inputs`, `media`, `bluetooth`, `volume`, `notch`, `calendar`, `power`, `screen_lock`, `sensors`, `downloads`, `focus`, and `calendar_range` (`[from, to]` in Unix milliseconds, the span asked for with `calendar_between`, or `null`). Shortcuts aren't listed; they come from the manifest |
| `revision` | Grows every time the tool's model may have changed. notchbase asks for views again only when it differs from the last one it saw |

## Events

Sent in `handle` calls, tagged by `"type"`:

```json
{"type":"ui","surface":"tile","handler":0,"value":"press"}
{"type":"ui","surface":"tile","handler":1,"value":{"bool":true}}
{"type":"ui","surface":"page","handler":2,"value":{"index":1}}
{"type":"ui","surface":"page","handler":3,"value":{"number":0.5}}
{"type":"ui","surface":"page","handler":4,"value":{"text":"hi"}}
{"type":"ui","surface":"notice","handler":0,"value":"press"}
{"type":"shortcut","id":"pick"}
{"type":"tick","every_ms":1000}
{"type":"settings","values":{"loud":true}}
{"type":"reply","id":1,"reply":{"type":"color","color":{"r":58,"g":123,"b":255,"a":255}}}
{"type":"recording","update":{"type":"state","state":{"state":"recording","since":1790000000000}}}
{"type":"recording","update":{"type":"saved","path":"clip.mp4","seconds":42.4,"note":null}}
{"type":"audio_inputs","inputs":[{"id":"a","name":"USB Mic","is_default":true}]}
{"type":"media","now_playing":{"title":"Song","artist":"Band","playing":true,"position":12.0,"duration":200.0}}
{"type":"bluetooth","change":{"type":"connected","device":{"id":"x","name":"AirPods","kind":"headphones","battery":80}}}
{"type":"volume","level":{"volume":0.4,"muted":false}}
{"type":"notch","state":{"open":true,"page":"tools","phase":"open","style":"island","expansion":1.0,"hug":0.0,"float":1.0,"width":640.0,"height":320.0,"notch":null,"held":false,"settled":true,"activities":[],"music":false}}
{"type":"calendar","feed":{"type":"events","events":[{"id":"7F2C","title":"Standup","start":1790000000000,"end":1790001800000,"all_day":false,"location":"","notes":"","url":"","calendar":"Work","color":null,"declined":false,"calendar_id":"work","editable":true}],"calendars":[{"id":"work","title":"Work","account":"iCloud","color":null,"writable":true}]}}
{"type":"power","power":{"level":64,"plugged":true,"charging":true}}
{"type":"screen_lock","locked":true}
{"type":"focus","focus":{"active":true,"name":"Work"}}
{"type":"sensors","sensors":{"camera":false,"microphone":true,"apps":["Zoom"],"muted":false}}
{"type":"download","change":{"type":"progress","download":{"id":"clip.mp4.crdownload","name":"clip.mp4","received":5200000,"total":null}}}
{"type":"download","change":{"type":"finished","download":{"id":"clip.mp4.crdownload","name":"clip.mp4","received":9100000,"total":9100000},"path":"C:/Users/me/Downloads/clip.mp4"}}
```

### UI events and handler ids

When the runtime turns a surface's elements into nodes, every handler gets a number, counting from 0 as it walks that surface's tree (a tile's own `on_press` included). Tile, page and live each have their own numbering, and each `views` call renumbers them. A `ui` event names the surface (`tile`, `page`, `live` or `notice`) and the number, plus a value:

| `value` | Sent by |
| --- | --- |
| `"press"` | Buttons, the tile itself, `on_submit` of a text input |
| `{"bool": b}` | Toggles, checkboxes, chips (the new state) |
| `{"index": n}` | Dropdowns, segmented controls, palettes; a day of a month or week, as `year * 10000 + month * 100 + day` |
| `{"number": x}` | Sliders; pans (points moved, or where a click landed); `on_scroll` of a month or week (`1` or `-1`) |
| `{"text": s}` | Text inputs (the whole text); a week's events (the event's `id`) |
| `{"picked": {"column": c, "index": n}}` | Pickers (the wheel and the option it landed on) |
| `{"pointer": {"kind": k, "x": x, "y": y, "dx": dx, "dy": dy}}` | Canvases. `kind` is `hover`, `leave`, `press`, `drag`, `release` or `click` for `on_pointer`, and `scroll` for `on_scroll` |

A number that doesn't exist, or a value of the wrong kind for that handler, is ignored.

The `notice` surface is the [controls](Surfaces.md#controls) of a notice sent with `Command::notice_with_controls`. It is not part of `views`: the runtime numbers the controls' handlers, from 0, when it writes the `notice` request, and keeps that table until the tool's next `notice` request. A `views` call leaves it alone. A `notice` request without controls drops it, so a press on the controls of the notice before is ignored like any unknown number.

### Replies

Commands that ask a question send a request with a `reply` id. notchbase answers with a `reply` event carrying the same id:

```json
{"type":"pick_color","reply":1}
{"type":"reply","id":1,"reply":{"type":"color","color":{"r":58,"g":123,"b":255,"a":255}}}

{"type":"capture_screen","reply":2}
{"type":"reply","id":2,"reply":{"type":"capture","capture":{"image":7,"width":2560,"height":1600,"on_clipboard":false}}}

{"type":"save_image","image":7,"folder":null,"prefix":"Screenshot","format":"png","reply":3}
{"type":"reply","id":3,"reply":{"type":"saved","result":{"Ok":"C:/Users/me/Pictures/Screenshot.png"}}}
{"type":"reply","id":3,"reply":{"type":"saved","result":{"Err":"disk full"}}}

{"type":"fetch","request":{"method":"GET","url":"https://api.example.com/items","headers":[["Accept","application/json"]],
 "body":null,"timeout_ms":null},"reply":4}
{"type":"reply","id":4,"reply":{"type":"fetched","result":{"Ok":{"status":200,"body":"[]"}}}}
{"type":"reply","id":4,"reply":{"type":"fetched","result":{"Err":"no connection"}}}

{"type":"read_file","path":"list.json","reply":5}
{"type":"reply","id":5,"reply":{"type":"read","result":{"Ok":"[]"}}}

{"type":"write_file","path":"~/Documents/Notes/today.md","text":"# Today","reply":6}
{"type":"reply","id":6,"reply":{"type":"saved","result":{"Ok":"C:/Users/me/Documents/Notes/today.md"}}}
```

`color` and `capture` are `null` when the user cancelled. `saved`, `fetched` and `read` carry a `result`, `{"Ok": …}` or `{"Err": message}`; a `write_file` is answered with `saved` and the path written, like a `save_image`. Each id is answered once; the runtime forgets it after the first answer.

## Requests

Returned in `effects.requests`, tagged by `"type"`:

```json
{"type":"notify","text":"Hello"}
{"type":"notice","notice":{"leading":{"type":"swatch","color":{"r":10,"g":132,"b":255,"a":255},"size":20.0},
 "trailing":{"type":"text","text":"#0A84FF","style":"value","tone":"primary","wrap":false,"align":"start"},
 "below":null,"banner":false,"duration_ms":2400,"pop":true,"shake":false,"size":"medium"}}
{"type":"notice","notice":{"leading":{"type":"icon","icon":"checkmark","size":15.0,"tone":"green"},
 "trailing":{"type":"text","text":"Copied","style":"callout","tone":"primary","wrap":false,"align":"start"},
 "below":null,"banner":false,"duration_ms":5000,"pop":false,"shake":false,"size":"medium",
 "controls":{"type":"button","label":"Annotate","icon":null,"style":"glass","on_press":0}}}
{"type":"dismiss_notice"}
{"type":"copy_text","text":"#3A7BFF"}
{"type":"copy_image","image":7}
{"type":"annotate","image":7}
{"type":"save_image","image":7,"folder":null,"prefix":"Screenshot","format":"png","reply":3}
{"type":"pick_color","reply":1}
{"type":"capture_screen","reply":2}
{"type":"start_recording","options":{"microphone":true,"desktop_audio":false,"microphone_id":null,"folder":null,
 "max_fps":null,"dim_outside":true}}
{"type":"stop_recording"}
{"type":"cancel_capture"}
{"type":"save_setting","key":"loud","value":true}
{"type":"shelve","paths":["clip.mp4"]}
{"type":"reveal","path":"clip.mp4"}
{"type":"open","target":"https://example.com"}
{"type":"open_page"}
{"type":"open_settings"}
{"type":"collapse"}
{"type":"hold","held":true}
{"type":"media","control":"play_pause"}
{"type":"log","text":"hi"}
{"type":"chime"}
{"type":"save_event","event":{"id":null,"calendar_id":"work","title":"Dentist","start":1790000000000,"end":1790001800000,
 "all_day":false,"location":null,"notes":null,"url":null}}
{"type":"delete_event","id":"7F2C"}
{"type":"keep_awake","awake":true}
{"type":"mute_microphone","muted":true}
{"type":"fetch","request":{"method":"GET","url":"https://api.example.com/items","headers":[["Accept","application/json"]],
 "body":null,"timeout_ms":null},"reply":4}
{"type":"read_file","path":"list.json","reply":5}
{"type":"write_file","path":"~/Documents/Notes/today.md","text":"# Today","reply":6}
```

A notice with `duration_ms` of `0` is sticky. A notice's `controls` is written only when it has some: a notice without them has no `controls` field at all, exactly as before version 5, and a missing field reads as none. `annotate` names a captured image by its id, like `copy_image`, and has no reply. `media` controls are `play_pause`, `next` and `previous`; image formats are `png` and `jpeg`. `hold` keeps the notch open while `held` is true and lets it go when false (`Command::hold_open` and `Command::release`). A `save_event` with an `id` of `null` adds an event; with an event's id it changes that one. In a `fetch`, `headers` are `[name, value]` pairs, and `body` and `timeout_ms` (milliseconds) may be `null`. `fetch`, `read_file` and `write_file` are answered with a [reply](#replies).

## Nodes

Views and notices are trees of nodes, each tagged by `"type"`. Fields in brackets may be left out when writing JSON by hand; they take the default shown.

| `type` | Fields |
| --- | --- |
| `row` | `children`, [`spacing`: null], [`align`: `"center"`] |
| `column` | `children`, [`spacing`: null], [`align`: `"start"`] |
| `frame` | `child`, [`width`], [`height`], [`align`: `"center"`] |
| `card` | `child`, [`padding`] |
| `list` | `children` |
| `adaptive` | `notch`, `island` |
| `keyed` | `key`, `child` |
| `pan` | `child`, [`offset`: 0], [`on_pan`], [`on_tap`] |
| `spacer` | [`size`: null = flexible] |
| `divider` | |
| `text` | `text`, [`style`: `"body"`], [`tone`: `"primary"`], [`wrap`: false], [`align`: `"start"`] |
| `icon` | `icon`, `size`, [`tone`: `"primary"`] |
| `button` | `label`, [`icon`], [`style`: `"glass"`], [`on_press`] |
| `toggle` | `on`, [`on_toggle`] |
| `checkbox` | `label`, `checked`, [`on_toggle`] |
| `chip` | `label`, [`icon`], [`icon_off`], `on`, [`on_toggle`] |
| `dropdown` | `options` (`[{"label": …, "icon": …}]`), [`selected`], [`placeholder`: ""], [`on_select`], [`compact`: false] |
| `segmented` | `options` (strings), `selected`, [`on_select`] |
| `slider` | `value`, `min`, `max`, [`step`], [`icon`], [`on_change`] |
| `text_input` | `value`, [`placeholder`: ""], [`on_change`], [`on_submit`] |
| `shortcut` | `id` |
| `keycaps` | `keys` |
| `progress` | `value`, [`tone`: `"primary"`] |
| `ring` | `value`, `size`, [`tone`: `"primary"`], [`span`: `{"start": ms, "end": ms}`], [`stroke`], [`center`: a node] |
| `picker` | `columns` (`[{"options": […], "selected": n, "unit": "", "looping": false}]`), [`on_change`] |
| `round_button` | `label`, [`tone`: `"primary"`], `size`, [`on_press`] |
| `swatch` | `color`, `size` |
| `dot` | `color`, `size` |
| `badge` | `icon`, `size`, [`tone`: `"primary"`] |
| `image` | `source` (`{"capture": id}` or `{"file": path}`), `width`, `height`, [`radius`: 0] |
| `timer` | `at`, [`mode`: `"elapsed"`], [`style`: `"body"`], [`tone`: `"primary"`] |
| `month` | `year`, `month`, [`today`], [`selected`], [`marks`: `[{"day": d, "colors": […]}]`], [`monday_first`: false], [`on_select`], [`on_scroll`] |
| `week` | `days` (`[{"year": y, "month": m, "day": d, "weekday": …, "today": false, "selected": false, "events": [{"id": …, "time": "", "title": …, "color": null}]}]`), [`on_day`], [`on_event`], [`on_scroll`] |
| `canvas` | `width`, `height`, [`stretch`: false], [`shapes`: none; see below], [`on_pointer`], [`on_scroll`] |
| `palette` | `colors`, [`selected`], [`on_select`] |

A canvas's `shapes` are tagged by `"type"` too:

| `type` | Fields |
| --- | --- |
| `rect` | `rect` (`[x, y, width, height]`), [`radius`: 0], [`fill`], [`stroke`] |
| `ellipse` | `center` (`[x, y]`), `radius` (`[across, down]`), [`fill`], [`stroke`] |
| `line` | `points` (`[[x, y], …]`), `stroke` |
| `polygon` | `points`, [`fill`], [`stroke`] |
| `text` | `at` (`[x, y]`), `text`, `size`, [`weight`: `"regular"`], `color`, [`anchor`: `"top_left"`], [`width`: null = not wrapped] |
| `icon` | `icon`, `center`, `size`, `color` |
| `run` | `at`, `parts` (shapes), [`gap`: 0], [`anchor`: `"top_left"`] |
| `image` | `source`, `rect`, [`radius`: 0] |
| `glass` | `rect`, [`radius`: 0] |
| `shadow` | `rect`, [`radius`: 0], `blur`, `color` |
| `group` | `shapes`, [`opacity`: 1], [`offset`: `[0, 0]`], [`scale`: 1], [`clip`: a rect] |

A `fill` is `{"solid": color}`, `{"linear": {"from": color, "to": color, "angle": a}}` or `{"radial": {"inner": color, "outer": color}}`, and a `stroke` is `{"width": w, "color": color}`. Weights are `regular`, `semibold` and `mono`; anchors are `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom` and `bottom_right`.

The defaults above are those of the wire format. The builders choose their own when you don't call a method (for example `timer` uses the `value` style and `progress` the `white` tone), and always write every field, except a ring's `span`, `stroke` and `center`, which are left out when unset. Timer modes are `elapsed`, `remaining` and `clock`.

Handler fields (`on_press`, `on_toggle`, …) hold handler numbers. Colors are `{"r","g","b","a"}` objects with values 0–255. Enum values are written in `snake_case`: tones (`"secondary"`, `"orange"`), text styles (`"subheadline"`), icons (`"microphone_muted"`), button styles, notice sizes and timer modes.

The three surface roots:

| Root | Fields |
| --- | --- |
| Tile | `title`, `icon` (a node), [`well`], [`tint`], [`wide`: false], [`enabled`: true], [`on_press`], [`shortcut`], [`accessory`], [`footer`] |
| Live | `leading`, `trailing`, [`controls`: none; a node whose handlers answer on the `live` surface] |
| Notice | `leading`, `trailing`, [`below`], [`banner`: false], `duration_ms`, [`pop`: false], [`shake`: false], [`size`: `"medium"`], [`controls`: none; a node whose handlers answer on the `notice` surface] |

## Manifest and settings

```json
{"id":"picker","name":"Picker","description":"","version":"","page":null,"page_icon":null,"tab":true,
 "header_tab":false,"background":false,
 "shortcuts":[{"id":"pick","title":"Pick","default":"Ctrl+Shift+P"}],
 "settings":[{"key":"loud","title":"Loud","detail":"","kind":{"type":"toggle"},"default":false,"visible":true}],
 "protocol":5}
```

Setting kinds: `{"type":"toggle"}`, `{"type":"choice","options":[{"value":…,"label":…}]}`, `{"type":"number","min":…,"max":…,"step":…,"unit":…}`, `{"type":"text","placeholder":…,"secret":…}`, `{"type":"folder","placeholder":…}` and `{"type":"data"}`. A text setting's `secret` may be left out, and then reads as `false`. A setting that only shows under another one carries `"shown_when":"save"` (while that toggle is on) or `"shown_when":"source=server"` (while that choice has that value); the field is left out otherwise.

## Versions

| Constant | Value | Meaning |
| --- | --- | --- |
| `ffi::ABI_VERSION` | 1 | The C table layout and calling convention. Checked when loading a plugin |
| `protocol::PROTOCOL_VERSION` | 5 | The JSON message format. Version 3 added the `power`, `screen_lock`, `sensors` and `download` events and the `keep_awake` and `mute_microphone` requests. Version 4 added the `pan` and `canvas` nodes, the `pointer` UI value, the `fetch`, `read_file` and `write_file` requests with their `fetched` and `read` replies, `time_zone` in `start` and `background` in the manifest. Version 5 added the `annotate` request, `controls` on a notice and the `notice` surface for `ui` events. `Manifest::new` stamps it into every manifest as `protocol`, and notchbase refuses a plugin built for a newer protocol than it speaks |

### What version 5 means for older hosts

Nothing in version 5 changes a message that existed before: a notice without controls is written field for field as it was, and a host that speaks 5 reads every older tool unchanged. The number moved because an older host knows none of the three additions: it has no `annotate` request, draws no `controls`, and has no `notice` surface to name in an event.

- **A tool built with SDK 0.4.0 carries `"protocol": 5`**, whether or not it uses any of this, because `Manifest::new` stamps the SDK's version. A notchbase that speaks 4 or less refuses to load it: "it needs a newer notchbase (tool protocol 5)". This holds for native plugins and portable (WebAssembly) tools alike.
- **A tool built with an older SDK** keeps loading in every newer notchbase.

So build against the SDK of the oldest notchbase you want your tool to run on.

## Tools without the Rust SDK

Nothing in the boundary is specific to Rust, so a plugin could be written in any language that can export a C function: return a `Plugin` table from `notchbase_tool_<name>`, speak the JSON above, number handlers and replies as described, and free your own buffers in `release`. The Rust SDK is the supported way to write tools and takes care of all of this.
