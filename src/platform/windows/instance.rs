//! One running copy per data folder, held with a named mutex made from a hash of the folder's path.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, SetLastError,
};
use windows_sys::Win32::System::Threading::CreateMutexW;

const PREFIX: &str = "Local\\notchbase.";
const KEY_BYTES: usize = 12;

pub struct HeldInstance(HANDLE);

impl Drop for HeldInstance {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

pub fn hold_instance(dir: &Path) -> Option<HeldInstance> {
    let name: Vec<u16> = std::ffi::OsStr::new(&mutex_name(dir)).encode_wide().chain([0]).collect();
    let (handle, error) = unsafe {
        SetLastError(0);
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        (handle, GetLastError())
    };
    let held = HeldInstance(handle);
    let taken = error == ERROR_ALREADY_EXISTS || (handle.is_null() && error == ERROR_ACCESS_DENIED);
    (!taken).then_some(held)
}

fn mutex_name(dir: &Path) -> String {
    let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let text = absolute.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let digest = Sha256::digest(text.as_bytes());
    let key: String = digest.iter().take(KEY_BYTES).map(|byte| format!("{byte:02x}")).collect();
    format!("{PREFIX}{key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("notchbase-instance-{name}-{}", std::process::id()))
    }

    #[test]
    fn second_claim_fails_until_dropped() {
        let dir = scratch("twice");
        let first = hold_instance(&dir).expect("the first claim");
        assert!(hold_instance(&dir).is_none(), "a second claim must fail");
        drop(first);
        assert!(hold_instance(&dir).is_some(), "a claim after the drop must succeed");
    }

    #[test]
    fn different_folders_do_not_collide() {
        let first = hold_instance(&scratch("one")).expect("the first folder");
        assert!(hold_instance(&scratch("two")).is_some());
        drop(first);
    }

    #[test]
    fn name_ignores_case_and_slashes() {
        let name = mutex_name(Path::new("C:\\Users\\Me\\AppData\\Roaming\\notchbase"));
        assert_eq!(name, mutex_name(Path::new("c:/users/me/appdata/roaming/notchbase/")));
        assert_ne!(name, mutex_name(Path::new("C:\\Users\\Me\\AppData\\Roaming\\other")));
        assert!(name.starts_with(PREFIX));
        assert_eq!(name.len(), PREFIX.len() + KEY_BYTES * 2);
        assert!(!name[PREFIX.len()..].contains('\\'));
    }
}
