# system

Small watchers and switches for the operating system, one folder each:

| Folder | What it does |
| --- | --- |
| `awake` | Keeps the computer awake while something asks for it |
| `bluetooth` | Devices connecting and leaving, with their battery |
| `brightness` | The built-in display's brightness, on a Mac |
| `calendar` | Apple Calendar events, and saving or deleting them |
| `downloads` | Downloads in progress in the Downloads folder |
| `focus` | Focus and Do Not Disturb |
| `menus` | Where the front app's menus end, so the notch can step aside |
| `mission_control` | Whether Mission Control or App Exposé is showing, on a Mac |
| `permissions` | What each feature needs, whether it was granted, and asking for it |
| `power` | The battery level and charger |
| `sensors` | The camera and microphone in use, and muting the microphone |
| `session` | Whether the screen is locked |
| `summon` | The key that opens Find: Command-Space, or a lone Windows key |
| `volume` | Volume, brightness and media keys, and the output level |

Each one has a `macos`, a `windows` and an `other` side with the same calls, and most are built on the workers in `src/poll`. The app switches each on only while something needs it (`src/app/system`), and tools get the feeds through `src/tools/system.rs`.

The wiki has a page for most of them: [System tools](../../wiki/System-Tools.md), [Bluetooth](../../wiki/Bluetooth.md), [Volume and Media Keys](../../wiki/Volume-and-Media-Keys.md) and [Calendar](../../wiki/Calendar.md).

Run the tests with `cargo test system::`. The ones that read the real battery, lock state, camera or devices are ignored.
