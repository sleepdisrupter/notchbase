//! The Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver): an
//! open-source indirect display driver, signed, so Windows takes it without test mode. It
//! offers the sizes listed in its settings file. Roam leaves those alone: the driver's command
//! to read them again can knock it over, so a screen is shown at the nearest size it offers.
//! Installing it, and once giving it room for more than one screen, are the steps that ask
//! for administrator rights.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::w;

use super::monitor;
use crate::display::vdd_settings;

/// The most screens the driver is set to make. It makes as many as the lending computer has
/// (so lending them all needs no change while some are in use), and no more: Windows lists every
/// one it makes in Display settings, greyed out while it's off.
pub const MONITORS: usize = 4;

/// How many screens the driver should make for a computer with `screens` screens.
pub fn monitors_for(screens: usize) -> usize {
    screens.clamp(1, MONITORS)
}
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
pub fn install(sizes: &[[u32; 2]]) -> Result<(), String> {
    let monitors = monitors_for(sizes.len());
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
    let sizes = vdd_settings::missing(&[], sizes);
    let sizes = if sizes.is_empty() { String::new() } else { format!(" -Sizes {sizes}") };
    let arguments = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" -Folder \"{}\" -Monitors {monitors}{sizes}",
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

/// The driver's settings file: in the folder its own installer names, else where Roam's puts it.
fn settings_file() -> Option<PathBuf> {
    let mut named = [0u16; 512];
    let mut bytes = std::mem::size_of_val(&named) as u32;
    // SAFETY: the buffer is as many bytes as `bytes` says.
    let found = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\MikeTheTech\\VirtualDisplayDriver"),
            w!("VDDPATH"),
            RRF_RT_REG_SZ,
            None,
            Some(named.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    }
    .is_ok();
    let named = found.then(|| {
        let end = named.iter().position(|&c| c == 0).unwrap_or(named.len());
        PathBuf::from(String::from_utf16_lossy(&named[..end]))
    });
    named
        .into_iter()
        .chain([PathBuf::from(r"C:\VirtualDisplayDriver")])
        .map(|folder| folder.join("vdd_settings.xml"))
        .find(|file| file.is_file())
}

/// The sizes the driver's settings list, if they can be read.
pub fn listed() -> Option<Vec<[u32; 2]>> {
    let xml = std::fs::read_to_string(settings_file()?).ok()?;
    Some(vdd_settings::sizes(&xml))
}

/// Whether the driver's settings list `size`: `None` when they can't be read.
pub fn offers(size: [u32; 2]) -> Option<bool> {
    listed().map(|sizes| sizes.contains(&size))
}

/// Whether the driver is ready for one more screen `size` big, of a computer with `screens`
/// screens, without asking anyone: a screen of it not in use, as many as that computer has (no
/// fewer, and no spares to clutter Display settings), and that size among its own. Or the person
/// said no to changing it this run, or a screen is in use, and changing the driver now would
/// knock that one over.
pub fn has_room(in_use: usize, size: [u32; 2], screens: usize) -> bool {
    let all = monitor::virtual_sources().len();
    let free = all > in_use;
    let settled = in_use > 0 || ROOM_REFUSED.load(Ordering::Acquire);
    free && (settled || (all == monitors_for(screens) && offers(size) != Some(false)))
}

/// The person was told Windows asks again for more room next time: it will.
pub fn ask_again() {
    ROOM_REFUSED.store(false, Ordering::Release);
}

/// Gets the driver ready for as many screens as `sizes` (one for each of the lending
/// computer's screens), and for each size it lacks: a script
/// run as administrator, after Windows asks the person, changes the driver's settings and
/// restarts its device, which makes the screens again as the settings say. Only while none of
/// its screens is in use, since restarting takes them all away a moment. `in_use` says how
/// many are, once it's this one's turn.
pub fn make_room(in_use: impl Fn() -> usize, sizes: &[[u32; 2]]) -> Result<(), String> {
    let _changing = CHANGING.lock();
    let before = monitor::virtual_sources().len();
    let monitors = monitors_for(sizes.len());
    let missing = listed().map(|listed| vdd_settings::missing(&listed, sizes)).unwrap_or_default();
    if (before == monitors && missing.is_empty()) || in_use() > 0 || ROOM_REFUSED.load(Ordering::Acquire) {
        return Ok(());
    }
    let folder = std::env::temp_dir().join("roam-display-driver");
    let log = folder.join("room.log");
    std::fs::create_dir_all(&folder).map_err(|err| format!("Couldn't make {}: {err}", folder.display()))?;
    let _ = std::fs::remove_file(&log);
    let script = folder.join("room.ps1");
    std::fs::write(&script, ROOM_SCRIPT).map_err(|err| format!("Couldn't write {}: {err}", script.display()))?;
    let sizes = if missing.is_empty() { String::new() } else { format!(" -Sizes {missing}") };
    let arguments = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" -Folder \"{}\" -Monitors {monitors}{sizes}",
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
        return Err(format!("The display driver didn't take the change: {why}"));
    }
    // The driver comes back with its screens on: they stay off until they're wanted. It may
    // still be on its way down when the script ends.
    let started = Instant::now();
    std::thread::sleep(Duration::from_secs(1));
    while started.elapsed() < ROOM_WAIT {
        if monitor::virtual_sources().len() == monitors {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // Windows may take a moment more to list the new sizes.
    std::thread::sleep(Duration::from_millis(500));
    let _ = monitor::detach_unused();
    let after = monitor::virtual_sources().len();
    if after < monitors {
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
