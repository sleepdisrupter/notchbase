//! The few registry calls the installer needs, all under `HKEY_CURRENT_USER`.

use std::io;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};

pub(super) enum Data<'a> {
    Text(&'a str),
    Number(u32),
}

pub(super) fn set(path: &str, name: &str, data: Data<'_>) -> io::Result<()> {
    let path = wide(path);
    let name = wide(name);
    let mut key: HKEY = null_mut();
    let options = REG_OPTION_NON_VOLATILE;
    check(unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            null(),
            options,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        )
    })?;
    let status = match data {
        Data::Text(text) => {
            let text = wide(text);
            let bytes = (text.len() * size_of::<u16>()) as u32;
            unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, text.as_ptr().cast(), bytes) }
        }
        Data::Number(number) => {
            let bytes = size_of::<u32>() as u32;
            unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, (&raw const number).cast(), bytes) }
        }
    };
    unsafe { RegCloseKey(key) };
    check(status)
}

/// A text value, or None when it's missing or isn't text.
pub(super) fn get(path: &str, name: &str) -> Option<String> {
    let path = wide(path);
    let name = wide(name);
    let read = |buffer: *mut u16, size: &mut u32| unsafe {
        RegGetValueW(HKEY_CURRENT_USER, path.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, null_mut(), buffer.cast(), size)
    };
    let mut size = 0;
    if read(null_mut(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(size_of::<u16>())];
    if read(buffer.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer[..size as usize / size_of::<u16>()]);
    Some(text.trim_end_matches('\0').to_owned())
}

pub(super) fn delete_value(path: &str, name: &str) -> io::Result<()> {
    let (path, name) = (wide(path), wide(name));
    gone(unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, path.as_ptr(), name.as_ptr()) })
}

pub(super) fn delete_key(path: &str) -> io::Result<()> {
    let path = wide(path);
    gone(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, path.as_ptr()) })
}

/// Deleting what's already gone counts as done.
fn gone(status: WIN32_ERROR) -> io::Result<()> {
    if status == ERROR_FILE_NOT_FOUND { Ok(()) } else { check(status) }
}

fn check(status: WIN32_ERROR) -> io::Result<()> {
    if status == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(status as i32)) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_and_vanish() {
        let path = format!(r"Software\notchbase-test-{}", std::process::id());
        set(&path, "Text", Data::Text("C:\\notchbase \u{e9}")).unwrap();
        set(&path, "Number", Data::Number(1)).unwrap();
        assert_eq!(get(&path, "Text").as_deref(), Some("C:\\notchbase \u{e9}"));
        assert_eq!(get(&path, "Number"), None, "only text is read");
        delete_value(&path, "Text").unwrap();
        assert_eq!(get(&path, "Text"), None);
        delete_value(&path, "Text").unwrap();
        delete_key(&path).unwrap();
        delete_key(&path).unwrap();
    }
}
