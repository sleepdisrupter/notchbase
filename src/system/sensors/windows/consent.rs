//! Reads the consent store in the registry, where Windows marks which apps are using the camera or microphone now.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_QWORD, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
const NON_PACKAGED: &str = "NonPackaged";
const STARTED: &str = "LastUsedTimeStart";
const STOPPED: &str = "LastUsedTimeStop";
const NAME_ROOM: usize = 256;

pub(super) fn users(capability: &str) -> Vec<String> {
    let packaged = format!("{STORE}\\{capability}");
    let desktop = format!("{packaged}\\{NON_PACKAGED}");
    [packaged, desktop].iter().flat_map(|path| using(path)).collect()
}

fn using(path: &str) -> Vec<String> {
    let Some(key) = Key::open(path) else { return Vec::new() };
    key.children().into_iter().filter(|child| child != NON_PACKAGED && key.in_use(child)).collect()
}

struct Key(HKEY);

impl Key {
    fn open(path: &str) -> Option<Self> {
        let path = wide(path);
        let mut key: HKEY = null_mut();
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) };
        (status == ERROR_SUCCESS).then_some(Self(key))
    }

    fn children(&self) -> Vec<String> {
        let mut names = Vec::new();
        for index in 0.. {
            let mut name = [0u16; NAME_ROOM];
            let mut length = NAME_ROOM as u32;
            let status = unsafe {
                RegEnumKeyExW(self.0, index, name.as_mut_ptr(), &mut length, null(), null_mut(), null_mut(), null_mut())
            };
            if status != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&name[..(length as usize).min(NAME_ROOM)]));
        }
        names
    }

    fn in_use(&self, child: &str) -> bool {
        self.stamp(child, STARTED).is_some_and(|started| started != 0) && self.stamp(child, STOPPED) == Some(0)
    }

    fn stamp(&self, child: &str, value: &str) -> Option<u64> {
        let (child, value) = (wide(child), wide(value));
        let mut stamp = 0u64;
        let mut size = size_of::<u64>() as u32;
        let status = unsafe {
            RegGetValueW(
                self.0,
                child.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_QWORD,
                null_mut(),
                (&raw mut stamp).cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(stamp)
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
