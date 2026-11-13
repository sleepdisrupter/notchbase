//! Brings another app's windows forward, with cooperative activation where macOS has it.

use objc2::MainThreadMarker;
use objc2::runtime::NSObjectProtocol as _;
use objc2::sel;
use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};

pub fn focus_window(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return false };
    let Some(target) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else { return false };
    if target.isTerminated() {
        return false;
    }
    handed_over(&target) || target.activateWithOptions(legacy_options())
}

fn handed_over(target: &NSRunningApplication) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let ns_app = NSApplication::sharedApplication(mtm);
    let cooperative = ns_app.respondsToSelector(sel!(yieldActivationToApplication:))
        && target.respondsToSelector(sel!(activateFromApplication:options:));
    if !cooperative || !ns_app.isActive() {
        return false;
    }
    ns_app.yieldActivationToApplication(target);
    target.activateFromApplication_options(
        &NSRunningApplication::currentApplication(),
        NSApplicationActivationOptions::ActivateAllWindows,
    )
}

#[allow(deprecated)]
fn legacy_options() -> NSApplicationActivationOptions {
    NSApplicationActivationOptions::ActivateAllWindows | NSApplicationActivationOptions::ActivateIgnoringOtherApps
}
