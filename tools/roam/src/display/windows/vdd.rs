//! The Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver): an
//! open-source indirect display driver, signed, so Windows takes it without test mode. It
//! offers the sizes listed in its settings file and takes commands on a named pipe that anyone
//! may open. Installing it is the one step that asks for administrator rights, through winget.

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

/// Whether the driver is there: its adapter is among Windows' displays.
pub fn installed() -> bool {
    !monitor::virtual_sources().is_empty()
}

/// Installs the driver with winget, which asks the person for administrator rights, and waits
/// for it to show up.
pub fn install() -> Result<(), String> {
    let status =
        hidden(std::process::Command::new("winget").args(["install", "--id", PACKAGE, "-e", "--silent"]).args([
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]))
        .status()
        .map_err(|err| format!("Couldn't run winget to install the display driver: {err}"))?;
    let started = Instant::now();
    while started.elapsed() < INSTALL_WAIT {
        if installed() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if status.success() {
        Err("The display driver installed, but Windows hasn't started it yet. Restart the PC and try again".into())
    } else {
        Err(format!(
            "The display driver didn't install (winget code {}). Install \"Virtual Display Driver\" from github.com/VirtualDrivers/Virtual-Display-Driver",
            status.code().unwrap_or(-1)
        ))
    }
}

fn hidden(command: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
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
