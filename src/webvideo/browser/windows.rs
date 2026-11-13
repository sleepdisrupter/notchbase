//! The browser in front on Windows. Programs registered under `Clients\StartMenuInternet` count as browsers,
//! and UI Automation reads what the window shows.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, IUIAutomationElement, TreeScope_Descendants, UIA_ComboBoxControlTypeId,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_ValueValuePropertyId,
};
use windows::core::BSTR;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::platform::Apartment;

use super::Browser;

/// Where browsers register themselves, in both registry views.
const REGISTERED: [&str; 2] =
    [r"SOFTWARE\Clients\StartMenuInternet", r"SOFTWARE\WOW6432Node\Clients\StartMenuInternet"];
const OPENED_WITH: &str = r"shell\open\command";
/// The environment variable for the folder Firefox forks keep their profiles in.
const PROFILES: &str = "APPDATA";
const NAME_ROOM: usize = 256;
const PATH_ROOM: usize = 1024;
/// How long UI Automation may wait on the browser for one request.
const PATIENCE_MS: u32 = 1500;
// Limits on the walk through the window, so a busy browser can't hold it up for long.
const LONGEST: Duration = Duration::from_millis(1500);
const MOST_ELEMENTS: usize = 400;
const DEEPEST: usize = 12;
const WIDEST: usize = 60;

pub(super) fn frontmost() -> Option<Browser> {
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    let program = program_of(pid)?;
    let name = program.file_name()?.to_string_lossy().to_ascii_lowercase();
    if !registered_browsers().contains(&name) {
        return None;
    }
    let profiles = std::env::var_os(PROFILES).map(PathBuf::from);
    let identity = Path::new(&name).file_stem()?.to_string_lossy().into_owned();
    Some(Browser { pid, window: window as usize as u64, cookies: super::cookies_for(&identity, profiles.as_deref()) })
}

pub(super) fn addresses(browser: &Browser, wanted: &dyn Fn(&str) -> bool) -> Vec<String> {
    let _apartment = Apartment::join();
    read(browser, wanted).unwrap_or_default()
}

/// Chromium and Gecko both give the address as the value of the window's first document. Failing that, the edit
/// fields outside any document are read.
fn read(browser: &Browser, wanted: &dyn Fn(&str) -> bool) -> windows::core::Result<Vec<String>> {
    let automation: IUIAutomation2 = unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }?;
    unsafe {
        let _ = automation.SetConnectionTimeout(PATIENCE_MS);
        let _ = automation.SetTransactionTimeout(PATIENCE_MS);
    }
    let window = unsafe { automation.ElementFromHandle(HWND(browser.window as usize as *mut _)) }?;
    let is_page = unsafe {
        automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_DocumentControlTypeId.0))
    }?;
    let page = unsafe { window.FindFirst(TreeScope_Descendants, &is_page) }.ok();
    let mut found: Vec<String> = page.as_ref().and_then(value).into_iter().collect();
    if found.iter().any(|address| wanted(address)) {
        return Ok(found);
    }
    let walker = unsafe { automation.ControlViewWalker() }?;
    let started = Instant::now();
    let mut queue = VecDeque::from([(window, 0usize)]);
    let mut visited = 0;
    while let Some((element, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MOST_ELEMENTS || started.elapsed() > LONGEST {
            break;
        }
        let kind = unsafe { element.CurrentControlType() }.unwrap_or_default();
        // The page was read above, and nothing inside it is worth the walk.
        if kind == UIA_DocumentControlTypeId {
            continue;
        }
        if kind == UIA_EditControlTypeId || kind == UIA_ComboBoxControlTypeId {
            found.extend(value(&element));
        }
        if depth >= DEEPEST {
            continue;
        }
        let mut child = unsafe { walker.GetFirstChildElement(&element) }.ok();
        for _ in 0..WIDEST {
            let Some(current) = child else { break };
            child = unsafe { walker.GetNextSiblingElement(&current) }.ok();
            queue.push_back((current, depth + 1));
        }
    }
    Ok(found)
}

fn value(element: &IUIAutomationElement) -> Option<String> {
    let value = unsafe { element.GetCurrentPropertyValue(UIA_ValueValuePropertyId) }.ok()?;
    let text = BSTR::try_from(&value).ok()?.to_string();
    (!text.is_empty()).then_some(text)
}

fn program_of(pid: u32) -> Option<PathBuf> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = [0u16; PATH_ROOM];
    let mut length = PATH_ROOM as u32;
    let read = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    (read != 0).then(|| PathBuf::from(String::from_utf16_lossy(&path[..(length as usize).min(PATH_ROOM)])))
}

/// Program names of the browsers registered for this user and for the computer.
fn registered_browsers() -> Vec<String> {
    let mut names = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for path in REGISTERED {
            let Some(key) = Key::open(root, path) else { continue };
            for browser in key.children() {
                let command = key.text(&format!("{browser}\\{OPENED_WITH}"));
                names.extend(command.as_deref().and_then(super::program_name));
            }
        }
    }
    names
}

/// An open registry key, closed when dropped.
struct Key(HKEY);

impl Key {
    fn open(root: HKEY, path: &str) -> Option<Self> {
        let path = wide(path);
        let mut key: HKEY = null_mut();
        let status = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ, &mut key) };
        (status == ERROR_SUCCESS).then_some(Self(key))
    }

    fn children(&self) -> Vec<String> {
        let mut names = Vec::new();
        for index in 0.. {
            let mut name = [0u16; NAME_ROOM];
            let mut length = NAME_ROOM as u32;
            let status = unsafe {
                RegEnumKeyExW(self.0, index, name.as_mut_ptr(), &mut length, null(), null_mut(), null_mut(), null_mut())
            };
            if status != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&name[..(length as usize).min(NAME_ROOM)]));
        }
        names
    }

    fn text(&self, child: &str) -> Option<String> {
        let child = wide(child);
        let mut text = [0u16; PATH_ROOM];
        let mut size = size_of_val(&text) as u32;
        let status = unsafe {
            RegGetValueW(self.0, child.as_ptr(), null(), RRF_RT_REG_SZ, null_mut(), text.as_mut_ptr().cast(), &mut size)
        };
        let length = (size as usize / size_of::<u16>()).min(PATH_ROOM);
        (status == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&text[..length]).trim_end_matches('\0').to_owned())
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
