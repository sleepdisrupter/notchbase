//! The browser in front on a Mac. `NSWorkspace` says whether an app opens web links, and the accessibility API, with
//! the Accessibility permission, reads its window.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

use crate::platform::{self, KnownDir};

use super::Browser;

/// Any web address does. It's only used to ask which apps open web links.
const ANY_WEB_PAGE: &str = "https://example.com/";
/// Firefox forks keep their profile folders here, in the home folder.
const PROFILES: &str = "Library/Application Support";
/// How long one accessibility request may wait on the browser, in seconds.
const PATIENCE: f32 = 0.35;
// Limits on the walk through the window, so a busy browser can't hold it up for long.
const LONGEST: Duration = Duration::from_millis(1500);
const MOST_ELEMENTS: usize = 500;
const DEEPEST: usize = 12;
const WIDEST: usize = 80;
const WINDOWS: [&str; 2] = ["AXFocusedWindow", "AXMainWindow"];
const PAGE: &str = "AXWebArea";
const FIELDS: [&str; 2] = ["AXTextField", "AXComboBox"];

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: *const c_void, attribute: *const c_void, value: *mut *mut c_void) -> i32;
}

pub(super) fn frontmost() -> Option<Browser> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let pid = u32::try_from(app.processIdentifier()).ok().filter(|pid| *pid != std::process::id())?;
    let bundle = app.bundleURL()?.path()?.to_string();
    let page = NSURL::URLWithString(&NSString::from_str(ANY_WEB_PAGE))?;
    let opens_web_pages = workspace
        .URLsForApplicationsToOpenURL(&page)
        .iter()
        .filter_map(|app| app.path())
        .any(|path| path.to_string().trim_end_matches('/') == bundle.trim_end_matches('/'));
    if !opens_web_pages {
        return None;
    }
    let identity = app.bundleIdentifier().map(|id| id.to_string()).unwrap_or_default();
    let profiles = platform::known_dir(KnownDir::Home).map(|home| home.join(PROFILES));
    Some(Browser { pid, window: 0, cookies: super::cookies_for(&identity, profiles.as_deref()) })
}

pub(super) fn addresses(browser: &Browser, wanted: &dyn Fn(&str) -> bool) -> Vec<String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Vec::new();
    }
    let Ok(pid) = i32::try_from(browser.pid) else { return Vec::new() };
    autoreleasepool(|_| read(pid, wanted))
}

/// Each engine keeps the address somewhere else, so all are read: Chromium on the window, Gecko on the page
/// element and Safari only in its address field.
fn read(pid: i32, wanted: &dyn Fn(&str) -> bool) -> Vec<String> {
    let Some(app) = (unsafe { Retained::from_raw(AXUIElementCreateApplication(pid).cast::<AnyObject>()) }) else {
        return Vec::new();
    };
    unsafe { AXUIElementSetMessagingTimeout(Retained::as_ptr(&app).cast(), PATIENCE) };
    let Some(window) = front_window(&app) else { return Vec::new() };
    let mut found: Vec<String> = attribute(&window, "AXDocument").as_deref().and_then(text).into_iter().collect();
    if found.iter().any(|address| wanted(address)) {
        return found;
    }
    let started = Instant::now();
    let mut fields = Vec::new();
    let mut queue = VecDeque::from([(window, 0usize)]);
    let mut visited = 0;
    while let Some((element, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MOST_ELEMENTS || started.elapsed() > LONGEST {
            break;
        }
        let role = attribute(&element, "AXRole").as_deref().and_then(text).unwrap_or_default();
        // A page gives its own address, and nothing inside it is worth the walk.
        if role == PAGE {
            let address = attribute(&element, "AXURL").as_deref().and_then(text);
            let is_wanted = address.as_deref().is_some_and(wanted);
            found.extend(address);
            if is_wanted {
                return found;
            }
            continue;
        }
        if FIELDS.contains(&role.as_str()) {
            fields.extend(attribute(&element, "AXValue").as_deref().and_then(text).filter(|value| !value.is_empty()));
        }
        if depth < DEEPEST {
            queue.extend(children(&element).into_iter().map(|child| (child, depth + 1)));
        }
    }
    found.extend(fields);
    found
}

/// The focused window, else the main one, else the first in the app's list.
fn front_window(app: &AnyObject) -> Option<Retained<AnyObject>> {
    WINDOWS
        .iter()
        .find_map(|name| attribute(app, name))
        .or_else(|| attribute(app, "AXWindows").and_then(|windows| items(&windows, 1).into_iter().next()))
}

fn children(element: &AnyObject) -> Vec<Retained<AnyObject>> {
    attribute(element, "AXChildren").map(|children| items(&children, WIDEST)).unwrap_or_default()
}

fn items(array: &AnyObject, most: usize) -> Vec<Retained<AnyObject>> {
    if array.downcast_ref::<objc2_foundation::NSArray>().is_none() {
        return Vec::new();
    }
    let count: usize = unsafe { msg_send![array, count] };
    (0..count.min(most))
        .filter_map(|index| {
            let item: *mut AnyObject = unsafe { msg_send![array, objectAtIndex: index] };
            unsafe { Retained::retain(item) }
        })
        .collect()
}

fn text(value: &AnyObject) -> Option<String> {
    if let Some(string) = value.downcast_ref::<NSString>() {
        return Some(string.to_string());
    }
    value.downcast_ref::<NSURL>()?.absoluteString().map(|address| address.to_string())
}

fn attribute(element: &AnyObject, name: &str) -> Option<Retained<AnyObject>> {
    let name = NSString::from_str(name);
    let mut value: *mut c_void = null_mut();
    let element = (element as *const AnyObject).cast::<c_void>();
    let status = unsafe { AXUIElementCopyAttributeValue(element, Retained::as_ptr(&name).cast(), &mut value) };
    if status != 0 {
        return None;
    }
    unsafe { Retained::from_raw(value.cast::<AnyObject>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads the address of the browser window that is open on this Mac; pass its pid in NOTCHBASE_BROWSER_PID"]
    fn front_window_gives_its_address() {
        let pid: u32 = std::env::var("NOTCHBASE_BROWSER_PID").expect("a pid").parse().expect("a number");
        let started = Instant::now();
        let found = addresses(&Browser { pid, window: 0, cookies: None }, &|_| false);
        eprintln!("{found:?} in {:?}", started.elapsed());
        assert!(found.iter().any(|address| address.contains('.')), "no address in {found:?}");
    }
}
