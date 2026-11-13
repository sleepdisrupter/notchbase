//! A short alert sound: Glass on a Mac, and the first alarm on Windows, or its notification sound if that's missing.

#[cfg(windows)]
pub fn chime() {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};

    let alarm = std::env::var_os("WINDIR")
        .map(|dir| std::path::PathBuf::from(dir).join("Media").join("Alarm01.wav"))
        .filter(|path| path.exists());
    let (name, kind) = match alarm {
        Some(path) => (path.into_os_string(), SND_FILENAME),
        None => ("SystemNotification".into(), SND_ALIAS),
    };
    let wide: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(name.as_os_str()).chain([0]).collect();
    unsafe { PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), kind | SND_ASYNC | SND_NODEFAULT) };
}

#[cfg(target_os = "macos")]
pub fn chime() {
    let _ = std::process::Command::new("/usr/bin/afplay").arg("/System/Library/Sounds/Glass.aiff").spawn();
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn chime() {}
