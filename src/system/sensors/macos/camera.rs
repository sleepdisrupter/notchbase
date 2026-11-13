//! Whether any camera is running somewhere, from CoreMediaIO.

use std::ffi::c_void;
use std::ptr::null;

const SYSTEM_OBJECT: u32 = 1;
const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const ELEMENT_MAIN: u32 = 0;
const DEVICES: u32 = u32::from_be_bytes(*b"dev#");
const RUNNING_SOMEWHERE: u32 = u32::from_be_bytes(*b"gone");

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

impl Address {
    const fn global(selector: u32) -> Self {
        Self { selector, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN }
    }
}

#[link(name = "CoreMediaIO", kind = "framework")]
unsafe extern "C" {
    fn CMIOObjectGetPropertyDataSize(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn CMIOObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        used: *mut u32,
        data: *mut c_void,
    ) -> i32;
}

pub(super) fn running() -> bool {
    devices().unwrap_or_default().into_iter().any(is_running)
}

pub(super) fn devices() -> Option<Vec<u32>> {
    let address = Address::global(DEVICES);
    let mut size = 0u32;
    if unsafe { CMIOObjectGetPropertyDataSize(SYSTEM_OBJECT, &address, 0, null(), &mut size) } != 0 {
        return None;
    }
    let mut ids = vec![0u32; size as usize / size_of::<u32>()];
    if ids.is_empty() {
        return Some(ids);
    }
    let mut used = 0u32;
    let room = (ids.len() * size_of::<u32>()) as u32;
    let status = unsafe {
        CMIOObjectGetPropertyData(SYSTEM_OBJECT, &address, 0, null(), room, &mut used, ids.as_mut_ptr().cast())
    };
    ids.truncate(used as usize / size_of::<u32>());
    (status == 0).then_some(ids)
}

fn is_running(device: u32) -> bool {
    let address = Address::global(RUNNING_SOMEWHERE);
    let (mut running, mut used) = (0u32, 0u32);
    let room = size_of::<u32>() as u32;
    let status =
        unsafe { CMIOObjectGetPropertyData(device, &address, 0, null(), room, &mut used, (&raw mut running).cast()) };
    status == 0 && running != 0
}
