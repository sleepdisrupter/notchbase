//! The Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver): an
//! open-source indirect display driver, signed, so Windows takes it without test mode. It
//! offers the sizes listed in its settings file. Roam leaves those alone: the driver's command
//! to read them again can knock it over, so a screen is shown at the nearest size it offers.
//! Installing it, and once giving it room for more than one screen, are the steps that ask
//! for administrator rights.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::monitor;

/// How many screens the driver is set to make: as many as anyone lends at once. Each one stays
/// off until it's in use.
pub const MONITORS: usize = 4;
/// What runs as administrator to give the driver room for more screens.
const ROOM_SCRIPT: &str = include_str!("vdd-room.ps1");
/// How long the driver may take to come back with its new screens.
const ROOM_WAIT: Duration = Duration::from_secs(30);
/// One install, or one change to the driver, at a time: two screens asked for at once would
/// otherwise each ask Windows for permission.
static CHANGING: Mutex<()> = Mutex::new(());
/// The person said no to more room this run: don't ask on every screen.
static ROOM_REFUSED: AtomicBool = AtomicBool::new(false);

/// How long the install may take, permission prompt and downloads included.
const INSTALL_RUN: Duration = Duration::from_secs(5 * 60);
/// How long the driver's monitor may take to show up after installing.
const INSTALL_WAIT: Duration = Duration::from_secs(30);
/// What runs as administrator to install the driver.
const SCRIPT: &str = include_str!("vdd-install.ps1");
const NOT_RUNNING: &str =
    "The display driver installed, but Windows hasn't shown its screen yet. Restart the PC and try again";

/// Whether the driver is there and working: its monitors are among Windows' displays, on or
/// off. A driver that runs without making any (as the one in the project's 25.5.2 setup does)
/// counts as missing, so installing replaces it.
pub fn installed() -> bool {
    !monitor::virtual_sources().is_empty()
}

/// Installs the driver as its project's own silent install does: a script, run as
/// administrator after Windows asks the person, downloads the signed driver and NefCon (both
/// pinned by SHA-256), takes away any earlier device and driver, and creates the driver's
/// device. Then waits for its monitor to show up. (The winget package of the same name is only
/// the project's control app, not the driver.)
pub fn install() -> Result<(), String> {
    let _changing = CHANGING.lock();
    if installed() {
        // Another screen asked at the same time, and its install did it.
        return Ok(());
    }
    if cfg!(target_arch = "aarch64") {
        return Err("The display driver doesn't support ARM PCs yet".into());
    }
    let folder = std::env::temp_dir().join("roam-display-driver");
    let log = folder.join("install.log");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).map_err(|err| format!("Couldn't make {}: {err}", folder.display()))?;
    let _ = std::fs::remove_file(&log);
    let script = folder.join("install.ps1");
    std::fs::write(&script, SCRIPT).map_err(|err| format!("Couldn't write {}: {err}", script.display()))?;
    let arguments = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" -Folder \"{}\" -Monitors {MONITORS}",
        script.display(),
        folder.display()
    );
    let code = elevated::run("powershell.exe", &arguments, INSTALL_RUN)?;
    let log = std::fs::read_to_string(&log).unwrap_or_default();
    if code != 0 {
        let why = log.lines().rev().find_map(|line| line.strip_prefix("FAILED: ")).unwrap_or("no reason given");
        return Err(format!("The display driver didn't install: {why}"));
    }
    let started = Instant::now();
    while started.elapsed() < INSTALL_WAIT {
        if installed() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(NOT_RUNNING.into())
}

/// Whether the driver has room for one more screen without asking anyone: a screen of it not in
/// use, and as many as it's meant to have (or the person said no to more this run, or a screen is
/// in use, and changing the driver now would knock that one over).
pub fn has_room(in_use: usize) -> bool {
    let all = monitor::virtual_sources().len();
    let free = all > in_use;
    free && (all >= MONITORS || in_use > 0 || ROOM_REFUSED.load(Ordering::Acquire))
}

/// The person was told Windows asks again for more room next time: it will.
pub fn ask_again() {
    ROOM_REFUSED.store(false, Ordering::Release);
}

/// Gives the driver room for `MONITORS` screens: a script run as administrator, after Windows
/// asks the person, sets the count in the driver's settings and restarts its device, which
/// makes the new screens. Only while none of its screens is in use, since restarting takes
/// them all away a moment. `in_use` says how many are, once it's this one's turn.
pub fn make_room(in_use: impl Fn() -> usize) -> Result<(), String> {
    let _changing = CHANGING.lock();
    let before = monitor::virtual_sources().len();
    if before >= MONITORS || in_use() > 0 || ROOM_REFUSED.load(Ordering::Acquire) {
        return Ok(());
    }
    let folder = std::env::temp_dir().join("roam-display-driver");
    let log = folder.join("room.log");
    std::fs::create_dir_all(&folder).map_err(|err| format!("Couldn't make {}: {err}", folder.display()))?;
    let _ = std::fs::remove_file(&log);
    let script = folder.join("room.ps1");
    std::fs::write(&script, ROOM_SCRIPT).map_err(|err| format!("Couldn't write {}: {err}", script.display()))?;
    let arguments = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" -Folder \"{}\" -Monitors {MONITORS}",
        script.display(),
        folder.display()
    );
    let code = match elevated::run("powershell.exe", &arguments, INSTALL_RUN) {
        Ok(code) => code,
        Err(why) => {
            ROOM_REFUSED.store(true, Ordering::Release);
            return Err(why);
        }
    };
    let log = std::fs::read_to_string(&log).unwrap_or_default();
    if code != 0 {
        ROOM_REFUSED.store(true, Ordering::Release);
        let why = log.lines().rev().find_map(|line| line.strip_prefix("FAILED: ")).unwrap_or("no reason given");
        return Err(format!("The display driver didn't make room for more screens: {why}"));
    }
    // The driver comes back with its screens on: they stay off until they're wanted.
    let started = Instant::now();
    while started.elapsed() < ROOM_WAIT {
        std::thread::sleep(Duration::from_millis(500));
        if monitor::virtual_sources().len() >= MONITORS {
            break;
        }
    }
    let _ = monitor::detach_unused();
    let after = monitor::virtual_sources().len();
    if after < MONITORS {
        ROOM_REFUSED.store(true, Ordering::Release);
    }
    if after == 0 {
        return Err(NOT_RUNNING.into());
    }
    Ok(())
}

/// Running a program as administrator.
mod elevated {
    use std::time::Duration;

    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
    use windows::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{HSTRING, PCWSTR, w};

    /// Runs `program` with `arguments` as administrator, once Windows has asked the person,
    /// and answers its exit code. Gives up after `most`.
    pub fn run(program: &str, arguments: &str, most: Duration) -> Result<u32, String> {
        // The shell may hand the call to its extensions, which want COM on this thread.
        // SAFETY: plain set-up; a thread that already has COM keeps it.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        let (program, arguments) = (HSTRING::from(program), HSTRING::from(arguments));
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(program.as_ptr()),
            lpParameters: PCWSTR(arguments.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        // SAFETY: `info` is filled in as the call expects, and its strings outlive it.
        if let Err(err) = unsafe { ShellExecuteExW(&mut info) } {
            return Err(if err.code() == ERROR_CANCELLED.to_hresult() {
                "Windows didn't get permission to change the display driver. Try again and say Yes".into()
            } else {
                format!("Couldn't start changing the display driver: {}", err.message())
            });
        }
        let process = info.hProcess;
        if process.is_invalid() {
            return Err("Couldn't follow the display driver install".into());
        }
        // SAFETY: the process handle is ours until it's closed below.
        let finished = unsafe { WaitForSingleObject(process, most.as_millis() as u32) } == WAIT_OBJECT_0;
        let mut code = 0;
        // SAFETY: as above.
        let read = unsafe { GetExitCodeProcess(process, &mut code) };
        if !finished {
            // SAFETY: as above.
            let _ = unsafe { TerminateProcess(process, 1) };
        }
        // SAFETY: closes the handle the call above gave.
        let _ = unsafe { CloseHandle(process) };
        if !finished {
            return Err(format!("Changing the display driver didn't finish in {} minutes", most.as_secs() / 60));
        }
        read.map_err(|err| format!("Lost track of the display driver change: {}", err.message()))?;
        Ok(code)
    }
}
