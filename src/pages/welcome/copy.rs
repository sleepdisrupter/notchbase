//! The walkthrough's words.

use crate::bridge::StepView;
use crate::system::permissions::{Access, Asking, Standing};
use crate::ui::glyph::Glyph;

pub(super) const WELCOME: &str = "Welcome to notchbase";
pub(super) const WELCOME_WHY: &str = "A few permissions let the notch do its best work. You choose each one.";
pub(super) const WELCOME_READY: &str = "Your notch is ready. It already has everything it needs.";
pub(super) const GET_STARTED: &str = "Get Started";
pub(super) const CONTINUE: &str = "Continue";
pub(super) const SKIP: &str = "Skip";
pub(super) const ALLOW: &str = "Allow";
pub(super) const OPEN_SETTINGS: &str = "Open Settings";
pub(super) const NOT_NOW: &str = "Not Now";
pub(super) const ALL_SET: &str = "You're all set";
pub(super) const LATER_IN_SETTINGS: &str = "Change any of these later in Settings, under Permissions.";
pub(super) const NOTHING_MISSING: &str = "notchbase has everything it needs.";
pub(super) const COME_BACK: &str = "Come back here whenever you want to turn one on.";
pub(super) const OPEN_ANY_TIME: &str = "Open the notch from the top of your screen any time.";
pub(super) const DONE: &str = "Done";
const TURN_ON: &str = "Turn on notchbase in System Settings";
const TURN_BACK_ON: &str = "Turn notchbase back on in System Settings";
const ADD_IT: &str = "Turn on notchbase there, or add it with +";
const REOPEN: &str = "Turn on notchbase, then let macOS reopen it";

pub(super) struct Words {
    pub(super) glyph: Glyph,
    pub(super) title: &'static str,
    pub(super) why: &'static str,
}

pub(super) fn words(access: Access) -> Words {
    let (glyph, title, why) = match access {
        Access::Accessibility => {
            (Glyph::Keyboard, "Accessibility", "Shows volume, brightness and the media keys in the notch")
        }
        Access::Bluetooth => (Glyph::Bluetooth, "Bluetooth", "Shows your headphones and their battery as they connect"),
        Access::Calendar => (Glyph::Calendar, "Calendars", "Brings your next meeting into the notch"),
        Access::Downloads => (Glyph::Download, "Downloads Folder", "Follows your downloads as they finish"),
        Access::Automation => (Glyph::Note, "Music and Spotify", "Shows and controls what's playing"),
        Access::SystemAudio => {
            (Glyph::Waveform, "System Audio", "Lets the notch move to your music and set each app's volume")
        }
        Access::ScreenRecording => {
            (Glyph::Viewfinder, "Screen Recording", "Takes screenshots and records your screen from the notch")
        }
        Access::FullDisk => (Glyph::Folder, "Full Disk Access", "Shows your Focus, and lets Find search every folder"),
    };
    Words { glyph, title, why }
}

pub(super) fn primary(step: &StepView) -> &'static str {
    match step.standing {
        Standing::Granted => CONTINUE,
        Standing::Denied => OPEN_SETTINGS,
        Standing::Undecided if sends_to_settings(step) => OPEN_SETTINGS,
        Standing::Undecided => ALLOW,
    }
}

pub(super) fn hint(step: &StepView) -> Option<&'static str> {
    match step.standing {
        Standing::Granted => None,
        Standing::Denied => Some(TURN_BACK_ON),
        Standing::Undecided if step.access == Access::FullDisk => Some(ADD_IT),
        Standing::Undecided if !sends_to_settings(step) => None,
        Standing::Undecided if step.access == Access::ScreenRecording => Some(REOPEN),
        Standing::Undecided => Some(TURN_ON),
    }
}

fn sends_to_settings(step: &StepView) -> bool {
    step.access == Access::FullDisk || (step.asked && step.access.asking() == Asking::Settings)
}
