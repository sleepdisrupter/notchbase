//! The Mac's displays: a virtual one made with `CGVirtualDisplay` (the private CoreGraphics
//! classes that DeskPad, BetterDisplay and Sidecar-style tools use, there since macOS 11), and
//! what a real one is like, to lend it.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_foundation::{NSArray, NSString};

use crate::display::Panel;
use crate::layout::Rect;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(most: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGBeginDisplayConfiguration(config: *mut *mut c_void) -> i32;
    fn CGConfigureDisplayOrigin(config: *mut c_void, display: u32, x: i32, y: i32) -> i32;
    fn CGCompleteDisplayConfiguration(config: *mut c_void, option: u32) -> i32;
    fn CGCancelDisplayConfiguration(config: *mut c_void) -> i32;
    fn CGDisplayCopyDisplayMode(display: u32) -> *mut c_void;
    fn CGDisplayModeGetPixelWidth(mode: *mut c_void) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: *mut c_void) -> usize;
    fn CGDisplayModeGetRefreshRate(mode: *mut c_void) -> f64;
    fn CGDisplayModeRelease(mode: *mut c_void);
}

unsafe extern "C" {
    static _dispatch_main_q: c_void;
}

/// Changes last for this login, like moving a display in System Settings without saving.
const FOR_SESSION: u32 = 1;
/// How long a new virtual display may take to show up among the displays.
const APPEAR_WAIT: Duration = Duration::from_secs(3);
/// Pixels per inch a virtual display claims: about a desktop monitor's, twice that for Retina.
const PPI: f64 = 110.0;

fn class(name: &std::ffi::CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

/// Whether this Mac has virtual displays.
pub fn available() -> bool {
    class(c"CGVirtualDisplay").is_some() && class(c"CGVirtualDisplayDescriptor").is_some()
}

/// The displays macOS has on now.
fn active() -> Vec<u32> {
    let mut ids = [0u32; 32];
    let mut count = 0u32;
    // SAFETY: the buffer holds 32 displays and CoreGraphics writes at most that many.
    if unsafe { CGGetActiveDisplayList(32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    ids[..(count as usize).min(32)].to_vec()
}

fn bounds(id: u32) -> Rect {
    // SAFETY: plain query; an unknown display gives an empty rectangle.
    let b = unsafe { CGDisplayBounds(id) };
    Rect::new(b.origin.x, b.origin.y, b.size.width, b.size.height)
}

/// The real display at `rect` (in points), as it's set up now.
pub fn panel(rect: Rect) -> Option<Panel> {
    let close = |a: Rect| (a.x - rect.x).abs() < 1.0 && (a.y - rect.y).abs() < 1.0 && (a.w - rect.w).abs() < 1.0;
    let id = active().into_iter().find(|id| close(bounds(*id)))?;
    // SAFETY: the mode is copied, read and released here.
    unsafe {
        let mode = CGDisplayCopyDisplayMode(id);
        if mode.is_null() {
            return None;
        }
        let pixels = [CGDisplayModeGetPixelWidth(mode) as u32, CGDisplayModeGetPixelHeight(mode) as u32];
        let hz = CGDisplayModeGetRefreshRate(mode);
        CGDisplayModeRelease(mode);
        // Built-in screens answer 0.
        let hz = if hz >= 1.0 { hz.round() as u32 } else { 60 };
        let scale = if rect.w > 0.0 { (f64::from(pixels[0]) / rect.w) as f32 } else { 1.0 };
        (pixels[0] > 0).then_some(Panel { pixels, hz, scale })
    }
}

/// A virtual display. Dropping it takes it away, and macOS moves its windows to the others.
pub struct Virtual {
    /// Kept for as long as the display should be there.
    _display: Retained<AnyObject>,
    id: u32,
}

// SAFETY: the display object is only kept, and released, never messaged from two threads at once.
unsafe impl Send for Virtual {}

impl Virtual {
    /// A display `pixels` big, named `name`. On a Retina-style display (`hidpi`), its size in
    /// points is half that.
    pub fn new(name: &str, pixels: [u32; 2], hz: u32, hidpi: bool) -> Result<Self, String> {
        let missing = || "This Mac can't make virtual displays".to_owned();
        let descriptor_class = class(c"CGVirtualDisplayDescriptor").ok_or_else(missing)?;
        let display_class = class(c"CGVirtualDisplay").ok_or_else(missing)?;
        let settings_class = class(c"CGVirtualDisplaySettings").ok_or_else(missing)?;
        let mode_class = class(c"CGVirtualDisplayMode").ok_or_else(missing)?;
        let ppi = if hidpi { PPI * 2.0 } else { PPI };
        let millimeters = pixels.map(|side| f64::from(side) / ppi * 25.4);
        let points = if hidpi { pixels.map(|side| side / 2) } else { pixels };
        // SAFETY: the private classes' documented-by-use properties, set with the types they
        // were declared with (unsigned ints, a CGSize, an NSString and a dispatch queue).
        unsafe {
            let descriptor: Retained<AnyObject> = msg_send![descriptor_class, new];
            let title = NSString::from_str(name);
            let _: () = msg_send![&*descriptor, setName: &*title];
            let _: () = msg_send![&*descriptor, setMaxPixelsWide: pixels[0]];
            let _: () = msg_send![&*descriptor, setMaxPixelsHigh: pixels[1]];
            let _: () = msg_send![&*descriptor, setSizeInMillimeters: CGSize::new(millimeters[0], millimeters[1])];
            let _: () = msg_send![&*descriptor, setVendorID: 0x524Fu32];
            let _: () = msg_send![&*descriptor, setProductID: 0x414Du32];
            let _: () = msg_send![&*descriptor, setSerialNum: serial(name)];
            let queue = (&raw const _dispatch_main_q).cast::<AnyObject>();
            let _: () = msg_send![&*descriptor, setQueue: queue];
            let allocated: Allocated<AnyObject> = msg_send![display_class, alloc];
            let display: Option<Retained<AnyObject>> = msg_send![allocated, initWithDescriptor: &*descriptor];
            let display = display.ok_or("macOS didn't make the virtual display")?;
            let allocated: Allocated<AnyObject> = msg_send![mode_class, alloc];
            let mode: Retained<AnyObject> =
                msg_send![allocated, initWithWidth: points[0], height: points[1], refreshRate: f64::from(hz)];
            let modes = NSArray::from_retained_slice(&[mode]);
            let settings: Retained<AnyObject> = msg_send![settings_class, new];
            let _: () = msg_send![&*settings, setHiDPI: u32::from(hidpi)];
            let _: () = msg_send![&*settings, setModes: &*modes];
            let applied: bool = msg_send![&*display, applySettings: &*settings];
            if !applied {
                return Err(format!("macOS didn't take {}x{} pixels for the virtual display", pixels[0], pixels[1]));
            }
            let id: u32 = msg_send![&*display, displayID];
            let made = Self { _display: display, id };
            let started = Instant::now();
            while !active().contains(&id) {
                if started.elapsed() > APPEAR_WAIT {
                    return Err("The virtual display didn't show up".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(made)
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    /// Where it is, in points.
    pub fn rect(&self) -> Option<Rect> {
        active().contains(&self.id).then(|| bounds(self.id))
    }

    /// Moves its top left to `at`, in points.
    pub fn place(&self, at: [i32; 2]) {
        let mut config = std::ptr::null_mut();
        // SAFETY: a configuration begun, changed, and completed or cancelled here.
        unsafe {
            if CGBeginDisplayConfiguration(&mut config) != 0 || config.is_null() {
                return;
            }
            if CGConfigureDisplayOrigin(config, self.id, at[0], at[1]) != 0 {
                CGCancelDisplayConfiguration(config);
                return;
            }
            CGCompleteDisplayConfiguration(config, FOR_SESSION);
        }
    }
}

/// The same serial number for the same lending computer, so macOS remembers where its
/// display goes.
fn serial(name: &str) -> u32 {
    name.bytes().fold(2_166_136_261u32, |hash, byte| (hash ^ u32::from(byte)).wrapping_mul(16_777_619))
}
