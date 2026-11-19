//! The Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver): an
//! open-source indirect display driver, signed, so Windows takes it without test mode. It
//! offers the sizes listed in its settings file. Roam leaves those alone: the driver's command
//! to read them again can knock it over, so a screen is shown at the nearest size it offers.
//! Installing it is the one step that asks for administrator rights.

use std::time::{Duration, Instant};

use super::monitor;

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
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" -Folder \"{}\"",
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
                "Windows didn't get permission to install the display driver. Try again and say Yes".into()
            } else {
                format!("Couldn't start the display driver install: {}", err.message())
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
            return Err(format!("The display driver install didn't finish in {} minutes", most.as_secs() / 60));
        }
        read.map_err(|err| format!("Lost track of the display driver install: {}", err.message()))?;
        Ok(code)
    }
}
