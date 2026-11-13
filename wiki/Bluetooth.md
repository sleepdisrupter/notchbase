# Bluetooth

When a Bluetooth device connects or disconnects (headphones, a keyboard, a mouse, a controller), the notch shows it with the device's kind, name and battery level, the way an iPhone does for AirPods. `src/system/bluetooth` finds connected devices and turns snapshots into changes; `src/notch/activity/device.rs` paints the indicator.

## The service (`system/bluetooth/watch.rs`)

`Bluetooth::start()` spawns the `notchbase-bluetooth` thread, which every `bluetooth.poll_ms` (1.5 seconds by default, at least 250 ms; see [Configuration](Configuration.md)):

1. asks the platform `Scanner` for the currently **connected** devices,
2. feeds that list to the `Tracker`, which turns it into `Change`s,
3. reads battery levels where the tracker wants them,
4. appends the changes to a shared list.

The app drains the list each frame (`Bluetooth::take_changes`) and calls `Notch::show_device(&change)` on every notch. Changes are ignored while a [preview](Previews.md) is running.

A `Device` is `{ id, name, kind, battery }`, and a `Change` is `Connected(device)`, `Disconnected(device)` or `Updated(device)` (a battery reading arrived).

## Turning snapshots into changes (`system/bluetooth/tracker.rs`)

Polling only tells what is connected right now, so the tracker keeps what it saw before:

- **The first scan primes the tracker.** Devices already connected when notchbase starts do not produce notices.
- **A new device** produces `Connected`.
- **A device that disappears** is not reported right away. It waits **2.5 seconds**; if it comes back within that time (Bluetooth devices often blip while switching profiles or reconnecting), nothing is shown and its known battery level is kept. Only after the settle time does it produce `Disconnected`.
- **Battery.** For **12 seconds** after a device connects, the tracker keeps asking for its battery level (many devices only report it a moment after connecting). A new, different level produces `Updated`. A level is only accepted between 1 and 100.

## Classifying devices (`system/bluetooth/kind.rs`)

The kind picks the glyph: headphones, speaker, keyboard, mouse, controller, phone, computer, watch, or other. It is decided from the best available hint, in order:

1. **The name**, matched against keywords: "airpods", "buds", "headphone", "headset", "beats", "wh-1000" and similar for headphones; "speaker", "soundbar", "homepod", "sonos" for speakers; "keyboard", "keys"; "mouse", "trackpad", "trackball", "mx master"; "controller", "gamepad", "xbox", "dualsense", "joy-con", "8bitdo"; "watch"; "iphone", "pixel", "galaxy s" for phones.
2. **The Bluetooth Class of Device** (major and minor class): computers, phones, audio (speakers for loudspeaker-style minor classes, headphones otherwise), peripherals (keyboards, pointing devices, gamepads and joysticks), wearable watches, toy controllers.
3. **The Bluetooth LE appearance** value, when a platform provides one.

The name comes first because it is the most reliable signal in practice: many headsets report generic classes.

## Windows (`system/bluetooth/windows`)

Windows exposes paired Bluetooth devices as device nodes through the SetupAPI (`windows/devices.rs`):

- `SetupDiGetClassDevsW` lists the **present** nodes in the Bluetooth device class, and the scanner treats those as the connected devices. Only real device nodes count: instance ids starting with `BTHENUM\DEV_` (classic) or `BTHLE\DEV_` (Low Energy); service and radio nodes are skipped.
- The name is the node's `DEVPKEY_Device_FriendlyName`.
- One physical device often has several nodes (one per profile or service). They share a **container id** (`DEVPKEY_Device_ContainerId`), which is used as the device's id, so each device appears once.
- The Class of Device comes from a Bluetooth device property (major and minor class bits are extracted from it).
- **Battery**: the level is a device property (a byte) that Windows publishes on one of the nodes in the device's container, often a node outside the Bluetooth class (for example the audio or HID node). The scanner lists all present nodes and takes the first battery value found in the same container.

## macOS (`system/bluetooth/macos.rs`)

On macOS the scanner uses the IOBluetooth framework through the Objective-C runtime:

- `IOBluetoothDevice.pairedDevices` gives the paired devices; only those with `isConnected` are kept.
- The id is the device's address string, the name its `name` (falling back to the address), and the Class of Device comes from `deviceClassMajor` and `deviceClassMinor`.
- **Battery** uses values IOBluetooth provides through key-value coding, when the device object responds to them: a single level, a combined level, or left and right levels (the lower of the two is shown, like earbuds in a case report per bud).

The bundle's `Info.plist` includes `NSBluetoothAlwaysUsageDescription`, which macOS shows when notchbase first uses Bluetooth; see [Platform: macOS](Platform-macOS.md).

## The notch indicator (`notch/activity/device.rs`)

A device change becomes a device activity (see [Notch](Notch.md#device-indicator)):

- a glyph for the kind (headphones, speaker, keyboard, mouse, gamepad, phone, a display for computers, watch, or the Bluetooth glyph for anything else) on the leading side, tinted blue when connecting,
- the device name,
- on the trailing side: a battery ring with the percentage when the device connected and its level is known (green, orange at 20% or below, red at 10% or below); otherwise the word "Connected" or "Disconnected" on an island, or a Bluetooth glyph (blue when connected) on a hardware notch.

On an island the indicator is a 264 x 37 capsule with the name between the glyph and the status. On a hardware notch the glyph and the status sit in the lanes beside the camera and the name is centered below the camera row. It shows for 3.2 seconds for a connection and 2.2 seconds for a disconnection by default. The durations (`notices.device_connected_ms`, `notices.device_disconnected_ms`), the polling interval, and whether the indicator (`bluetooth.indicator`) and battery levels (`bluetooth.battery`) appear at all are settings in [Configuration](Configuration.md). An `Updated` change for the device on screen refreshes its battery ring in place without restarting the activity.

Every change is also broadcast to tools that subscribe to Bluetooth (`app/tools/feeds.rs`), as `Connected`, `Disconnected` or `Updated` with the device's id, name, kind (`headphones`, `speaker`, `keyboard`, `mouse`, `controller`, `phone`, `computer`, `watch` or `other`) and battery. Tools receive changes even when the notch indicator is turned off. See [tools/Subscriptions.md](tools/Subscriptions.md).

## Other platforms

`system/bluetooth/other.rs` reports no devices, so nothing is ever shown.

## Tests

`system/bluetooth/tracker.rs` tests that devices present at launch stay quiet, that a short blip is not a disconnect, and that a late battery reading updates a device once, all with synthetic clocks. `system/bluetooth/kind.rs` tests classification by name, class and appearance. `notch/activity/device.rs` tests the battery colors. On Windows an ignored test reads this computer's real Bluetooth devices; run it deliberately with `cargo test -- --ignored` (see [Building and Testing](Building-and-Testing.md#ignored-tests)).
