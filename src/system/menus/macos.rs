//! Reads the front app's menu bar items through Accessibility, keeping the last answer while notchbase is in front.

use std::ffi::c_void;
use std::ptr::null_mut;

use egui::{Rect, pos2, vec2};
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;

const POINT: u32 = 1;
const SIZE: u32 = 2;
const PATIENCE: f32 = 0.25;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: *const c_void, attribute: *const c_void, value: *mut *mut c_void) -> i32;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> u8;
}

enum Seen {
    Items(Vec<Rect>),
    Ourselves,
    Nothing,
}

pub(super) fn reader() -> impl FnMut() -> Vec<Rect> {
    let mut last = Vec::new();
    move || {
        match autoreleasepool(|_| frontmost()) {
            Seen::Items(items) => last = items,
            Seen::Nothing => last.clear(),
            Seen::Ourselves => {}
        }
        last.clone()
    }
}

fn frontmost() -> Seen {
    if !unsafe { AXIsProcessTrusted() } {
        return Seen::Nothing;
    }
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else { return Seen::Nothing };
    let pid = app.processIdentifier();
    if pid == std::process::id() as i32 {
        return Seen::Ourselves;
    }
    Seen::Items(menu_items(pid).unwrap_or_default())
}

fn menu_items(pid: i32) -> Option<Vec<Rect>> {
    let app = unsafe { Retained::from_raw(AXUIElementCreateApplication(pid).cast::<AnyObject>()) }?;
    unsafe { AXUIElementSetMessagingTimeout(Retained::as_ptr(&app).cast(), PATIENCE) };
    let bar = attribute(&app, "AXMenuBar")?;
    let children = attribute(&bar, "AXChildren")?;
    let count: usize = unsafe { msg_send![&*children, count] };
    let items = (0..count).filter_map(|index| {
        let item: *mut AnyObject = unsafe { msg_send![&*children, objectAtIndex: index] };
        let item = unsafe { item.as_ref() }?;
        let [x, y] = value(item, "AXPosition", POINT)?;
        let [width, height] = value(item, "AXSize", SIZE)?;
        Some(Rect::from_min_size(pos2(x as f32, y as f32), vec2(width as f32, height as f32)))
    });
    Some(items.collect())
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

fn value(element: &AnyObject, name: &str, kind: u32) -> Option<[f64; 2]> {
    let wrapped = attribute(element, name)?;
    let mut pair = [0.0f64; 2];
    let read = unsafe { AXValueGetValue(Retained::as_ptr(&wrapped).cast(), kind, pair.as_mut_ptr().cast()) };
    (read != 0).then_some(pair)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads the menu bar of the app in front, which needs Accessibility"]
    fn front_apps_menus_can_be_read() {
        let mut read = reader();
        eprintln!("{:?}", read());
    }
}
