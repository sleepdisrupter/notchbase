//! The private aggregate device a tap plays through, with the default output as its main device.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

use super::TapError;
use super::process_tap::ProcessTap;
use crate::audio::macos::property::{AudioObjectID, OSStatus};

const AGGREGATE_UID_PREFIX: &str = "app.notchbase.mixer.";
const AGGREGATE_NAME: &str = "notchbase mixer";

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioHardwareCreateAggregateDevice(description: *const c_void, device: *mut AudioObjectID) -> OSStatus;
    fn AudioHardwareDestroyAggregateDevice(device: AudioObjectID) -> OSStatus;
}

pub fn is_mixer_aggregate(uid: &str) -> bool {
    uid.starts_with(AGGREGATE_UID_PREFIX)
}

pub struct Aggregate {
    device: AudioObjectID,
    _tap: ProcessTap,
}

impl Aggregate {
    pub fn create(output_uid: &str, tap: ProcessTap) -> Result<Self, TapError> {
        let tap_uid = tap.uid().ok_or(TapError::Failed)?;
        let description = aggregate_description(output_uid, &tap_uid);
        let mut device = 0;
        let status = unsafe { AudioHardwareCreateAggregateDevice(Retained::as_ptr(&description).cast(), &mut device) };
        if device == 0 {
            return Err(TapError::Failed);
        }
        let aggregate = Self { device, _tap: tap };
        if status != 0 {
            return Err(TapError::Failed);
        }
        Ok(aggregate)
    }

    pub fn device(&self) -> AudioObjectID {
        self.device
    }
}

impl Drop for Aggregate {
    fn drop(&mut self) {
        unsafe { AudioHardwareDestroyAggregateDevice(self.device) };
    }
}

fn aggregate_description(output_uid: &str, tap_uid: &str) -> Retained<NSDictionary<NSString, AnyObject>> {
    let (on, off) = (NSNumber::new_i32(1), NSNumber::new_i32(0));
    let output = NSString::from_str(output_uid);
    let tap = NSString::from_str(tap_uid);
    let uid = NSString::from_str(&format!("{AGGREGATE_UID_PREFIX}{}.{tap_uid}", std::process::id()));
    let name = NSString::from_str(AGGREGATE_NAME);

    let sub_device_values: [&AnyObject; 1] = [&output];
    let sub_device = dictionary(&["uid"], &sub_device_values);
    let sub_tap_values: [&AnyObject; 2] = [&tap, &on];
    let sub_tap = dictionary(&["uid", "drift"], &sub_tap_values);
    let sub_devices = NSArray::from_retained_slice(&[sub_device]);
    let taps = NSArray::from_retained_slice(&[sub_tap]);

    let keys = ["uid", "name", "master", "private", "stacked", "tapautostart", "subdevices", "taps"];
    let values: [&AnyObject; 8] = [&uid, &name, &output, &on, &off, &on, &sub_devices, &taps];
    dictionary(&keys, &values)
}

fn dictionary(keys: &[&str], values: &[&AnyObject]) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<Retained<NSString>> = keys.iter().map(|key| NSString::from_str(key)).collect();
    let keys: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    NSDictionary::from_slices(&keys, values)
}
