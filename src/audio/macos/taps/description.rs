//! Describes a tap: a stereo mixdown of an app's processes, muted at the source and private, or one of everything.

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSArray, NSNumber, NSString};

use crate::audio::macos::property::AudioObjectID;

const TAP_NAME: &str = "notchbase";
const UNMUTED: isize = 0;
const MUTED_WHEN_TAPPED: isize = 2;

pub struct TapDescription {
    object: Retained<AnyObject>,
    follows_bundles: bool,
}

impl TapDescription {
    pub fn new(class: &AnyClass, processes: &[AudioObjectID], bundle_ids: &[String]) -> Option<Self> {
        let object = stereo_mixdown(class, processes)?;
        configure(&object, MUTED_WHEN_TAPPED);
        let follows_bundles = !bundle_ids.is_empty() && follow_bundles(&object, bundle_ids);
        Some(Self { object, follows_bundles })
    }

    pub fn everything(class: &AnyClass) -> Option<Self> {
        let nobody = NSArray::<NSNumber>::new();
        let object: Option<Retained<AnyObject>> = unsafe {
            let allocated: Allocated<AnyObject> = msg_send![class, alloc];
            msg_send![allocated, initStereoGlobalTapButExcludeProcesses: &*nobody]
        };
        let object = object?;
        configure(&object, UNMUTED);
        Some(Self { object, follows_bundles: false })
    }

    pub fn follows_bundles(&self) -> bool {
        self.follows_bundles
    }

    pub fn as_ptr(&self) -> *mut AnyObject {
        Retained::as_ptr(&self.object).cast_mut()
    }
}

fn stereo_mixdown(class: &AnyClass, processes: &[AudioObjectID]) -> Option<Retained<AnyObject>> {
    let numbers: Vec<Retained<NSNumber>> = processes.iter().map(|&process| NSNumber::new_u32(process)).collect();
    let list = NSArray::from_retained_slice(&numbers);
    unsafe {
        let allocated: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![allocated, initStereoMixdownOfProcesses: &*list]
    }
}

fn configure(description: &AnyObject, mute_behavior: isize) {
    let name = NSString::from_str(TAP_NAME);
    unsafe {
        let _: () = msg_send![description, setPrivate: true];
        let _: () = msg_send![description, setMuteBehavior: mute_behavior];
        let _: () = msg_send![description, setName: &*name];
    }
    if let Some(uuid) = new_uuid() {
        unsafe {
            let _: () = msg_send![description, setUUID: &*uuid];
        }
    }
}

fn new_uuid() -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"NSUUID")?;
    unsafe { msg_send![class, UUID] }
}

fn follow_bundles(description: &AnyObject, bundle_ids: &[String]) -> bool {
    let responds = |selector: Sel| -> bool { unsafe { msg_send![description, respondsToSelector: selector] } };
    if !responds(sel!(setBundleIDs:)) {
        return false;
    }
    let ids: Vec<Retained<NSString>> = bundle_ids.iter().map(|id| NSString::from_str(id)).collect();
    let ids = NSArray::from_retained_slice(&ids);
    unsafe {
        let _: () = msg_send![description, setBundleIDs: &*ids];
    }
    if responds(sel!(setProcessRestoreEnabled:)) {
        unsafe {
            let _: () = msg_send![description, setProcessRestoreEnabled: true];
        }
    }
    true
}
