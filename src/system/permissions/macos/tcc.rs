//! Reads a TCC permission without prompting, through the private TCC framework loaded at runtime.

use std::ffi::c_void;
use std::ptr;
use std::sync::OnceLock;

use libloading::Library;
use objc2::rc::Retained;
use objc2_foundation::NSString;

use super::super::Standing;

const FRAMEWORK: &str = "/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC";
const PREFLIGHT: &[u8] = b"TCCAccessPreflight\0";
const GRANTED: i32 = 0;
const DENIED: i32 = 1;

type Preflight = unsafe extern "C" fn(service: *const c_void, options: *const c_void) -> i32;

pub fn preflight(service: &str) -> Option<Standing> {
    let preflight = preflight_fn()?;
    let service = NSString::from_str(service);
    Some(match unsafe { preflight(Retained::as_ptr(&service).cast(), ptr::null()) } {
        GRANTED => Standing::Granted,
        DENIED => Standing::Denied,
        _ => Standing::Undecided,
    })
}

fn preflight_fn() -> Option<Preflight> {
    static LOADED: OnceLock<Option<(Library, Preflight)>> = OnceLock::new();
    let loaded = LOADED.get_or_init(|| {
        let library = unsafe { Library::new(FRAMEWORK) }.ok()?;
        let preflight = *unsafe { library.get::<Preflight>(PREFLIGHT) }.ok()?;
        Some((library, preflight))
    });
    loaded.as_ref().map(|(_, preflight)| *preflight)
}
