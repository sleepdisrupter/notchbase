# Alerts

Alerts turns a small webhook on your own computer into notch notifications. Anything that can make an HTTP request (a build script, a test runner, a cron job, a home server reached through an SSH tunnel, a shell one-liner after a long command) can make the notch shake and show a title, a message and a colored badge.

It is a bundled tool: the crate is `tools/alerts`, its id is `alerts`, and it is written against the tool SDK like the other built-in tools (see [Tools overview](tools/Overview.md)).

## Quick start

With notchbase running, send:

```sh
curl -d title="Hello from the terminal" http://127.0.0.1:47832/alert
```

The notch jolts sideways, scales up and shows the alert for 4 seconds. That's it: no account, no setup.

## Endpoints

The server listens on `127.0.0.1` only, on port **47832** by default.

| Method | Path | What it does |
| --- | --- | --- |
| `POST` | `/alert` | Show an alert. Fields come from a JSON object, a form body, or the query string. |
| `GET` | `/alert?title=…` | Show an alert from the query string. Handy for quick `curl` calls and the browser address bar. |
| `DELETE` | `/alert` | Dismiss the alert that is showing right now. |
| `POST` | `/alert/dismiss` | The same, for clients that can't send `DELETE`. |

A trailing slash (`/alert/`) is accepted. Anything else answers `404`, and a known path with the wrong method answers `405` with an `Allow` header.

Every request gets one response and the connection is closed (`Connection: close`).

## Fields

Only `title` is required.

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `title` | text | (required) | Shown next to the badge. Cut to 60 characters. |
| `message` | text | none | Shown under the title, wrapped onto more lines when it's long. Cut to 140 characters. `text` is accepted as another name for it. |
| `icon` | name | `bell` | The badge glyph. See [Icons](#icons). |
| `tone` | name | `blue` | The badge color. See [Tones](#tones). |
| `duration_ms` | number | `4000` | How long the alert stays, in milliseconds, from `1` up to `86400000` (24 hours; longer values are capped). `0` means it stays until clicked. |
| `sticky` | boolean | `false` | `true` keeps the alert on screen until you click the notch or it is dismissed, whatever `duration_ms` says. |
| `shake` | boolean | `true` | `false` turns off the sideways jolt; the alert still pops in. |

Details:

- Text is tidied before it is shown: line breaks, tabs and other control characters become spaces, runs of spaces collapse, and the ends are trimmed. A message that is empty after tidying is dropped.
- Names (`icon`, `tone`) are case-insensitive. Icons also accept `-` or a space instead of `_` (`chevron-down`, `Chevron Down`).
- Booleans accept `true`/`false`, `1`/`0`, `yes`/`no` and `on`/`off`. In a query string a bare key counts as true: `?sticky`.
- Numbers may also be sent as strings (`"duration_ms": "6000"`), which is what form bodies and query strings do anyway.
- Unknown fields are ignored.

### Where fields come from

For `POST /alert`, the body decides:

- a body starting with `{` is read as a **JSON object** (a UTF-8 byte order mark is skipped);
- any other non-empty body is read as a **form** (`application/x-www-form-urlencoded`), which is what `curl -d key=value` and PowerShell's `-Body @{...}` send;
- an empty body means the fields come from the query string alone.

The query string is always read first and the body's fields override it, so `POST /alert?title=Build` with a body of `message=done` works. The `Content-Type` header is not trusted either way, so `curl -d '{"title":"Hi"}'` (which labels JSON as a form) still works.

Query strings and form bodies are percent-decoded, with `+` read as a space.

## Examples

### macOS and Linux (curl)

```sh
curl -X POST http://127.0.0.1:47832/alert \
  -H 'Content-Type: application/json' \
  -d '{"title":"Build finished","message":"All 212 tests passed","icon":"checkmark","tone":"green"}'
```

Form fields, no JSON quoting needed:

```sh
curl -d title="Deploy done" -d message="api-3 is live" -d tone=teal http://127.0.0.1:47832/alert
```

A quick `GET`:

```sh
curl "http://127.0.0.1:47832/alert?title=Coffee+time&icon=clock&tone=orange"
```

An alert that stays until clicked, without the shake:

```sh
curl -X POST http://127.0.0.1:47832/alert \
  -H 'Content-Type: application/json' \
  -d '{"title":"Server down","message":"db-1 stopped answering","icon":"exclamation","tone":"red","sticky":true,"shake":false}'
```

Dismiss it:

```sh
curl -X DELETE http://127.0.0.1:47832/alert
```

Tell yourself when a long command finishes:

```sh
make release && curl -s -d title="Release built" -d tone=green http://127.0.0.1:47832/alert \
  || curl -s -d title="Release failed" -d tone=red -d icon=exclamation http://127.0.0.1:47832/alert
```

Alerts from another machine, through an SSH tunnel back to your computer (the remote side can then call `127.0.0.1:47832` as if it were local):

```sh
ssh -R 47832:127.0.0.1:47832 you@build-server
```

### Windows (PowerShell)

```powershell
$alert = @{ title = 'Build finished'; message = 'All 212 tests passed'; icon = 'checkmark'; tone = 'green' }
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:47832/alert `
  -ContentType 'application/json; charset=utf-8' -Body ($alert | ConvertTo-Json)
```

A hashtable body is sent as a form, which works too:

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:47832/alert -Body @{ title = 'Deploy done'; tone = 'teal' }
```

A quick `GET`, a sticky alert, and dismissing it:

```powershell
Invoke-RestMethod 'http://127.0.0.1:47832/alert?title=Coffee+time&icon=clock&tone=orange'

Invoke-RestMethod -Method Post -Uri http://127.0.0.1:47832/alert -ContentType 'application/json' `
  -Body (@{ title = 'Server down'; tone = 'red'; icon = 'exclamation'; sticky = $true } | ConvertTo-Json)

Invoke-RestMethod -Method Delete -Uri http://127.0.0.1:47832/alert
```

Windows PowerShell 5.1 sends string bodies in a Western code page, so emoji and accented letters arrive mangled. Send UTF-8 bytes instead (PowerShell 7 already does):

```powershell
$json = @{ title = 'Tests ✅'; message = 'Déploiement terminé' } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:47832/alert `
  -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($json))
```

`curl.exe` ships with Windows 10 and later, so the curl examples above work in PowerShell too if you type `curl.exe` (in Windows PowerShell 5.1, plain `curl` is an alias of `Invoke-WebRequest`) and swap the single quotes around JSON for escaped double quotes.

## Responses

Responses are small JSON objects:

```json
{"ok":true}
```

```json
{"ok":false,"error":"unknown tone \"magenta\""}
```

| Status | When |
| --- | --- |
| `200` | The alert was shown or dismissed. |
| `400` | Bad input: no title, an unknown icon or tone, invalid JSON, JSON that isn't an object, a negative or non-numeric `duration_ms`, a boolean that isn't one, a malformed request. |
| `401` | A token is configured and the request didn't carry it (see [Token](#token)). Includes `WWW-Authenticate: Bearer`. |
| `403` | The request came from a web page or through a foreign host name (see [Security](#security)). |
| `404` | Unknown path. |
| `405` | Wrong method for a known path. Includes an `Allow` header. |
| `408` | The request wasn't fully received within 2 seconds. |
| `411` | A chunked body. Send a `Content-Length` instead (every common client does). |
| `413` | The body is over 16 KiB, or the request line and headers are over 8 KiB. |

Clients that send `Expect: 100-continue` (curl does for large bodies, and so can .NET clients such as Windows PowerShell 5.1) get the interim `100 Continue` right away instead of waiting out their own timer, and a body that is too large is refused before it is sent.

## Tones

`blue` (default), `green`, `orange`, `red`, `purple`, `teal`, `yellow`, `pink`, `gray`.

## Icons

`bell` (default), `star`, `checkmark`, `exclamation`, `xmark`, `clock`, `note`, `gear`, `folder`, `tray`, `plus`, `minus`, `play`, `pause`, `stop`, `record`, `backward`, `forward`, `speaker`, `speaker_muted`, `headphones`, `microphone`, `microphone_muted`, `display`, `device`, `phone`, `watch`, `keyboard`, `mouse`, `gamepad`, `bluetooth`, `pipette`, `viewfinder`, `convert`, `compress`, `combine`, `split`, `chevron_up`, `chevron_down`, `chevron_left`, `chevron_right`, `grid`, `gauge`, `timer`.

These are the SDK's `Icon` names; any icon the SDK adds later works here too.

## Token

By default any program on your computer can send alerts. To require a shared secret, set `token` in `config.json` (it isn't shown in the notch's Settings):

```json
"token": "a-long-random-string"
```

Then every request, including dismissals, must carry it, either as a header (preferred) or in the query string:

```sh
curl -H "Authorization: Bearer a-long-random-string" -d title="Signed" http://127.0.0.1:47832/alert
curl "http://127.0.0.1:47832/alert?title=Signed&token=a-long-random-string"
```

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:47832/alert `
  -Headers @{ Authorization = 'Bearer a-long-random-string' } -Body @{ title = 'Signed' }
```

The `Bearer` scheme is case-insensitive. When a header is present it is the one checked. Query strings end up in shell history and logs, so prefer the header in scripts. An empty or blank token means no token.

## Settings and `config.json`

The notch's Settings show two of the tool's settings; the token is only in `config.json`.

| Key | Setting | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | Webhook alerts | `true` | Off stops the server; the port is released. |
| `port` | Webhook port | `47832` | `1024` to `65535`. Out-of-range numbers are clamped; a value that isn't a number falls back to the default. |
| `token` | (hidden) | `""` | See [Token](#token). |

They live under `tools.alerts.settings` in `config.json` (next to the notchbase executable, or wherever `NOTCHBASE_CONFIG` points; see [Configuration](Configuration.md)):

```json
{
  "tools": {
    "alerts": {
      "enabled": true,
      "settings": {
        "enabled": true,
        "port": 47832,
        "token": ""
      }
    }
  }
}
```

`tools.alerts.settings.enabled` (the **Webhook alerts** switch) stops the webhook. `tools.alerts.enabled` turns the whole tool off, the same as its switch in **Settings › Indicators**; its settings then leave the Settings page too.

Changes apply live, whether made in Settings or by editing the file: when `enabled`, `port` or `token` changes, the old listener stops and a new one starts. If the port can't be taken (another program is listening on it, or the system reserves it), the notch says **Alerts couldn't listen on port 47832**. Pick another port, or free that one, and the tool tries again as soon as the setting changes.

## In the notch

Each alert is a **banner**, laid out like a notification: the colored badge on the left, the title in bold beside it, and the message under the title, wrapped over as many lines as it needs. On an island (every Windows display, and Macs without a notch) the capsule grows to fit it; on a notched Mac the banner hangs below the camera.

- **Motion.** By default the notch jolts sideways and scales up as the alert appears; `"shake": false` keeps just the scale-up.
- **How long.** It stays for `duration_ms`, or until clicked when sticky. A new alert replaces the one showing.
- **Nothing else.** Alerts has no tile, no tab and no history: it only shows the alert. Its two visible settings are on the notch's Settings page, and it's listed under **Settings › Indicators** with the other background tools.

## Security

- **Local only.** The server binds `127.0.0.1`, so other devices on your network can't reach it, even with the firewall off. Use `127.0.0.1` or `localhost` in URLs; it doesn't listen on the IPv6 loopback `::1` (curl falls back to `127.0.0.1` on its own).
- **Any local program can send alerts** unless you set a [token](#token). That includes programs running as other users on a shared computer. Set a token if that matters to you; it is compared in constant time.
- **Web pages are turned away.** A web page you visit could otherwise make your browser call `127.0.0.1` and put text in your notch. Requests that carry an `Origin` header, or a `Sec-Fetch-Site` header other than `none` (which modern browsers add to requests made by pages), are refused with `403`. Typing a `GET /alert?…` URL into the address bar still works, because the browser marks that as `none`.
- **DNS rebinding is blocked.** The `Host` header, when present, must be `127.0.0.1` or `localhost` (any port), so a hostile domain that re-points itself at `127.0.0.1` gets `403`.
- **No CORS.** The server never sends `Access-Control-*` headers, so pages can't read its responses.
- **Bounded work.** Request line and headers are limited to 8 KiB, bodies to 16 KiB, a whole request must arrive within 2 seconds, one request is served per connection, and requests are handled one at a time, so a stuck or flooding client can't grow memory or hold the server for long. Titles and messages are cut to 60 and 140 characters.
- **No other effects.** A request can show an alert or dismiss it. It can't open URLs, run anything, or read anything back.
- To reach it from another machine, tunnel over SSH (see the [example](#macos-and-linux-curl)) rather than exposing the port.

## How it works

The crate keeps each concern in its own small module:

| Module | Role |
| --- | --- |
| `lib.rs` | The `Alerts` tool: its settings, messages, `update`, and starting and restarting the server. |
| `config.rs` | The three settings and how they are read (`Config`), and the webhook URL. |
| `server.rs` | The listener thread: binding, accepting, per-connection deadlines, replying and closing. |
| `http.rs` | A minimal HTTP/1.1 reader and writer over any `Read`: the request line, headers, `Content-Length` bodies, `100 Continue`, size limits, percent-decoding and JSON replies. |
| `webhook.rs` | Routing, the browser and host checks, the token check, and gathering fields from the query, JSON or form body. |
| `alert.rs` | Turning fields into an `Alert` (validation, tidying, limits, durations) and an `Alert` into a banner `Notice`. |

The server uses only `std::net`. `update` starts it with `Command::spawn`; the spawned thread binds `127.0.0.1:<port>` with a nonblocking listener and polls `accept` every 100 ms, checking a shared stop flag between polls. The tool holds a `Server` handle whose `Drop` raises that flag, so replacing the handle (a settings change) or dropping the tool (notchbase quitting or the tool being turned off) stops the old listener within about 100 ms. A restart on the same port retries the bind for up to 1.5 seconds while the old listener lets go.

Each accepted connection gets a 2 second deadline for the whole request. Received alerts and dismissals travel to `update` as messages through the task's `Emitter`, before the `200` is written, and `update` answers them with `Command::notice(...)` or `Command::dismiss_notice()`. The message saying a listener couldn't start (`Unreachable`) carries a generation number, so news from a listener that has already been replaced is ignored.

Tests cover the HTTP reader, routing, validation and auth as pure functions over bytes, real round trips over `TcpStream` (including `100 Continue`, a stalled client, a `413` that isn't lost to a connection reset, and a taken port), and the tool itself through the SDK `Runtime`: alerts arriving through the emitter as banners, sticky alerts and dismissals, and restarts when the port, token or switch changes. Run them with:

```sh
cargo test -p alerts
```
