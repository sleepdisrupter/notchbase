//! One dock at a time, and a clean way out: the pid file, `--quit` (a note in the dock's folder, which it looks for
//! on every tick), Ctrl-C and the panic hook all end with the taskbar back as it was.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

const PID_FILE: &str = "dock.pid";
const QUIT_FILE: &str = "quit";
static QUIT: AtomicBool = AtomicBool::new(false);
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// `%APPDATA%\notchbase dock`, or `$NOTCHBASE_DOCK_DIR`.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NOTCHBASE_DOCK_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("notchbase dock")
}

pub fn pid_path(dir: &Path) -> PathBuf {
    dir.join(PID_FILE)
}

fn alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
    if handle.is_null() {
        return false;
    }
    let mut code = 0;
    let read = unsafe { GetExitCodeProcess(handle, &mut code) } != 0;
    unsafe { CloseHandle(handle) };
    read && code == STILL_ACTIVE as u32
}

fn is_dock(pid: i32) -> bool {
    super::sys::program_of(pid)
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .is_some_and(|stem| stem == "notchbase-dock")
}

fn read_pid(text: &str) -> Option<i32> {
    text.lines().next()?.trim().parse().ok()
}

/// The pid of another dock that's running with this folder.
pub fn running_instance(dir: &Path) -> Option<i32> {
    let pid = read_pid(&std::fs::read_to_string(pid_path(dir)).ok()?)?;
    (pid != std::process::id() as i32 && alive(pid) && is_dock(pid)).then_some(pid)
}

pub fn claim(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let _ = std::fs::remove_file(dir.join(QUIT_FILE));
    std::fs::write(pid_path(dir), format!("{}\n", std::process::id()))
}

pub fn release(dir: &Path) {
    let ours = std::fs::read_to_string(pid_path(dir)).is_ok_and(|p| read_pid(&p) == Some(std::process::id() as i32));
    if ours {
        let _ = std::fs::remove_file(pid_path(dir));
    }
}

/// Asks the dock at `pid` to quit and waits up to `wait` for it to go.
pub fn ask_to_quit(pid: i32, wait: Duration) -> bool {
    let _ = std::fs::write(data_dir().join(QUIT_FILE), "");
    let start = Instant::now();
    while start.elapsed() < wait {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    !alive(pid)
}

pub fn quit_requested() -> bool {
    if QUIT.load(Ordering::SeqCst) {
        return true;
    }
    let dir = DATA_DIR.get().cloned().unwrap_or_else(data_dir);
    if std::fs::remove_file(dir.join(QUIT_FILE)).is_ok() {
        QUIT.store(true, Ordering::SeqCst);
    }
    QUIT.load(Ordering::SeqCst)
}

/// Windows has no handing over to a newer copy: the taskbar is put back and hidden again by the new one.
pub fn take_handover(_dir: &Path) -> bool {
    false
}

unsafe extern "system" fn on_console(_: u32) -> windows_sys::core::BOOL {
    QUIT.store(true, Ordering::SeqCst);
    // Give the loop a moment to put the taskbar back before the process ends.
    std::thread::sleep(Duration::from_millis(400));
    1
}

pub fn install_handlers(dir: &Path) {
    let _ = DATA_DIR.set(dir.to_path_buf());
    unsafe { SetConsoleCtrlHandler(Some(on_console), 1) };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic while picking a GL config is caught, and the dock carries on.
        if crate::gl::picking_config() {
            return;
        }
        if let Some(dir) = DATA_DIR.get() {
            let _ = super::taskbar::restore(dir);
            release(dir);
        }
        previous(info);
    }));
}
