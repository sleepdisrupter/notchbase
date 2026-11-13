//! Windows basics: the known folders, the data folders in AppData, local time and opening things with the shell.

use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures, FOLDERID_Profile, FOLDERID_Videos,
    KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use winit::event_loop::{DeviceEvents, EventLoop, EventLoopBuilder};
use winit::monitor::MonitorHandle;

use crate::platform::{KnownDir, LocalTime};

pub const MODIFIER_LABELS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Win"];
pub const KEY_JOINER: &str = "+";

pub fn setup_event_loop<T>(_builder: &mut EventLoopBuilder<T>) {
    super::pacing::keep_full_speed();
}

pub fn let_hooks_hear_our_windows<T>(event_loop: &EventLoop<T>) {
    // While we're registered for raw input, Windows skips our keyboard hooks for keys typed into our own windows.
    event_loop.listen_device_events(DeviceEvents::Never);
}

pub fn hardware_notch(_monitor: &MonitorHandle) -> Option<egui::Vec2> {
    None
}

pub fn videos_dir() -> Option<PathBuf> {
    known_dir(KnownDir::Videos)
}

pub fn known_dir(dir: KnownDir) -> Option<PathBuf> {
    let (id, fallback) = match dir {
        KnownDir::Home => (&FOLDERID_Profile, ""),
        KnownDir::Pictures => (&FOLDERID_Pictures, "Pictures"),
        KnownDir::Videos => (&FOLDERID_Videos, "Videos"),
        KnownDir::Desktop => (&FOLDERID_Desktop, "Desktop"),
        KnownDir::Downloads => (&FOLDERID_Downloads, "Downloads"),
        KnownDir::Documents => (&FOLDERID_Documents, "Documents"),
    };
    let known = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok().and_then(|path| {
        let text = unsafe { path.to_string() }.ok();
        unsafe { CoTaskMemFree(Some(path.0 as *const c_void)) };
        text
    });
    known.map(PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(fallback)))
}

pub fn open(target: &str) -> Result<(), String> {
    let file = HSTRING::from(target);
    let result = unsafe { ShellExecuteW(None, w!("open"), &file, None, None, SW_SHOWNORMAL) };
    if result.0 as usize > 32 { Ok(()) } else { Err(format!("Couldn't open {target}")) }
}

pub fn local_time() -> LocalTime {
    let mut now = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut now) };
    LocalTime {
        year: i32::from(now.wYear),
        month: u32::from(now.wMonth),
        day: u32::from(now.wDay),
        hour: u32::from(now.wHour),
        minute: u32::from(now.wMinute),
        second: u32::from(now.wSecond),
    }
}

pub fn default_local_data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("notchbase"))
}

pub fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("notchbase"))
}
