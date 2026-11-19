//! The Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver): an
//! open-source indirect display driver, signed, so Windows takes it without test mode. It
//! offers the sizes listed in its settings file and takes commands on a named pipe that anyone
//! may open. Installing it is the one step that asks for administrator rights, through winget.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING, WriteFile,
};
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{HSTRING, w};

use super::monitor;

const PIPE: &str = r"\\.\pipe\MTTVirtualDisplayPipe";
const PACKAGE: &str = "VirtualDrivers.Virtual-Display-Driver";
const DEFAULT_FOLDER: &str = r"C:\VirtualDisplayDriver";
/// How long the driver may take to show up after installing.
const INSTALL_WAIT: Duration = Duration::from_secs(90);
/// How long winget may take, permission prompt and download included.
const WINGET_WAIT: Duration = Duration::from_secs(5 * 60);
/// A console window of its own, even when started from an app without one.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
/// winget's answers for a package that's already there: no applicable upgrade, already
/// installed, and no newer version.
const ALREADY: [u32; 3] = [0x8A15_002B, 0x8A15_0061, 0x8A15_0060];
const NOT_RUNNING: &str = "The display driver is installed, but Windows isn't running it. Restart the PC and try again. If it still doesn't work, look for Virtual Display Driver under Display adapters in Device Manager";

/// Whether the driver is there: its adapter is among Windows' displays.
pub fn installed() -> bool {
    !monitor::virtual_sources().is_empty()
}

/// Installs the driver with winget, which asks the person for administrator rights, and waits
/// for it to show up. winget runs in a window of its own, so its progress and Windows'
/// permission prompt are in front of the person rather than hidden behind a background app.
pub fn install() -> Result<(), String> {
    let mut winget = std::process::Command::new("winget")
        .args(["install", "--id", PACKAGE, "-e", "--silent"])
        .args(["--accept-package-agreements", "--accept-source-agreements"])
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map_err(|err| format!("Couldn't run winget to install the display driver: {err}"))?;
    let started = Instant::now();
    let status = loop {
        match winget.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < WINGET_WAIT => std::thread::sleep(Duration::from_millis(250)),
            Ok(None) => {
                let _ = winget.kill();
                return Err(format!(
                    "The display driver install didn't finish in {} minutes. Run \"winget install --id {PACKAGE} -e\" in a terminal to see why",
                    WINGET_WAIT.as_secs() / 60
                ));
            }
            Err(err) => return Err(format!("Lost track of the display driver install: {err}")),
        }
    };
    let code = status.code().map(|c| c as u32);
    // Nothing was installed, so there's nothing to wait for.
    let wait = if code.is_some_and(|c| ALREADY.contains(&c)) { Duration::from_secs(2) } else { INSTALL_WAIT };
    let started = Instant::now();
    while started.elapsed() < wait {
        if installed() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    match code {
        Some(0) => Err(NOT_RUNNING.into()),
        // Already installed, with nothing newer: the files are there but the driver isn't.
        Some(code) if ALREADY.contains(&code) => Err(NOT_RUNNING.into()),
        // winget's codes are HRESULTs, which read best in hex.
        code => Err(format!(
            "The display driver didn't install (winget said {}). Run \"winget install --id {PACKAGE} -e\" in a terminal to see why",
            code.map_or_else(|| "nothing".to_owned(), |c| format!("0x{c:08X}"))
        )),
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
