//! The events other threads send the event loop, and the wake that sends them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use global_hotkey::GlobalHotKeyEvent;
use winit::event_loop::EventLoopProxy;

use crate::bridge::ToolSlot;
use crate::poll::{Wake, lock};

#[derive(Debug)]
pub(super) enum UserEvent {
    HotKey(GlobalHotKeyEvent),
    Wake,
    ScreenshotNotCopied(String),
    // Only Windows runs an app as administrator, on a thread that waits for the UAC prompt.
    #[cfg_attr(not(windows), allow(dead_code))]
    NotRunAsAdmin(String),
    FolderPicked(ToolSlot, String, PathBuf),
    // Only a Mac can be kept awake with the lid closed.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    LidFloor {
        enabled: bool,
        ok: bool,
    },
    Summon,
    Quit,
}

pub(super) fn sender(proxy: &EventLoopProxy<UserEvent>) -> impl Fn(UserEvent) + Send + Sync + 'static {
    let proxy = Mutex::new(proxy.clone());
    move |event| {
        let _ = lock(&proxy).send_event(event);
    }
}

/// Set while a Wake is on its way to the event loop and not yet handled.
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);

/// Wakes the event loop, but only once until that wake is handled. A worker that wakes hundreds of times a second
/// (Find's file watcher while the disk is busy, say) would otherwise fill the queue: on Windows every event is a
/// posted message, and posted messages come before the mouse and before painting, so the notch stopped drawing and
/// stopped answering the mouse until the burst was over.
pub(super) fn wake(proxy: &EventLoopProxy<UserEvent>) -> Wake {
    let send = sender(proxy);
    Arc::new(move || {
        if !WAKE_PENDING.swap(true, Ordering::AcqRel) {
            send(UserEvent::Wake);
        }
    })
}

/// The event loop took the pending Wake: the next wake sends a new one. Called before the work, so a wake that
/// comes while it runs isn't lost.
pub(super) fn woke() {
    WAKE_PENDING.store(false, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wakes_coalesce_until_handled() {
        woke();
        assert!(!WAKE_PENDING.swap(true, Ordering::AcqRel), "the first wake is sent");
        assert!(WAKE_PENDING.swap(true, Ordering::AcqRel), "more wakes before it's handled are not");
        woke();
        assert!(!WAKE_PENDING.swap(true, Ordering::AcqRel), "once handled, the next one is sent again");
        woke();
    }
}
