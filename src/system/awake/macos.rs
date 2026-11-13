//! Staying awake on macOS: power assertions against idle sleep, and telling the Mac not to sleep when its lid closes.

use std::ffi::{CStr, c_char, c_void};

use objc2::rc::Retained;
use objc2_foundation::NSString;

mod floor;

pub(super) fn request_lid_floor(disabled: bool, report: impl FnOnce(bool) + Send + 'static) {
    floor::request(disabled, report);
}

const KEEP_AWAKE: [&str; 2] = ["PreventUserIdleDisplaySleep", "PreventUserIdleSystemSleep"];
const REASON: &str = "notchbase is keeping this Mac awake";
const LEVEL_ON: u32 = 255;
const POWER_ROOT: &CStr = c"IOPMrootDomain";
const SET_CLAMSHELL_SLEEP_STATE: u32 = 12;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(kind: *const c_void, level: u32, reason: *const c_void, id: *mut u32) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
    fn IOServiceMatching(name: *const c_char) -> *const c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *const c_void) -> u32;
    fn IOServiceOpen(service: u32, task: u32, kind: u32, connection: *mut u32) -> i32;
    fn IOServiceClose(connection: u32) -> i32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOConnectCallScalarMethod(
        connection: u32,
        selector: u32,
        input: *const u64,
        inputs: u32,
        output: *mut u64,
        outputs: *mut u32,
    ) -> i32;
}

unsafe extern "C" {
    static mach_task_self_: u32;
}

pub(super) struct Assertion {
    _held: Vec<Held>,
    lid: Option<Lid>,
}

impl Assertion {
    pub(super) fn hold() -> Option<Self> {
        let held: Vec<Held> = KEEP_AWAKE.into_iter().filter_map(Held::take).collect();
        (held.len() == KEEP_AWAKE.len()).then(|| Self { _held: held, lid: hold_lid() })
    }

    pub(super) fn renew(&mut self) {
        match &self.lid {
            Some(lid) => drop(lid.sleeps_when_closed(false)),
            None => self.lid = hold_lid(),
        }
    }
}

struct Held(u32);

impl Held {
    fn take(kind: &str) -> Option<Self> {
        let (kind, reason) = (NSString::from_str(kind), NSString::from_str(REASON));
        let mut id = 0u32;
        let status = unsafe {
            IOPMAssertionCreateWithName(
                Retained::as_ptr(&kind).cast(),
                LEVEL_ON,
                Retained::as_ptr(&reason).cast(),
                &mut id,
            )
        };
        (status == 0).then_some(Self(id))
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        unsafe { IOPMAssertionRelease(self.0) };
    }
}

pub(super) fn forget_lid() {
    drop(hold_lid());
}

fn hold_lid() -> Option<Lid> {
    (!cfg!(test)).then(Lid::hold).flatten()
}

struct Lid(u32);

impl Lid {
    fn hold() -> Option<Self> {
        let mut connection = 0u32;
        let opened = unsafe {
            let root = IOServiceGetMatchingService(0, IOServiceMatching(POWER_ROOT.as_ptr()));
            if root == 0 {
                return None;
            }
            let status = IOServiceOpen(root, mach_task_self_, 0, &mut connection);
            IOObjectRelease(root);
            status
        };
        if opened != 0 {
            return None;
        }
        let lid = Self(connection);
        lid.sleeps_when_closed(false).then_some(lid)
    }

    fn sleeps_when_closed(&self, sleeps: bool) -> bool {
        let disabled = u64::from(!sleeps);
        let status = unsafe {
            IOConnectCallScalarMethod(
                self.0,
                SET_CLAMSHELL_SLEEP_STATE,
                &disabled,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        status == 0
    }
}

impl Drop for Lid {
    fn drop(&mut self) {
        self.sleeps_when_closed(true);
        unsafe { IOServiceClose(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "tells this Mac for a moment not to sleep when its lid closes"]
    fn lid_can_be_told_not_to_sleep() {
        assert!(Lid::hold().is_some());
    }
}
