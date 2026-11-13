//! Permissions on macOS: each one read through TCC, EventKit, Accessibility or Apple Events, and asked for with the
//! system prompt or the right pane of System Settings.

pub mod accessibility;
pub mod automation;
pub mod tcc;

use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_core_graphics::CGPreflightScreenCaptureAccess;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
use objc2_foundation::{NSArray, NSError};

use super::{Access, Standing};
use crate::platform::{self, KnownDir};

pub const SUPPORTED: bool = true;

const BLUETOOTH: &str = "kTCCServiceBluetoothAlways";
const DOWNLOADS: &str = "kTCCServiceSystemPolicyDownloadsFolder";
const SYSTEM_AUDIO: &str = "kTCCServiceAudioCapture";
const SCREEN: &str = "kTCCServiceScreenCapture";
const PANE: &str = "x-apple.systempreferences:com.apple.preference.security?";
const ANSWER_WAIT: Duration = Duration::from_secs(300);
const ASKER: &str = "notchbase-ask";

const UNANSWERED: u8 = 0;
const ALLOWED: u8 = 1;
const REFUSED: u8 = 2;

static PROMPTED_ACCESSIBILITY: AtomicBool = AtomicBool::new(false);
static DOWNLOADS_ANSWER: AtomicU8 = AtomicU8::new(UNANSWERED);

pub fn standing(access: Access) -> Option<Standing> {
    match access {
        Access::Accessibility => Some(granted_or_undecided(accessibility::trusted())),
        Access::Bluetooth => tcc::preflight(BLUETOOTH),
        Access::Calendar => Some(calendar()),
        Access::Downloads => downloads(),
        Access::Automation => automation::combined(running_players().filter_map(automation::standing)),
        Access::SystemAudio => crate::audio::Listener::supported().then(|| tcc::preflight(SYSTEM_AUDIO)).flatten(),
        Access::ScreenRecording => Some(granted_or_undecided(screen_recording())),
        Access::FullDisk => Some(granted_or_undecided(crate::find::full_disk_access())),
    }
}

pub fn ask(access: Access) {
    if standing(access) == Some(Standing::Denied) {
        open_settings(access);
        return;
    }
    match access {
        Access::Accessibility if !PROMPTED_ACCESSIBILITY.swap(true, Ordering::Relaxed) => accessibility::prompt(),
        Access::Accessibility | Access::FullDisk => open_settings(access),
        Access::ScreenRecording => {
            platform::screen_capture_access(true);
        }
        Access::Bluetooth => in_background(touch_bluetooth),
        Access::Calendar => in_background(ask_for_calendar),
        Access::Downloads => in_background(touch_downloads),
        Access::Automation => in_background(|| {
            for bundle in running_players() {
                automation::ask(bundle);
            }
        }),
        Access::SystemAudio => in_background(crate::audio::Listener::ask_for_access),
    }
}

fn open_settings(access: Access) {
    let _ = platform::open(&format!("{PANE}{}", pane(access)));
}

pub fn prompt_accessibility_once() {
    if !PROMPTED_ACCESSIBILITY.swap(true, Ordering::Relaxed) {
        accessibility::prompt();
    }
}

fn pane(access: Access) -> &'static str {
    match access {
        Access::Accessibility => "Privacy_Accessibility",
        Access::Bluetooth => "Privacy_Bluetooth",
        Access::Calendar => "Privacy_Calendars",
        Access::Downloads => "Privacy_FilesAndFolders",
        Access::Automation => "Privacy_Automation",
        Access::SystemAudio | Access::ScreenRecording => "Privacy_ScreenCapture",
        Access::FullDisk => "Privacy_AllFiles",
    }
}

fn granted_or_undecided(granted: bool) -> Standing {
    if granted { Standing::Granted } else { Standing::Undecided }
}

fn screen_recording() -> bool {
    CGPreflightScreenCaptureAccess() || tcc::preflight(SCREEN) == Some(Standing::Granted)
}

fn calendar() -> Standing {
    match unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) } {
        EKAuthorizationStatus::FullAccess => Standing::Granted,
        EKAuthorizationStatus::NotDetermined => Standing::Undecided,
        _ => Standing::Denied,
    }
}

fn running_players() -> impl Iterator<Item = &'static str> {
    automation::PLAYERS.into_iter().filter(|bundle| automation::running(bundle))
}

fn in_background(work: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new().name(ASKER.into()).spawn(work);
}

fn touch_bluetooth() {
    autoreleasepool(|_| {
        let Some(class) = AnyClass::get(c"IOBluetoothDevice") else { return };
        let _: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![class, pairedDevices] };
    });
}

fn downloads() -> Option<Standing> {
    let asked = tcc::preflight(DOWNLOADS);
    if asked.is_some_and(|standing| standing != Standing::Undecided) {
        return asked;
    }
    match DOWNLOADS_ANSWER.load(Ordering::Relaxed) {
        ALLOWED => Some(Standing::Granted),
        REFUSED => Some(Standing::Denied),
        _ => asked,
    }
}

fn touch_downloads() {
    let Some(folder) = platform::known_dir(KnownDir::Downloads) else { return };
    let answer = match std::fs::read_dir(folder) {
        Ok(_) => ALLOWED,
        Err(err) if err.kind() == ErrorKind::PermissionDenied => REFUSED,
        Err(_) => UNANSWERED,
    };
    DOWNLOADS_ANSWER.store(answer, Ordering::Relaxed);
}

fn ask_for_calendar() {
    let store = unsafe { EKEventStore::new() };
    let (answered, answer) = mpsc::channel();
    let reply = RcBlock::new(move |_granted: Bool, _error: *mut NSError| {
        let _ = answered.send(());
    });
    unsafe { store.requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&reply)) };
    let _ = answer.recv_timeout(ANSWER_WAIT);
}
