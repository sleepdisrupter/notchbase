//! Staying awake on Windows through SetThreadExecutionState.

use windows_sys::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
};

pub(super) struct Assertion;

impl Assertion {
    pub(super) fn hold() -> Option<Self> {
        let held = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) };
        (held != 0).then_some(Self)
    }

    pub(super) fn renew(&mut self) {}
}

pub(super) fn forget_lid() {}

impl Drop for Assertion {
    fn drop(&mut self) {
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}
