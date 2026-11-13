//! The process tap functions and `CATapDescription`, looked up at runtime since they only exist from macOS 14.2.

use std::ffi::c_void;

use objc2::runtime::{AnyClass, AnyObject};

use super::description::TapDescription;
use crate::audio::macos::property::{AudioObjectID, OSStatus};
use crate::audio::macos::symbols::global_symbol;

type CreateTap = unsafe extern "C" fn(description: *mut AnyObject, tap: *mut AudioObjectID) -> OSStatus;
type DestroyTap = unsafe extern "C" fn(tap: AudioObjectID) -> OSStatus;

#[derive(Clone, Copy)]
pub struct TapApi {
    create_process_tap: CreateTap,
    destroy_process_tap: DestroyTap,
    description_class: &'static AnyClass,
}

impl TapApi {
    pub fn load() -> Option<Self> {
        let description_class = AnyClass::get(c"CATapDescription")?;
        let create = global_symbol(c"AudioHardwareCreateProcessTap")?;
        let destroy = global_symbol(c"AudioHardwareDestroyProcessTap")?;
        Some(Self {
            create_process_tap: unsafe { std::mem::transmute::<*mut c_void, CreateTap>(create.as_ptr()) },
            destroy_process_tap: unsafe { std::mem::transmute::<*mut c_void, DestroyTap>(destroy.as_ptr()) },
            description_class,
        })
    }

    pub fn description_class(&self) -> &'static AnyClass {
        self.description_class
    }

    pub fn create_process_tap(&self, description: &TapDescription) -> Option<AudioObjectID> {
        let mut tap = 0;
        let status = unsafe { (self.create_process_tap)(description.as_ptr(), &mut tap) };
        (status == 0 && tap != 0).then_some(tap)
    }

    pub fn destroy_process_tap(&self, tap: AudioObjectID) {
        unsafe { (self.destroy_process_tap)(tap) };
    }
}
