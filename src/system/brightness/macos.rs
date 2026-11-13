//! Brightness on macOS through the private DisplayServices framework, loaded at runtime, for the built-in display.

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const FRAMEWORK: &CStr = c"/System/Library/PrivateFrameworks/DisplayServices.framework/Versions/A/DisplayServices";
const RECHECK_DISPLAY: Duration = Duration::from_millis(500);
const MAX_DISPLAYS: usize = 16;

type DisplayId = u32;
type CanChange = unsafe extern "C" fn(display: DisplayId) -> bool;
type Get = unsafe extern "C" fn(display: DisplayId, brightness: *mut f32) -> i32;
type Set = unsafe extern "C" fn(display: DisplayId, brightness: f32) -> i32;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, displays: *mut DisplayId, count: *mut u32) -> i32;
    fn CGDisplayIsBuiltin(display: DisplayId) -> u32;
}

#[derive(Clone, Copy)]
struct Services {
    can_change: CanChange,
    get: Get,
    set: Set,
}

pub struct Display {
    id: Option<DisplayId>,
    checked_at: Option<Instant>,
}

impl Display {
    pub fn open() -> Self {
        Self { id: None, checked_at: None }
    }

    pub fn is_adjustable(&mut self) -> bool {
        self.follow().is_some()
    }

    pub fn read(&mut self) -> Option<f32> {
        let (services, id) = self.follow()?;
        level_of(services, id)
    }

    pub fn set(&mut self, level: f32) {
        if let Some((services, id)) = self.follow() {
            unsafe { (services.set)(id, level.clamp(0.0, 1.0)) };
        }
    }

    fn follow(&mut self) -> Option<(Services, DisplayId)> {
        let services = services()?;
        let now = Instant::now();
        if self.checked_at.is_none_or(|at| now.duration_since(at) >= RECHECK_DISPLAY) {
            self.checked_at = Some(now);
            self.id = adjustable(services);
        }
        Some((services, self.id?))
    }
}

fn adjustable(services: Services) -> Option<DisplayId> {
    let mut ids = [0; MAX_DISPLAYS];
    let mut count = 0u32;
    if unsafe { CGGetActiveDisplayList(MAX_DISPLAYS as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return None;
    }
    let shown = &ids[..(count as usize).min(MAX_DISPLAYS)];
    let can = |id: &&DisplayId| unsafe { (services.can_change)(**id) } && level_of(services, **id).is_some();
    let builtin = shown.iter().filter(|id| unsafe { CGDisplayIsBuiltin(**id) } != 0).find(can);
    builtin.or_else(|| shown.iter().find(can)).copied()
}

fn level_of(services: Services, id: DisplayId) -> Option<f32> {
    let mut level = 0.0f32;
    let read = unsafe { (services.get)(id, &mut level) } == 0;
    (read && level.is_finite()).then(|| level.clamp(0.0, 1.0))
}

fn services() -> Option<Services> {
    static SERVICES: OnceLock<Option<Services>> = OnceLock::new();
    *SERVICES.get_or_init(|| {
        let library = NonNull::new(unsafe { libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_NOW) })?;
        let symbol = |name: &CStr| NonNull::new(unsafe { libc::dlsym(library.as_ptr(), name.as_ptr()) });
        let can_change = symbol(c"DisplayServicesCanChangeBrightness")?.as_ptr();
        let get = symbol(c"DisplayServicesGetBrightness")?.as_ptr();
        let set = symbol(c"DisplayServicesSetBrightness")?.as_ptr();
        Some(unsafe {
            Services {
                can_change: std::mem::transmute::<*mut c_void, CanChange>(can_change),
                get: std::mem::transmute::<*mut c_void, Get>(get),
                set: std::mem::transmute::<*mut c_void, Set>(set),
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads this Mac's real display brightness and writes the same level back"]
    fn brightness_can_be_read_and_set() {
        let mut display = Display::open();
        if !display.is_adjustable() {
            return;
        }
        let level = display.read().expect("an adjustable display has a level");
        assert!((0.0..=1.0).contains(&level));
        display.set(level);
        let after = display.read().expect("still readable");
        assert!((after - level).abs() < 0.01, "{level} became {after}");
    }
}
