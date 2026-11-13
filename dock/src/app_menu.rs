//! An app's own actions for its dock menu (New Window, New Private Window, Play, Next…), read from its menu bar through
//! Accessibility. The system Dock gets these from the app directly, which only it can do; the menu bar has the same
//! everyday actions and can be read without anything showing on screen. Reading happens on a thread of its own.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2_foundation::{NSString, ns_string};

/// One action from an app's menu bar.
#[derive(Clone, Debug, PartialEq)]
pub struct AppItem {
    /// The menu it's in, as titled in the menu bar.
    pub menu: String,
    pub title: String,
    /// Bring the app forward after pressing it (new windows), or leave it where it is (playback).
    pub activates: bool,
}

type AxElement = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AxElement;
    fn AXUIElementCopyAttributeValue(element: AxElement, attribute: *const c_void, value: *mut *const c_void) -> i32;
    fn AXUIElementPerformAction(element: AxElement, action: *const c_void) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AxElement, seconds: f32) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(boolean: *const c_void) -> bool;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
}

/// The most actions an app adds to its menu.
const MOST: usize = 6;
/// How long a read stays fresh.
const FRESH: Duration = Duration::from_secs(20);

fn name(s: &NSString) -> *const c_void {
    s as *const NSString as *const c_void
}

/// An owned Accessibility value, released when dropped.
struct Owned(*const c_void);

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn copy(element: AxElement, attribute: &NSString) -> Option<Owned> {
    let mut out = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, name(attribute), &mut out) };
    (error == 0 && !out.is_null()).then(|| Owned(out))
}

fn text(element: AxElement, attribute: &NSString) -> String {
    copy(element, attribute)
        .filter(|v| unsafe { CFGetTypeID(v.0) == CFStringGetTypeID() })
        .map(|v| unsafe { &*(v.0 as *const NSString) }.to_string())
        .unwrap_or_default()
}

fn enabled(element: AxElement) -> bool {
    copy(element, ns_string!("AXEnabled"))
        .filter(|v| unsafe { CFGetTypeID(v.0) == CFBooleanGetTypeID() })
        .is_none_or(|v| unsafe { CFBooleanGetValue(v.0) })
}

/// The children of an element, retained.
fn children(element: AxElement) -> Vec<Owned> {
    let Some(list) = copy(element, ns_string!("AXChildren")) else { return Vec::new() };
    if unsafe { CFGetTypeID(list.0) != CFArrayGetTypeID() } {
        return Vec::new();
    }
    let count = unsafe { CFArrayGetCount(list.0) };
    (0..count)
        .map(|i| unsafe { CFArrayGetValueAtIndex(list.0, i) })
        .filter(|child| !child.is_null())
        .map(|child| Owned(unsafe { CFRetain(child) }))
        .collect()
}

/// Walks `pid`'s menu bar: each top menu's title and its items' elements.
fn menus(pid: i32, mut visit: impl FnMut(&str, &str, AxElement) -> bool) {
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return;
    }
    let app = Owned(app);
    unsafe { AXUIElementSetMessagingTimeout(app.0, 0.4) };
    let Some(bar) = copy(app.0, ns_string!("AXMenuBar")) else {
        debug(format_args!("no menu bar"));
        return;
    };
    for top in children(bar.0) {
        let menu = text(top.0, ns_string!("AXTitle"));
        debug(format_args!("menu {menu}: {} lists", children(top.0).len()));
        for list in children(top.0) {
            for item in children(list.0) {
                let title = text(item.0, ns_string!("AXTitle"));
                if !title.is_empty() && visit(&menu, &title, item.0) {
                    return;
                }
            }
        }
    }
}

fn debug(what: std::fmt::Arguments) {
    if std::env::var_os("NOTCHBASE_DOCK_LOG_MENU").is_some() {
        eprintln!("app menu: {what}");
    }
}

/// Whether a menu bar item belongs in the dock menu, and if so whether it brings the app forward.
pub fn pick(menu: &str, title: &str) -> Option<bool> {
    let playback = ["Controls", "Playback"].contains(&menu);
    let plays = ["Play", "Pause", "Next", "Previous", "Stop"].iter().any(|w| title.starts_with(w));
    if playback && plays {
        return Some(false);
    }
    // New windows, tabs, documents and messages; not folders, which land wherever Finder happens to be.
    let new = title.starts_with("New ") && !title.contains("Folder") && !title.contains("Tab Group");
    new.then_some(true)
}

/// Reads `pid`'s actions. Needs Accessibility; without it there are none.
pub fn read(pid: i32) -> Vec<AppItem> {
    if !unsafe { AXIsProcessTrusted() } {
        return Vec::new();
    }
    autoreleasepool(|_| {
        let mut items: Vec<AppItem> = Vec::new();
        menus(pid, |menu, title, element| {
            if let Some(activates) = pick(menu, title)
                && enabled(element)
                && !items.iter().any(|i| i.title == title)
            {
                items.push(AppItem { menu: menu.to_owned(), title: title.to_owned(), activates });
            }
            items.len() >= MOST
        });
        items
    })
}

/// Presses one action, found again by its menu and title. Runs off the main thread.
pub fn press(pid: i32, item: AppItem) {
    std::thread::spawn(move || {
        autoreleasepool(|_| {
            menus(pid, |menu, title, element| {
                let found = menu == item.menu && title == item.title;
                if found {
                    unsafe { AXUIElementPerformAction(element, name(ns_string!("AXPress"))) };
                }
                found
            });
        });
        if item.activates {
            crate::mac::activate(pid);
        }
    });
}

/// Reads apps' actions ahead of time on a thread, so a menu opens with them already there.
pub struct Reader {
    known: HashMap<i32, (Vec<AppItem>, Instant)>,
    waiting: HashMap<i32, Receiver<Vec<AppItem>>>,
}

impl Reader {
    pub fn new() -> Self {
        Self { known: HashMap::new(), waiting: HashMap::new() }
    }

    /// Starts reading `pid`'s actions unless a fresh read is known or on its way.
    pub fn want(&mut self, pid: i32) {
        let fresh = self.known.get(&pid).is_some_and(|(_, at)| at.elapsed() < FRESH);
        if fresh || self.waiting.contains_key(&pid) {
            return;
        }
        let (send, done) = channel();
        let started = std::thread::Builder::new().name("app menu".into()).spawn(move || {
            let _ = send.send(read(pid));
        });
        if started.is_ok() {
            self.waiting.insert(pid, done);
        }
    }

    /// Takes in finished reads. Returns the pids that arrived.
    pub fn collect(&mut self) -> Vec<i32> {
        let mut arrived = Vec::new();
        self.waiting.retain(|pid, done| match done.try_recv() {
            Ok(items) => {
                self.known.insert(*pid, (items, Instant::now()));
                arrived.push(*pid);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        arrived
    }

    pub fn get(&self, pid: i32) -> &[AppItem] {
        self.known.get(&pid).map_or(&[], |(items, _)| items)
    }
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn picks_new_windows_and_playback_only() {
        assert_eq!(pick("File", "New Window"), Some(true));
        assert_eq!(pick("File", "New Private Window"), Some(true));
        assert_eq!(pick("File", "New Folder"), None);
        assert_eq!(pick("File", "Open…"), None);
        assert_eq!(pick("Playback", "Next"), Some(false));
        assert_eq!(pick("Controls", "Play"), Some(false));
        assert_eq!(pick("Edit", "Play"), None);
        assert_eq!(pick("Window", "Minimize"), None);
    }
}
