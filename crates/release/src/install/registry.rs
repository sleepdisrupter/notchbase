//! Reads the installed version back from the uninstall entry in the registry.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};

use super::{DISPLAY_VERSION, UNINSTALL_KEY};

pub fn registered_version() -> Option<String> {
    let (key, name) = (wide(UNINSTALL_KEY), wide(DISPLAY_VERSION));
    let read = |buffer: *mut u16, size: &mut u32| unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, null_mut(), buffer.cast(), size)
    };
    let mut size = 0;
    // The first call only asks for the size.
    if read(null_mut(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(size_of::<u16>())];
    if read(buffer.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    Some(text_of(&buffer[..size as usize / size_of::<u16>()]))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn text_of(units: &[u16]) -> String {
    String::from_utf16_lossy(units).trim_end_matches('\0').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_are_nul_terminated() {
        assert_eq!(wide("ab"), [u16::from(b'a'), u16::from(b'b'), 0]);
        assert_eq!(wide(""), [0]);
    }

    #[test]
    fn trailing_nuls_are_trimmed() {
        assert_eq!(text_of(&wide("0.16.2")), "0.16.2");
        assert_eq!(text_of(&[0, 0]), "");
    }

    #[test]
    #[ignore = "reads the real registry"]
    fn reads_installed_version() {
        if let Some(version) = registered_version() {
            assert!(!version.is_empty());
        }
    }
}
