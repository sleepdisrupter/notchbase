//! Joins the COM multithreaded apartment for as long as the guard lives.

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub struct Apartment {
    joined: bool,
}

impl Apartment {
    pub fn join() -> Self {
        Self { joined: unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok() }
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // Only undo our own join. If CoInitializeEx failed we must not uninitialize.
        if self.joined {
            unsafe { CoUninitialize() };
        }
    }
}
