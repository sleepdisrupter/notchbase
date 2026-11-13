//! Keys a session by its process's image path. The audio engine itself is not an app.

use ::windows::Win32::Foundation::CloseHandle;
use ::windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use ::windows::core::PWSTR;

use crate::audio::keys::pid_key;

const AUDIO_ENGINE: &str = "audiodg.exe";
const MAX_PATH_UNITS: usize = 1024;

pub fn app_key_for_process(pid: u32) -> Option<String> {
    match process_path(pid) {
        Some(path) if is_audio_engine(&path) => None,
        Some(path) => Some(path.to_lowercase()),
        None => Some(pid_key(pid)),
    }
}

fn is_audio_engine(path: &str) -> bool {
    path.rsplit(['\\', '/']).next().is_some_and(|file| file.eq_ignore_ascii_case(AUDIO_ENGINE))
}

fn process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; MAX_PATH_UNITS];
    let mut len = buf.len() as u32;
    let found =
        unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }.is_ok();
    let _ = unsafe { CloseHandle(handle) };
    found.then(|| String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_engine_is_not_an_app() {
        assert!(is_audio_engine(r"C:\Windows\System32\AUDIODG.EXE"));
        assert!(!is_audio_engine(r"C:\Apps\notaudiodg.exe"));
        assert!(!is_audio_engine(r"C:\Apps\spotify.exe"));
    }
}
