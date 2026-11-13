//! Window layouts on a Mac. Core Graphics lists the windows and where they are, and the accessibility API, which
//! needs the Accessibility permission, moves them.

use std::ffi::c_void;
use std::ptr::null_mut;

use egui::{Pos2, Rect, pos2, vec2};
use objc2::MainThreadMarker;
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSScreen};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, ns_string};
use winit::monitor::MonitorHandle;
use winit::platform::macos::MonitorHandleExtMacOS as _;

use super::{Grab, Handle, Screen};

const ON_SCREEN_ONLY: u32 = 1;
const INCLUDING_WINDOW: u32 = 1 << 3;
const WITHOUT_DESKTOP: u32 = 1 << 4;
const POINT: u32 = 1;
const SIZE: u32 = 2;
/// How long a request to another app may wait, in seconds, so a hung app can't hold things up.
const PATIENCE: f32 = 0.3;
/// Anything smaller is too small to be a window worth laying out.
const SMALLEST: (f32, f32) = (150.0, 100.0);
const NOT_ALLOWED: &str = "Allow Accessibility for notchbase to move windows";
const NOT_MOVED: &str = "Couldn't move that window";
const NOT_FOUND: &str = "That window is gone";
/// System processes whose windows are never laid out.
const OTHER_OWNERS: [&str; 3] = ["Window Server", "Dock", "Control Center"];

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: *const c_void, attribute: *const c_void, value: *mut *mut c_void) -> i32;
    fn AXUIElementSetAttributeValue(element: *const c_void, attribute: *const c_void, value: *const c_void) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> *mut c_void;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> u8;
    fn _AXUIElementGetWindow(element: *const c_void, id: *mut u32) -> i32;
}

struct Entry {
    number: u64,
    pid: u32,
    frame: Rect,
    layer: i64,
    alpha: f64,
    owner: String,
}

pub(super) fn allowed() -> bool {
    crate::system::permissions::macos::accessibility::trusted()
}

pub(super) fn ask_for_access() {
    crate::system::permissions::macos::prompt_accessibility_once();
}

/// The pointer, flipped so y runs down from the top of the main screen like window frames.
pub(super) fn cursor() -> Option<Pos2> {
    let top = primary_height()?;
    let mouse = NSEvent::mouseLocation();
    Some(pos2(mouse.x as f32, (top - mouse.y) as f32))
}

pub(super) fn screens() -> Vec<Screen> {
    let (Some(mtm), Some(top)) = (MainThreadMarker::new(), primary_height()) else { return Vec::new() };
    NSScreen::screens(mtm)
        .iter()
        .filter_map(|screen| {
            let key = screen_number(&screen)?;
            // AppKit measures up from the bottom of the main screen, so frames are flipped to match the window list.
            let flip = |rect: objc2_core_foundation::CGRect| {
                Rect::from_min_size(
                    pos2(rect.origin.x as f32, (top - rect.origin.y - rect.size.height) as f32),
                    vec2(rect.size.width as f32, rect.size.height as f32),
                )
            };
            Some(Screen { key, frame: flip(screen.frame()), work: flip(screen.visibleFrame()), scale: 1.0 })
        })
        .collect()
}

pub(super) fn monitor_key(monitor: &MonitorHandle) -> u64 {
    u64::from(monitor.native_id())
}

/// The topmost ordinary window under the point that isn't notchbase's own.
pub(super) fn window_at(point: Pos2) -> Option<Grab> {
    let me = std::process::id();
    autoreleasepool(|_| listing(ON_SCREEN_ONLY | WITHOUT_DESKTOP, 0)).into_iter().find_map(|entry| {
        (entry.layer == 0 && entry.alpha > 0.01 && entry.pid != me && entry.frame.contains(point))
            .then(|| entry.grab())
            .filter(|grab| !is_sliver(grab.frame))
    })
}

pub(super) fn frame_of(handle: Handle) -> Option<Rect> {
    let id = u32::try_from(handle.id).ok()?;
    autoreleasepool(|_| listing(INCLUDING_WINDOW, id))
        .into_iter()
        .find(|entry| entry.number == handle.id)
        .map(|e| e.frame)
}

pub(super) fn windows_on(screen: &Screen) -> Vec<Grab> {
    let me = std::process::id();
    autoreleasepool(|_| listing(ON_SCREEN_ONLY | WITHOUT_DESKTOP, 0))
        .into_iter()
        .filter(|entry| {
            entry.layer == 0
                && entry.alpha > 0.01
                && entry.pid != me
                && !OTHER_OWNERS.contains(&entry.owner.as_str())
                && !is_sliver(entry.frame)
                && screen.frame.contains(entry.frame.center())
        })
        .map(|entry| entry.grab())
        .collect()
}

pub(super) fn place(handle: Handle, frame: Rect) -> Result<(), String> {
    if !allowed() {
        ask_for_access();
        return Err(NOT_ALLOWED.into());
    }
    autoreleasepool(|_| move_window(handle, frame))
}

impl Entry {
    fn grab(&self) -> Grab {
        Grab { handle: Handle { id: self.number, pid: self.pid }, frame: self.frame }
    }
}

fn is_sliver(frame: Rect) -> bool {
    frame.width() < SMALLEST.0 || frame.height() < SMALLEST.1
}

fn primary_height() -> Option<f64> {
    let mtm = MainThreadMarker::new()?;
    Some(NSScreen::screens(mtm).firstObject()?.frame().size.height)
}

fn screen_number(screen: &NSScreen) -> Option<u64> {
    let value = screen.deviceDescription().objectForKey(ns_string!("NSScreenNumber"))?;
    Some(u64::from(value.downcast::<NSNumber>().ok()?.unsignedIntValue()))
}

fn listing(option: u32, relative_to: u32) -> Vec<Entry> {
    let raw = unsafe { CGWindowListCopyWindowInfo(option, relative_to) };
    let Some(array) = (unsafe { Retained::from_raw(raw.cast_mut().cast::<NSArray<NSDictionary>>()) }) else {
        return Vec::new();
    };
    array.iter().filter_map(|info| entry(&info)).collect()
}

fn entry(info: &NSDictionary) -> Option<Entry> {
    let bounds = lookup(info, "kCGWindowBounds")?;
    let bounds = bounds.downcast_ref::<NSDictionary>()?;
    let side = |name: &str| numeric(bounds, name);
    let frame = Rect::from_min_size(
        pos2(side("X")? as f32, side("Y")? as f32),
        vec2(side("Width")? as f32, side("Height")? as f32),
    );
    let owner = lookup(info, "kCGWindowOwnerName")
        .and_then(|name| name.downcast::<NSString>().ok())
        .map(|name| name.to_string())
        .unwrap_or_default();
    Some(Entry {
        number: numeric(info, "kCGWindowNumber")? as u64,
        pid: numeric(info, "kCGWindowOwnerPID")? as u32,
        frame,
        layer: numeric(info, "kCGWindowLayer")? as i64,
        alpha: numeric(info, "kCGWindowAlpha").unwrap_or(1.0),
        owner,
    })
}

fn lookup(dictionary: &NSDictionary, key: &str) -> Option<Retained<AnyObject>> {
    let key = NSString::from_str(key);
    let key: &AnyObject = &key;
    dictionary.objectForKey(key)
}

fn numeric(dictionary: &NSDictionary, key: &str) -> Option<f64> {
    let value = lookup(dictionary, key)?;
    Some(value.downcast_ref::<NSNumber>()?.doubleValue())
}

fn move_window(handle: Handle, frame: Rect) -> Result<(), String> {
    let pid = i32::try_from(handle.pid).map_err(|_| NOT_FOUND)?;
    let app = unsafe { Retained::from_raw(AXUIElementCreateApplication(pid).cast::<AnyObject>()) }.ok_or(NOT_FOUND)?;
    unsafe { AXUIElementSetMessagingTimeout(Retained::as_ptr(&app).cast(), PATIENCE) };
    let windows = attribute(&app, "AXWindows").ok_or(NOT_MOVED)?;
    let window = find_window(&windows, handle).ok_or(NOT_FOUND)?;
    let position = [f64::from(frame.min.x), f64::from(frame.min.y)];
    let size = [f64::from(frame.width()), f64::from(frame.height())];
    // Size, move, then size again, since the system may cut a size short until the window is in its new place.
    let sized = set(&window, "AXSize", SIZE, size);
    let moved = set(&window, "AXPosition", POINT, position);
    let resized = set(&window, "AXSize", SIZE, size);
    if moved || sized || resized { Ok(()) } else { Err(NOT_MOVED.into()) }
}

/// `_AXUIElementGetWindow` is private, so a window it can't number is matched by its frame instead.
fn find_window(windows: &AnyObject, handle: Handle) -> Option<Retained<AnyObject>> {
    let count: usize = unsafe { msg_send![windows, count] };
    let all: Vec<Retained<AnyObject>> = (0..count)
        .filter_map(|index| {
            let item: *mut AnyObject = unsafe { msg_send![windows, objectAtIndex: index] };
            unsafe { Retained::retain(item) }
        })
        .collect();
    let numbered = |element: &Retained<AnyObject>| {
        let mut id = 0u32;
        let status = unsafe { _AXUIElementGetWindow(Retained::as_ptr(element).cast(), &mut id) };
        (status == 0).then_some(u64::from(id))
    };
    if let Some(found) = all.iter().find(|element| numbered(element) == Some(handle.id)) {
        return Some(found.clone());
    }
    let wanted = frame_of(handle)?;
    all.into_iter().find(|element| current_frame(element).is_some_and(|frame| near(frame, wanted)))
}

fn current_frame(element: &AnyObject) -> Option<Rect> {
    let [x, y] = pair(element, "AXPosition", POINT)?;
    let [w, h] = pair(element, "AXSize", SIZE)?;
    Some(Rect::from_min_size(pos2(x as f32, y as f32), vec2(w as f32, h as f32)))
}

fn near(a: Rect, b: Rect) -> bool {
    (a.min - b.min).length() < 3.0 && (a.size() - b.size()).length() < 3.0
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

fn pair(element: &AnyObject, name: &str, kind: u32) -> Option<[f64; 2]> {
    let wrapped = attribute(element, name)?;
    let mut out = [0.0f64; 2];
    let read = unsafe { AXValueGetValue(Retained::as_ptr(&wrapped).cast(), kind, out.as_mut_ptr().cast()) };
    (read != 0).then_some(out)
}

fn set(element: &AnyObject, name: &str, kind: u32, value: [f64; 2]) -> bool {
    let name = NSString::from_str(name);
    let Some(wrapped) = (unsafe { Retained::from_raw(AXValueCreate(kind, value.as_ptr().cast()).cast::<AnyObject>()) })
    else {
        return false;
    };
    let element = (element as *const AnyObject).cast::<c_void>();
    let status = unsafe {
        AXUIElementSetAttributeValue(element, Retained::as_ptr(&name).cast(), Retained::as_ptr(&wrapped).cast())
    };
    status == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads the real window list of this Mac"]
    fn this_macs_windows_can_be_listed() {
        let entries = listing(ON_SCREEN_ONLY | WITHOUT_DESKTOP, 0);
        for entry in entries.iter().take(8) {
            eprintln!("{} {} {:?} layer {} {}", entry.number, entry.pid, entry.frame, entry.layer, entry.owner);
        }
        assert!(!entries.is_empty());
    }

    #[test]
    fn near_frames_match_far_ones_dont() {
        let a = Rect::from_min_size(pos2(10.0, 10.0), vec2(500.0, 400.0));
        assert!(near(a, a.translate(vec2(1.0, 1.0))));
        assert!(!near(a, a.translate(vec2(30.0, 0.0))));
    }
}
