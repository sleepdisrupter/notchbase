//! Mission Control on a Mac: while it or App Exposé shows, WindowManager puts a shield window over every display,
//! one layer under the Dock's. Only the window numbers are listed on each check, which is cheap; a window is looked
//! up once, when it first shows. Its owner, layer and size need no Screen Recording permission, unlike its name.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

const ON_SCREEN_ONLY: u32 = 1;
const WITHOUT_DESKTOP: u32 = 1 << 4;
const OWNER: &str = "WindowManager";
/// kCGDockWindowLevel (20) less one.
const SHIELD_LAYER: f64 = 19.0;
/// A shield covers a whole display, so anything smaller is some other window.
const SMALLEST: f64 = 400.0;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCreate(option: u32, relative_to: u32) -> *const c_void;
    fn CGWindowListCreateDescriptionFromArray(windows: *const c_void) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFArrayCreate(
        alloc: *const c_void,
        values: *const *const c_void,
        count: isize,
        callbacks: *const c_void,
    ) -> *const c_void;
}

pub(super) fn reader() -> impl FnMut() -> bool {
    let mut shields = HashMap::new();
    move || autoreleasepool(|_| showing(&mut shields))
}

/// `shields` remembers, for each window seen on screen, whether it's a shield.
fn showing(shields: &mut HashMap<u32, bool>) -> bool {
    let on_screen = on_screen();
    shields.retain(|id, _| on_screen.contains(id));
    let new: Vec<u32> = on_screen.iter().copied().filter(|id| !shields.contains_key(id)).collect();
    if !new.is_empty() {
        let found = shields_among(&new);
        shields.extend(new.into_iter().map(|id| (id, found.contains(&id))));
    }
    shields.values().any(|&shield| shield)
}

fn on_screen() -> HashSet<u32> {
    let ids = unsafe { CGWindowListCreate(ON_SCREEN_ONLY | WITHOUT_DESKTOP, 0) };
    if ids.is_null() {
        return HashSet::new();
    }
    let count = unsafe { CFArrayGetCount(ids) };
    let on_screen = (0..count).map(|i| unsafe { CFArrayGetValueAtIndex(ids, i) } as usize as u32).collect();
    unsafe { CFRelease(ids) };
    on_screen
}

fn shields_among(ids: &[u32]) -> HashSet<u32> {
    let values: Vec<*const c_void> = ids.iter().map(|&id| id as usize as *const c_void).collect();
    let array = unsafe { CFArrayCreate(std::ptr::null(), values.as_ptr(), values.len() as isize, std::ptr::null()) };
    if array.is_null() {
        return HashSet::new();
    }
    let info = unsafe { CGWindowListCreateDescriptionFromArray(array) };
    unsafe { CFRelease(array) };
    let Some(list) = (unsafe { Retained::from_raw(info.cast_mut().cast::<NSArray<NSDictionary>>()) }) else {
        return HashSet::new();
    };
    list.iter()
        .filter(|window| is_shield(window))
        .filter_map(|window| number(&window, "kCGWindowNumber"))
        .map(|n| n as u32)
        .collect()
}

fn is_shield(window: &NSDictionary) -> bool {
    let owner = lookup(window, "kCGWindowOwnerName").and_then(|name| name.downcast::<NSString>().ok());
    let bounds = lookup(window, "kCGWindowBounds");
    let bounds = bounds.as_ref().and_then(|bounds| bounds.downcast_ref::<NSDictionary>());
    let side = |name: &str| bounds.and_then(|bounds| number(bounds, name)).unwrap_or(0.0);
    owner.is_some_and(|owner| owner.to_string() == OWNER)
        && number(window, "kCGWindowLayer") == Some(SHIELD_LAYER)
        && side("Width") >= SMALLEST
        && side("Height") >= SMALLEST
}

fn lookup(dictionary: &NSDictionary, key: &str) -> Option<Retained<AnyObject>> {
    let key = NSString::from_str(key);
    let key: &AnyObject = &key;
    dictionary.objectForKey(key)
}

fn number(dictionary: &NSDictionary, key: &str) -> Option<f64> {
    Some(lookup(dictionary, key)?.downcast_ref::<NSNumber>()?.doubleValue())
}
