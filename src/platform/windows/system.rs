//! Windows basics: the known folders, the data folders in AppData, local time and opening things with the shell, as
//! the user or as administrator.

use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::ERROR_CANCELLED;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures, FOLDERID_Profile, FOLDERID_Videos,
    KF_FLAG_DEFAULT, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SHGetKnownFolderPath, ShellExecuteExW, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use winit::event_loop::{DeviceEvents, EventLoop, EventLoopBuilder};
use winit::monitor::MonitorHandle;

use crate::platform::{KnownDir, LocalTime};

pub const MODIFIER_LABELS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Win"];
pub const KEY_JOINER: &str = "+";
const AS_ADMIN_THREAD: &str = "notchbase-run-as-admin";

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

/// Whether `monitor` is a virtual display (Roam's extended display, from the Virtual Display Driver). Its pictures show
/// on another computer's screen, where a notch would sit over that computer's own and catch the clicks made there.
pub fn virtual_display(monitor: &MonitorHandle) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{DISPLAY_DEVICEW, EnumDisplayDevicesW};
    const DRIVERS: [&str; 3] = ["virtual display driver", "iddsampledriver", "mttvdd"];
    let Some(name) = monitor.name() else { return false };
    let text = |wide: &[u16]| {
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..end])
    };
    for index in 0.. {
        // SAFETY: a zeroed DISPLAY_DEVICEW with its size set, as the call expects.
        let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0) } == 0 {
            return false;
        }
        if text(&device.DeviceName) != name {
            continue;
        }
        let about = format!("{} {}", text(&device.DeviceString), text(&device.DeviceID)).to_ascii_lowercase();
        return DRIVERS.iter().any(|driver| about.contains(driver));
    }
    false
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

/// Runs `target` as administrator, behind the UAC prompt, as "Run as administrator" in the Start menu does. Windows
/// waits for the prompt to be answered before it returns, so this runs on a thread of its own and the notch keeps
/// drawing. `failed` hears why it didn't run; a declined prompt is the user's answer and says nothing.
pub fn run_as_admin(target: &str, failed: impl FnOnce(String) + Send + 'static) -> Result<(), String> {
    let target = target.to_owned();
    let work = move || {
        let joined = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.is_ok();
        let ran = elevated(&target);
        if joined {
            unsafe { CoUninitialize() };
        }
        if let Err(err) = ran {
            failed(err);
        }
    };
    std::thread::Builder::new().name(AS_ADMIN_THREAD.into()).spawn(work).map(drop).map_err(|err| err.to_string())
}

fn elevated(target: &str) -> Result<(), String> {
    let file = HSTRING::from(target);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(u32::MAX),
        fMask: SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    match unsafe { ShellExecuteExW(&raw mut info) } {
        Ok(()) => Ok(()),
        Err(err) if err.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
        Err(err) => Err(format!("{target}: {}", err.message())),
    }
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
