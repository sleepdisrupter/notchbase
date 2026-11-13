//! Tells a detachable drive (USB, SD and the like) from a fixed one by the bus it sits on.

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::HSTRING;

const QUERY_PROPERTY: u32 = 0x002D_1400;
const DEVICE_PROPERTY: u32 = 0;
const STANDARD_QUERY: u32 = 0;
const NO_ACCESS: u32 = 0;
const DESCRIPTOR_BYTES: usize = 1024;
const BUS_TYPE_AT: usize = 28;
const USB: u32 = 0x07;
const SD: u32 = 0x0C;
const MMC: u32 = 0x0D;
const DETACHABLE: [u32; 3] = [USB, SD, MMC];
const DEVICE_PREFIX: &str = r"\\.\";
const SEPARATOR: char = '\\';

#[repr(C)]
struct Query {
    property: u32,
    query: u32,
    extra: [u8; 4],
}

pub(super) fn detachable(drive: &Path) -> bool {
    descriptor(drive).is_some_and(|bytes| detachable_bus(&bytes))
}

fn descriptor(drive: &Path) -> Option<Vec<u8>> {
    let device = HSTRING::from(device(drive));
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE;
    let opened =
        unsafe { CreateFileW(&device, NO_ACCESS, share, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) };
    let handle = opened.ok()?;
    let query = Query { property: DEVICE_PROPERTY, query: STANDARD_QUERY, extra: [0; 4] };
    let mut bytes = vec![0u8; DESCRIPTOR_BYTES];
    let mut returned = 0u32;
    let asked = unsafe {
        DeviceIoControl(
            handle,
            QUERY_PROPERTY,
            Some(std::ptr::from_ref(&query).cast::<c_void>()),
            size_of::<Query>() as u32,
            Some(bytes.as_mut_ptr().cast::<c_void>()),
            DESCRIPTOR_BYTES as u32,
            Some(&raw mut returned),
            None,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    asked.ok()?;
    bytes.truncate(returned as usize);
    Some(bytes)
}

fn device(drive: &Path) -> String {
    format!("{DEVICE_PREFIX}{}", drive.to_string_lossy().trim_end_matches(SEPARATOR))
}

fn detachable_bus(descriptor: &[u8]) -> bool {
    let Some(four) = descriptor.get(BUS_TYPE_AT..BUS_TYPE_AT + 4) else { return false };
    DETACHABLE.contains(&u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SATA: u32 = 0x0B;
    const NVME: u32 = 0x11;

    fn on_bus(bus: u32) -> Vec<u8> {
        let mut bytes = vec![0; BUS_TYPE_AT + 8];
        bytes[BUS_TYPE_AT..BUS_TYPE_AT + 4].copy_from_slice(&bus.to_le_bytes());
        bytes
    }

    #[test]
    fn usb_and_cards_are_detachable() {
        assert!([USB, SD, MMC].into_iter().all(|bus| detachable_bus(&on_bus(bus))));
        assert!([SATA, NVME, 0].into_iter().all(|bus| !detachable_bus(&on_bus(bus))));
    }

    #[test]
    fn short_descriptor_not_detachable() {
        assert!(!detachable_bus(&on_bus(USB)[..BUS_TYPE_AT + 3]) && !detachable_bus(&[]));
    }

    #[test]
    fn letter_asked_through_volume() {
        assert_eq!(device(Path::new("E:\\")), r"\\.\E:");
        assert_eq!(device(Path::new("E:")), r"\\.\E:");
    }
}
