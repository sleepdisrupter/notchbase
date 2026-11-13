//! Reads the UserAssist counts out of the registry.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegEnumValueW, RegOpenKeyExW,
};

const USER_ASSIST: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist";
const COUNT: &str = "Count";
const NAME_ROOM: usize = 16_384;
const RECORD_ROOM: usize = 256;

pub(super) fn counts(branch: &str) -> Vec<(String, Vec<u8>)> {
    let path: Vec<u16> = format!("{USER_ASSIST}\\{branch}\\{COUNT}").encode_utf16().chain([0]).collect();
    let mut key: HKEY = null_mut();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut name = vec![0u16; NAME_ROOM];
    for index in 0.. {
        let mut record = [0u8; RECORD_ROOM];
        let (mut named, mut sized) = (NAME_ROOM as u32, RECORD_ROOM as u32);
        let status = unsafe {
            RegEnumValueW(
                key,
                index,
                name.as_mut_ptr(),
                &mut named,
                null(),
                null_mut(),
                record.as_mut_ptr(),
                &mut sized,
            )
        };
        match status {
            ERROR_SUCCESS => {
                let name = String::from_utf16_lossy(&name[..(named as usize).min(NAME_ROOM)]);
                found.push((name, record[..(sized as usize).min(RECORD_ROOM)].to_vec()));
            }
            ERROR_MORE_DATA => {}
            _ => break,
        }
    }
    unsafe { RegCloseKey(key) };
    found
}
