//! Screen Recording permission. The first ask shows macOS's dialog, and later ones open the pane in System Settings.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2_app_kit::NSWorkspace;
use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};
use objc2_foundation::{NSString, NSURL};

const SCREEN_RECORDING_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

static ASKED_FOR_SCREEN_CAPTURE: AtomicBool = AtomicBool::new(false);

pub fn screen_capture_access(request: bool) -> bool {
    if CGPreflightScreenCaptureAccess() {
        return true;
    }
    if request {
        if !ASKED_FOR_SCREEN_CAPTURE.swap(true, Ordering::Relaxed) {
            CGRequestScreenCaptureAccess();
        } else {
            open_screen_recording_settings();
        }
    }
    false
}

fn open_screen_recording_settings() {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(SCREEN_RECORDING_SETTINGS)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
