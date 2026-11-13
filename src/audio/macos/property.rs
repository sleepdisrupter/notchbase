//! Thin wrappers for reading and writing Core Audio object properties.

use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::ptr;

use objc2::rc::Retained;
use objc2_foundation::NSString;

use super::selectors::{ELEMENT_MAIN, SCOPE_GLOBAL};

pub type AudioObjectID = u32;
pub type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

impl PropertyAddress {
    pub const fn global(selector: u32) -> Self {
        Self::scoped(selector, SCOPE_GLOBAL)
    }

    pub const fn scoped(selector: u32, scope: u32) -> Self {
        Self::element(selector, scope, ELEMENT_MAIN)
    }

    pub const fn element(selector: u32, scope: u32, element: u32) -> Self {
        Self { selector, scope, element }
    }
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectHasProperty(object: AudioObjectID, address: *const PropertyAddress) -> u8;
    fn AudioObjectIsPropertySettable(
        object: AudioObjectID,
        address: *const PropertyAddress,
        settable: *mut u8,
    ) -> OSStatus;
    fn AudioObjectGetPropertyDataSize(
        object: AudioObjectID,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> OSStatus;
    fn AudioObjectGetPropertyData(
        object: AudioObjectID,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
    fn AudioObjectSetPropertyData(
        object: AudioObjectID,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> OSStatus;
}

pub fn has(object: AudioObjectID, address: PropertyAddress) -> bool {
    unsafe { AudioObjectHasProperty(object, &address) != 0 }
}

pub fn settable(object: AudioObjectID, address: PropertyAddress) -> bool {
    let mut out = 0u8;
    unsafe { AudioObjectIsPropertySettable(object, &address, &mut out) == 0 && out != 0 }
}

pub fn data_size(object: AudioObjectID, address: PropertyAddress) -> usize {
    let mut size = 0u32;
    let status = unsafe { AudioObjectGetPropertyDataSize(object, &address, 0, ptr::null(), &mut size) };
    if status == 0 { size as usize } else { 0 }
}

pub fn read<T: Copy>(object: AudioObjectID, address: PropertyAddress) -> Option<T> {
    let mut value = MaybeUninit::<T>::zeroed();
    let mut size = size_of::<T>() as u32;
    let status =
        unsafe { AudioObjectGetPropertyData(object, &address, 0, ptr::null(), &mut size, value.as_mut_ptr().cast()) };
    (status == 0 && size as usize == size_of::<T>()).then(|| unsafe { value.assume_init() })
}

pub fn read_leading<T: Copy>(object: AudioObjectID, address: PropertyAddress) -> Option<T> {
    let size = data_size(object, address);
    if size < size_of::<T>() {
        return None;
    }
    let mut storage = vec![0u64; size.div_ceil(size_of::<u64>())];
    let mut filled = size as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(object, &address, 0, ptr::null(), &mut filled, storage.as_mut_ptr().cast())
    };
    (status == 0).then(|| unsafe { storage.as_ptr().cast::<T>().read_unaligned() })
}

pub fn read_ids(object: AudioObjectID, address: PropertyAddress) -> Vec<AudioObjectID> {
    let mut ids = vec![0u32; data_size(object, address) / size_of::<AudioObjectID>()];
    if ids.is_empty() {
        return ids;
    }
    let mut size = (ids.len() * size_of::<AudioObjectID>()) as u32;
    let status =
        unsafe { AudioObjectGetPropertyData(object, &address, 0, ptr::null(), &mut size, ids.as_mut_ptr().cast()) };
    if status != 0 {
        return Vec::new();
    }
    ids.truncate(size as usize / size_of::<AudioObjectID>());
    ids
}

pub fn read_string(object: AudioObjectID, address: PropertyAddress) -> Option<String> {
    let raw: *mut c_void = read(object, address)?;
    let string = unsafe { Retained::from_raw(raw.cast::<NSString>()) }?;
    Some(string.to_string())
}

pub fn write<T: Copy>(object: AudioObjectID, address: PropertyAddress, value: T) -> bool {
    let data = (&value as *const T).cast::<c_void>();
    unsafe { AudioObjectSetPropertyData(object, &address, 0, ptr::null(), size_of::<T>() as u32, data) == 0 }
}
