//! COM and OLE for the threads that need them, and shell item ID lists for paths.

use std::path::Path;

use ::windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize};
use ::windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use ::windows::Win32::UI::Shell::Common::ITEMIDLIST;
use ::windows::Win32::UI::Shell::{ILFree, SHParseDisplayName};
use ::windows::core::HSTRING;

use super::shell_path::shell_parsable;

struct ApartmentForThisThread {
    initialized_here: bool,
}

impl Drop for ApartmentForThisThread {
    fn drop(&mut self) {
        if self.initialized_here {
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static APARTMENT: ApartmentForThisThread = ApartmentForThisThread {
        initialized_here: unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
        }
        .is_ok(),
    };
}

/// Joins a single-threaded apartment once per thread, and leaves it when the thread ends.
pub(crate) fn ensure_com_on_this_thread() {
    APARTMENT.with(|_| {});
}

/// Runs work with OLE initialised, and uninitialises only if this call did it.
pub(crate) fn with_ole<T>(work: impl FnOnce() -> T) -> T {
    let initialized_here = unsafe { OleInitialize(None) }.is_ok();
    let result = work();
    if initialized_here {
        unsafe { OleUninitialize() };
    }
    result
}

pub(crate) struct ItemIdList(*mut ITEMIDLIST);

impl ItemIdList {
    pub(crate) fn for_path(path: &Path) -> Result<Self, String> {
        let mut list = std::ptr::null_mut();
        let display_name = HSTRING::from(shell_parsable(path).as_path());
        unsafe { SHParseDisplayName(&display_name, None, &mut list, 0, None) }
            .map_err(|e| format!("Couldn't drag {}: {}", file_name(path), e.message()))?;
        Ok(Self(list))
    }

    pub(crate) fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

impl Drop for ItemIdList {
    fn drop(&mut self) {
        unsafe { ILFree(Some(self.0)) };
    }
}
