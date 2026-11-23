//! The notes for each release, newest first. A line or a release that's only true on one system is built only there.

use super::{Change, Release};
use crate::ui::glyph::Glyph;

const FIND: &str = "Press the magnifier to reach any file, folder or app";
#[cfg(target_os = "macos")]
const LEARNS: &str = "Find learns what you open, only on this Mac";
#[cfg(not(target_os = "macos"))]
const LEARNS: &str = "Find learns what you open, only on this PC";
const DRAWS: &str = "Press Annotate after you take a screenshot";
#[cfg(target_os = "macos")]
const HINGE: &str = "While it is on, closing this Mac neither sleeps nor locks it";
#[cfg(target_os = "macos")]
const PADLOCK: &str = "It shows after you close the lid with All displays on";
#[cfg(target_os = "macos")]
const LID_FLOOR: &str = "Asks for your password once so plugging in can't sleep it";

#[cfg(target_os = "macos")]
const TAKES_OVER: &str = "Find can replace Spotlight";
#[cfg(not(target_os = "macos"))]
const TAKES_OVER: &str = "Find can replace the Start menu";
const SWITCH: &str = "Turn it on or off in General settings";
const YOURS: &str = "Hidden and system files stay out of the list";
#[cfg(target_os = "macos")]
const PANES: &str = "Type wifi or bluetooth to open that pane";

#[cfg(target_os = "macos")]
const KEY: &str = "\u{2318}Space opens Find";
#[cfg(not(target_os = "macos"))]
const KEY: &str = "The Windows key opens Find";
const SWITCH_OFF: &str = "Turn it off in General settings";
const BAR: &str = "It completes the top hit as you type";
#[cfg(target_os = "macos")]
const EVERY_APP: &str = "Every app Spotlight knows is in Find too";

const GITHUB: &str = "New versions now arrive from GitHub releases";
const OPEN: &str = "Read or fork it at github.com/sleepdisrupter/notchbase";

const PIN: &str = "Right-click anything in Find to pin it";
const PINNED: &str = "Drag one to put them in your order";

#[cfg(target_os = "macos")]
const TALK: &str = "Hold fn and talk to type";
#[cfg(not(target_os = "macos"))]
const TALK: &str = "Hold Ctrl + Win and talk";
const GROQ: &str = "Add your free Groq key in the Dictation tool";
const TAP: &str = "Double-tap the key to talk hands-free";
#[cfg(target_os = "macos")]
const LIVE: &str = "Double-tap fn to type as you talk";
#[cfg(not(target_os = "macos"))]
const LIVE: &str = "Double-tap Ctrl + Win to type as you talk";
#[cfg(target_os = "macos")]
const STOP: &str = "Press fn or Esc to stop";
#[cfg(not(target_os = "macos"))]
const STOP: &str = "Press Ctrl + Win or Esc to stop";
const EVERY_APP_TYPES: &str = "Dictation types into every app";
const CLIPBOARD: &str = "Your clipboard is left alone";
const TERMS: &str = "It learns your names and terms";
#[cfg(target_os = "macos")]
const FIXES: &str = "Words you correct are spelled your way next time";
#[cfg(not(target_os = "macos"))]
const FIXES: &str = "Words you say often are spelled your way next time";

#[cfg(target_os = "macos")]
const SHIELD: &str = "The notch stays closed in Mission Control";
#[cfg(target_os = "macos")]
const EXPOSE: &str = "App Expos\u{e9} gets your clicks too";
#[cfg(target_os = "macos")]
const BADGES: &str = "The dock shows app badges";
#[cfg(target_os = "macos")]
const UNREAD: &str = "Unread counts show on each icon";
const LEADS: &str = "Dictation comes first while it listens";
const MIDDLE: &str = "It takes the middle of the notch";

pub(super) static NOTES: &[Release] = &[
    Release {
        version: "0.31.0",
        picture: None,
        changes: &[
            #[cfg(target_os = "macos")]
            Change { glyph: Glyph::Grid, title: SHIELD, caption: EXPOSE },
            #[cfg(target_os = "macos")]
            Change { glyph: Glyph::Bell, title: BADGES, caption: UNREAD },
            Change { glyph: Glyph::Microphone, title: LEADS, caption: MIDDLE },
        ],
    },
    Release {
        version: "0.30.1",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Waveform, title: LIVE, caption: STOP },
            Change { glyph: Glyph::Keyboard, title: EVERY_APP_TYPES, caption: CLIPBOARD },
        ],
    },
    Release {
        version: "0.30.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Microphone, title: TALK, caption: GROQ },
            Change { glyph: Glyph::Waveform, title: "Keep listening without holding", caption: TAP },
            Change { glyph: Glyph::Spark, title: TERMS, caption: FIXES },
        ],
    },
    Release {
        version: "0.24.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Download, title: "Updates come from GitHub", caption: GITHUB },
            Change { glyph: Glyph::Link, title: "notchbase is open source", caption: OPEN },
        ],
    },
    Release {
        version: "0.19.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Pin, title: "Pin what you open most", caption: PIN },
            Change { glyph: Glyph::Search, title: "Four pins under the search bar", caption: PINNED },
        ],
    },
    Release {
        version: "0.18.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Keyboard, title: KEY, caption: SWITCH_OFF },
            Change { glyph: Glyph::Search, title: "Find is a search bar now", caption: BAR },
            #[cfg(target_os = "macos")]
            Change { glyph: Glyph::Search, title: "No app is missed", caption: EVERY_APP },
        ],
    },
    Release {
        version: "0.17.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Keyboard, title: TAKES_OVER, caption: SWITCH },
            Change { glyph: Glyph::Search, title: "Apps and your own files come first", caption: YOURS },
            #[cfg(target_os = "macos")]
            Change { glyph: Glyph::Search, title: "System Settings in Find", caption: PANES },
        ],
    },
    #[cfg(target_os = "macos")]
    Release {
        version: "0.16.0",
        picture: None,
        changes: &[Change {
            glyph: Glyph::Cup,
            title: "Keep Awake really holds with the lid closed",
            caption: LID_FLOOR,
        }],
    },
    #[cfg(target_os = "macos")]
    Release {
        version: "0.15.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Cup, title: "Keep Awake holds with the lid closed", caption: HINGE },
            Change { glyph: Glyph::Lock, title: "The padlock is back on the lock screen", caption: PADLOCK },
        ],
    },
    Release {
        version: "0.14.0",
        picture: None,
        changes: &[
            Change { glyph: Glyph::Search, title: "Find", caption: FIND },
            Change { glyph: Glyph::Clock, title: "Your usual picks come first", caption: LEARNS },
            Change { glyph: Glyph::Viewfinder, title: "Draw and write on screenshots", caption: DRAWS },
        ],
    },
];
