//! The Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver): an
//! open-source indirect display driver, signed, so Windows takes it without test mode. It
//! offers the sizes listed in its settings file and takes commands on a named pipe that anyone
//! may open. Installing it is the one step that asks for administrator rights.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, GENERIC_WRITE, GetLastError};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING, WriteFile,
};
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{HSTRING, w};

use super::monitor;

const PIPE: &str = r"\\.\pipe\MTTVirtualDisplayPipe";
const DEFAULT_FOLDER: &str = r"C:\VirtualDisplayDriver";
/// The driver's own installer, from its project's 25.5.2 release, and what it must hash to.
const SETUP_URL: &str = "https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/download/25.5.2/Virtual.Display.Driver-v25.05.03-setup-x64.exe";
const SETUP_NAME: &str = "Virtual.Display.Driver-v25.05.03-setup-x64.exe";
const SETUP_SHA256: &str = "ca10b85babecfb636c85b3f04d2306968d4f940dd3dd35767f866207bfba846e";
/// How long the person may take with the installer.
const SETUP_RUN: Duration = Duration::from_secs(15 * 60);
/// How long the driver may take to show up after installing.
const INSTALL_WAIT: Duration = Duration::from_secs(30);
const NOT_RUNNING: &str = "The display driver is installed, but Windows isn't running it. Restart the PC and try again. If it still doesn't work, look for Virtual Display Driver under Display adapters in Device Manager";

/// Whether the driver is there: its adapter is among Windows' displays, or it's running and
/// listening for commands.
pub fn installed() -> bool {
    !monitor::virtual_sources().is_empty() || listening()
}

/// Whether the driver's pipe is there. Waiting on a pipe doesn't connect to it, so the driver
/// doesn't notice.
fn listening() -> bool {
    let pipe = HSTRING::from(PIPE);
    // SAFETY: waits by name, for at most a millisecond.
    if unsafe { WaitNamedPipeW(&pipe, 1) }.as_bool() {
        return true;
    }
    // Busy with another client still means it's there; only a missing pipe means it isn't.
    // SAFETY: reads this thread's last error, just set by the call above.
    let error = unsafe { GetLastError() };
    error != ERROR_FILE_NOT_FOUND
}

/// Installs the driver with its project's own installer, the one in its 25.5.2 release,
/// pinned by SHA-256: downloaded, then opened for the person, after Windows asks them for
/// permission. Then waits for the driver to show up. (The winget package of the same name is
/// only the project's control app, not the driver.)
pub fn install() -> Result<(), String> {
    // Installed by hand, or by an earlier try, since Roam last looked.
    if installed() {
        return Ok(());
    }
    if cfg!(target_arch = "aarch64") {
        return Err("The display driver doesn't support ARM PCs yet".into());
    }
    let folder = std::env::temp_dir().join("roam-display-driver");
    std::fs::create_dir_all(&folder).map_err(|err| format!("Couldn't make {}: {err}", folder.display()))?;
    let setup = folder.join(SETUP_NAME);
    if !matches_pin(&setup) {
        download(SETUP_URL, &setup)?;
        if !matches_pin(&setup) {
            let _ = std::fs::remove_file(&setup);
            return Err("The display driver download didn't match the expected file. Try again later".into());
        }
    }
    let code = elevated::run(&setup.to_string_lossy(), "", SETUP_RUN)?;
    match code {
        0 => {}
        // The installer's own answers for being closed before it finished.
        2 | 5 => return Err("The display driver installer was closed before it finished".into()),
        code => return Err(format!("The display driver installer stopped with code {code}")),
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

/// Whether `path` is the pinned installer.
fn matches_pin(path: &std::path::Path) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(bytes) = std::fs::read(path) else { return false };
    let digest = Sha256::digest(&bytes);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    hex == SETUP_SHA256
}

/// Downloads `url` to `to` with the curl that comes with Windows.
fn download(url: &str, to: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut curl = std::process::Command::new("curl.exe")
        .args(["--fail", "--location", "--silent", "--show-error", "--max-time", "300", "--output"])
        .arg(to)
        .arg(url)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| format!("Couldn't download the display driver: {err}"))?;
    let mut why = String::new();
    if let Some(mut stderr) = curl.stderr.take() {
        use std::io::Read;
        let _ = stderr.read_to_string(&mut why);
    }
    let status = curl.wait().map_err(|err| format!("Couldn't download the display driver: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        let why = why.trim();
        Err(format!("Couldn't download the display driver: {}", if why.is_empty() { "no connection" } else { why }))
    }
}

/// Running a program as administrator.
mod elevated {
    use std::time::Duration;

    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
    use windows::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
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
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        // SAFETY: `info` is filled in as the call expects, and its strings outlive it.
        if let Err(err) = unsafe { ShellExecuteExW(&mut info) } {
            return Err(if err.code() == ERROR_CANCELLED.to_hresult() {
                "Windows didn't get permission to install the display driver. Try again and say Yes".into()
            } else {
                format!("Couldn't open the display driver installer: {}", err.message())
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

/// Sends one command to the driver. It answers some on the same pipe, but nothing here needs
/// the answer.
pub fn command(text: &str) -> Result<(), String> {
    let pipe = HSTRING::from(PIPE);
    let open = || {
        // SAFETY: plain open of a named pipe by name.
        unsafe {
            CreateFileW(
                &pipe,
                GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
    };
    let handle = match open() {
        Ok(handle) => handle,
        Err(_) => {
            // Busy with another client: wait for a free instance.
            // SAFETY: waits by name.
            let _ = unsafe { WaitNamedPipeW(&pipe, 2_000) };
            open().map_err(|err| format!("The display driver isn't answering: {}", err.message()))?
        }
    };
    let message: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut written = 0;
    // SAFETY: the handle is open; the buffer lives for the call.
    let sent = unsafe { WriteFile(handle, Some(&message), Some(&mut written), None) };
    // SAFETY: closes what was opened above.
    let _ = unsafe { CloseHandle(handle) };
    sent.map_err(|err| format!("Couldn't talk to the display driver: {}", err.message()))
}

/// Where the driver keeps its settings.
fn settings_file() -> PathBuf {
    let mut buffer = [0u16; 260];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer and its size in bytes are passed together.
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\MikeTheTech\VirtualDisplayDriver"),
            w!("VDDPATH"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    let folder = if read.is_ok() {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(0);
        String::from_utf16_lossy(&buffer[..end])
    } else {
        String::new()
    };
    let folder = if folder.trim().is_empty() { DEFAULT_FOLDER.to_owned() } else { folder };
    PathBuf::from(folder).join("vdd_settings.xml")
}

/// Makes sure the driver offers `size` pixels: adds it to the settings and has the driver read
/// them again if it was missing. Answers whether anything changed.
pub fn offer(size: [u32; 2], hz: u32) -> Result<bool, String> {
    let path = settings_file();
    let text = std::fs::read_to_string(&path).map_err(|err| format!("Couldn't read {}: {err}", path.display()))?;
    let Some(changed) = super::super::settings::with_resolution(&text, size, hz) else { return Ok(false) };
    std::fs::write(&path, changed).map_err(|err| format!("Couldn't change {}: {err}", path.display()))?;
    command("RELOAD_DRIVER")?;
    Ok(true)
}
