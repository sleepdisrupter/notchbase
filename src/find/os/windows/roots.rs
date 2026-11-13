//! The fixed drives to index, the boot drive first, and the Start menu folders that hold the apps.

use std::path::{Path, PathBuf};

use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
use windows::core::HSTRING;

use crate::find::os::{Root, root};

const FIXED_DRIVE: u32 = 3;
const LETTERS: u8 = 26;
const FIRST_LETTER: u8 = b'A';
const SYSTEM_DRIVE: &str = "SystemDrive";
const USUAL_SYSTEM_DRIVE: &str = "C:";
const WINDOWS_FOLDER: &str = "windir";
const SEALED_ON_EVERY_DRIVE: [&str; 2] = ["$Recycle.Bin", "System Volume Information"];
const SERIAL_BYTES: usize = 4;
const START_MENU_HOLDERS: [&str; 2] = ["ProgramData", "APPDATA"];
const START_MENU_PROGRAMS: &str = "Microsoft\\Windows\\Start Menu\\Programs";

pub fn roots(volumes: bool) -> Vec<Root> {
    let boot = boot_drive();
    let drives = fixed_drives().into_iter().map(|drive| (same_drive(&drive, &boot), drive));
    let kept = drives.filter(|(boots, _)| volumes || *boots);
    kept.map(|(boots, drive)| Root {
        volume: serial(&drive),
        path: drive,
        devices: [0, 0],
        flags: if boots { root::BOOT } else { 0 },
    })
    .collect()
}

pub fn sealed() -> Vec<PathBuf> {
    let windows = std::env::var_os(WINDOWS_FOLDER).map(PathBuf::from);
    let per_drive = fixed_drives().into_iter().flat_map(|drive| SEALED_ON_EVERY_DRIVE.map(|name| drive.join(name)));
    windows.into_iter().chain(per_drive).collect()
}

pub fn unsealed() -> Vec<PathBuf> {
    Vec::new()
}

pub fn guarded(_home: &Path) -> Vec<PathBuf> {
    Vec::new()
}

pub fn launchers(_home: Option<&Path>) -> Vec<PathBuf> {
    let holders = START_MENU_HOLDERS.into_iter().filter_map(std::env::var_os).filter(|holder| !holder.is_empty());
    holders.map(|holder| PathBuf::from(holder).join(START_MENU_PROGRAMS)).chain(crate::find::apps::own_menu()).collect()
}

pub fn full_access() -> bool {
    true
}

fn fixed_drives() -> Vec<PathBuf> {
    let present = unsafe { GetLogicalDrives() };
    let letters = (0..LETTERS).filter(|index| present & (1 << index) != 0);
    let drives = letters.map(|index| PathBuf::from(format!("{}:\\", char::from(FIRST_LETTER + index))));
    drives.filter(|drive| unsafe { GetDriveTypeW(&HSTRING::from(drive.as_os_str())) } == FIXED_DRIVE).collect()
}

fn boot_drive() -> PathBuf {
    let drive = std::env::var(SYSTEM_DRIVE).unwrap_or_else(|_| USUAL_SYSTEM_DRIVE.to_owned());
    PathBuf::from(format!("{}\\", drive.trim_end_matches('\\')))
}

fn same_drive(left: &Path, right: &Path) -> bool {
    left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
}

fn serial(drive: &Path) -> [u8; 16] {
    let mut number = 0u32;
    let name = HSTRING::from(drive.as_os_str());
    let read = unsafe { GetVolumeInformationW(&name, None, Some(&raw mut number), None, None, None) };
    let mut volume = [0; 16];
    if read.is_ok() {
        volume[..SERIAL_BYTES].copy_from_slice(&number.to_le_bytes());
    }
    volume
}
