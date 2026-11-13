//! The internal battery on macOS, from IOKit's power sources.

use std::ffi::c_void;

use notchbase_sdk::protocol::Power;
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::NSString;

const TYPE: &str = "Type";
const INTERNAL_BATTERY: &str = "InternalBattery";
const CURRENT: &str = "Current Capacity";
const MAX: &str = "Max Capacity";
const CHARGING: &str = "Is Charging";
const STATE: &str = "Power Source State";
const AC_POWER: &str = "AC Power";

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> *mut c_void;
    fn IOPSCopyPowerSourcesList(info: *mut c_void) -> *mut c_void;
    fn IOPSGetPowerSourceDescription(info: *mut c_void, source: *mut c_void) -> *mut c_void;
}

pub(super) fn read() -> Power {
    autoreleasepool(|_| battery()).unwrap_or_default()
}

fn battery() -> Option<Power> {
    let info = unsafe { Retained::from_raw(IOPSCopyPowerSourcesInfo().cast::<AnyObject>()) }?;
    let info_ptr = Retained::as_ptr(&info).cast_mut().cast::<c_void>();
    let sources = unsafe { Retained::from_raw(IOPSCopyPowerSourcesList(info_ptr).cast::<AnyObject>()) }?;
    let count: usize = unsafe { msg_send![&*sources, count] };
    (0..count).find_map(|index| {
        let source: *mut AnyObject = unsafe { msg_send![&*sources, objectAtIndex: index] };
        let described = unsafe { IOPSGetPowerSourceDescription(info_ptr, source.cast()) }.cast::<AnyObject>();
        let described = unsafe { described.as_ref() }?;
        internal(described).then(|| power(described))
    })
}

fn internal(source: &AnyObject) -> bool {
    is(source, TYPE, INTERNAL_BATTERY)
}

fn power(source: &AnyObject) -> Power {
    let (current, max) = (number(source, CURRENT), number(source, MAX));
    let level = match (current, max) {
        (Some(current), Some(max)) if max > 0 => Some((current * 100 / max).clamp(0, 100) as u8),
        _ => None,
    };
    let plugged = is(source, STATE, AC_POWER);
    Power { level, plugged, charging: plugged && flag(source, CHARGING) }
}

fn value(source: &AnyObject, key: &str) -> Option<Retained<AnyObject>> {
    unsafe { msg_send![source, objectForKey: &*NSString::from_str(key)] }
}

fn number(source: &AnyObject, key: &str) -> Option<isize> {
    value(source, key).map(|value| unsafe { msg_send![&*value, integerValue] })
}

fn flag(source: &AnyObject, key: &str) -> bool {
    value(source, key).is_some_and(|value| unsafe { msg_send![&*value, boolValue] })
}

fn is(source: &AnyObject, key: &str, wanted: &str) -> bool {
    value(source, key).is_some_and(|value| unsafe { msg_send![&*value, isEqual: &*NSString::from_str(wanted)] })
}
