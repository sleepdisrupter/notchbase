//! Parents come from one Toolhelp snapshot and stop at the shell. Window owners are processes with an app window.

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use crate::platform::{shell_process, windowed_processes};

pub(super) fn owner(me: u32) -> Option<u32> {
    let parents = parents()?;
    let windowed = windowed_processes();
    let shell = shell_process();
    let parent = |pid: u32| parents.get(&pid).copied().filter(|parent| Some(*parent) != shell);
    super::nearest(me, parent, |pid| windowed.contains(&pid))
}

fn parents() -> Option<HashMap<u32, u32>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut parents = HashMap::new();
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    Some(parents)
}
