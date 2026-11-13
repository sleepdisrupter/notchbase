//! An app's windows: listing them, capturing small pictures of them off the main thread, and raising, restoring or
//! closing one. The pictures are shared by everything that previews windows.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use egui::{Rect, pos2, vec2};
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, ns_string};

#[derive(Clone, Debug, PartialEq)]
pub struct Win {
    pub id: u32,
    pub title: String,
    /// Global top-left points.
    pub frame: Rect,
    pub on_screen: bool,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFUUIDCreateString(allocator: *const c_void, uuid: *const c_void) -> *const c_void;
}

const ALL: u32 = 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;
const INCLUDING_WINDOW: u32 = 1 << 3;
const IGNORE_FRAMING: u32 = 1 << 0;
const NOMINAL_RESOLUTION: u32 = 1 << 4;

/// Whether window pictures and titles can be read. Asks macOS once (it shows its own prompt) when `ask` is set.
pub fn can_capture(ask: bool) -> bool {
    static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if unsafe { CGPreflightScreenCaptureAccess() } {
        return true;
    }
    if ask && !ASKED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        unsafe { CGRequestScreenCaptureAccess() };
    }
    false
}

/// `pid`'s real windows, front to back: on this Space, on others, and minimized.
pub fn of_app(pid: i32) -> Vec<Win> {
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ALL | EXCLUDE_DESKTOP, 0) };
        if raw.is_null() {
            return Vec::new();
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let wins = list
            .iter()
            .filter_map(|entry| {
                let entry = dict(&entry)?;
                let num = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok();
                if num(ns_string!("kCGWindowOwnerPID"))?.intValue() != pid
                    || num(ns_string!("kCGWindowLayer"))?.intValue() != 0
                    || num(ns_string!("kCGWindowAlpha")).is_some_and(|a| a.doubleValue() < 0.05)
                {
                    return None;
                }
                let on_screen = num(ns_string!("kCGWindowIsOnscreen")).is_some_and(|n| n.boolValue());
                let title = entry
                    .objectForKey(ns_string!("kCGWindowName"))
                    .and_then(|t| t.downcast::<NSString>().ok())
                    .map(|t| t.to_string())
                    .unwrap_or_default();
                let bounds = entry.objectForKey(ns_string!("kCGWindowBounds"))?;
                let bounds = dict(&bounds)?;
                let f = |key: &NSString| {
                    bounds.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.doubleValue() as f32)
                };
                let frame = Rect::from_min_size(
                    pos2(f(ns_string!("X"))?, f(ns_string!("Y"))?),
                    vec2(f(ns_string!("Width"))?, f(ns_string!("Height"))?),
                );
                // Off-screen helpers apps keep around have no title; real windows elsewhere do.
                let real = frame.width() >= 120.0 && frame.height() >= 80.0 && (on_screen || !title.is_empty());
                let id = num(ns_string!("kCGWindowNumber"))?.intValue() as u32;
                real.then_some(Win { id, title, frame, on_screen })
            })
            .collect();
        unsafe { CFRelease(raw) };
        wins
    })
}

fn dict(object: &AnyObject) -> Option<&NSDictionary<NSString, AnyObject>> {
    let is: bool = unsafe { objc2::msg_send![object, isKindOfClass: objc2::class!(NSDictionary)] };
    is.then(|| unsafe { &*(object as *const AnyObject as *const NSDictionary<NSString, AnyObject>) })
}

/// Captures windows on a thread of its own and keeps the latest picture of each.
pub struct Thumbs {
    ask: Sender<(u32, u32)>,
    done: Receiver<(u32, egui::ColorImage)>,
    pub cache: HashMap<u32, (egui::TextureHandle, Instant)>,
    asked: HashMap<u32, Instant>,
}

impl Thumbs {
    pub fn new() -> Self {
        let (ask, requests) = channel::<(u32, u32)>();
        let (send, done) = channel();
        std::thread::Builder::new()
            .name("window pictures".into())
            .spawn(move || {
                while let Ok(first) = requests.recv() {
                    // Only the newest request for each window matters.
                    let mut wanted = HashMap::from([(first.0, first.1)]);
                    while let Ok((id, px)) = requests.try_recv() {
                        wanted.insert(id, px);
                    }
                    for (id, px) in wanted {
                        if let Some(image) = autoreleasepool(|_| capture(id, px))
                            && send.send((id, image)).is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .expect("a thread for window pictures");
        Self { ask, done, cache: HashMap::new(), asked: HashMap::new() }
    }

    /// Asks for a fresh picture of `id`, `px` pixels tall, at most every `every` seconds.
    pub fn want(&mut self, id: u32, px: u32, every: f32) {
        let now = Instant::now();
        if self.asked.get(&id).is_some_and(|at| now.duration_since(*at).as_secs_f32() < every) {
            return;
        }
        self.asked.insert(id, now);
        let _ = self.ask.send((id, px));
    }

    /// Turns finished pictures into textures. Returns true when one arrived.
    pub fn collect(&mut self, ctx: &egui::Context) -> bool {
        let mut any = false;
        while let Ok((id, image)) = self.done.try_recv() {
            let options = egui::TextureOptions {
                magnification: egui::TextureFilter::Linear,
                minification: egui::TextureFilter::Linear,
                wrap_mode: egui::TextureWrapMode::ClampToEdge,
                mipmap_mode: Some(egui::TextureFilter::Linear),
            };
            match self.cache.get_mut(&id) {
                Some((texture, at)) => {
                    texture.set(image, options);
                    *at = Instant::now();
                }
                None => {
                    let texture = ctx.load_texture(format!("window {id}"), image, options);
                    self.cache.insert(id, (texture, Instant::now()));
                }
            }
            any = true;
        }
        any
    }

    pub fn get(&self, id: u32) -> Option<&egui::TextureHandle> {
        self.cache.get(&id).map(|(t, _)| t)
    }

    /// Takes a picture that was captured elsewhere, so it shows straight away.
    pub fn put(&mut self, ctx: &egui::Context, id: u32, image: egui::ColorImage) {
        let options = egui::TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification: egui::TextureFilter::Linear,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
            mipmap_mode: Some(egui::TextureFilter::Linear),
        };
        let texture = ctx.load_texture(format!("window {id}"), image, options);
        self.cache.insert(id, (texture, Instant::now()));
        self.asked.insert(id, Instant::now());
    }

    /// Forgets windows that are gone.
    pub fn keep(&mut self, alive: impl Fn(u32) -> bool) {
        self.cache.retain(|id, _| alive(*id));
        self.asked.retain(|id, _| alive(*id));
    }
}

type CreateImage = unsafe extern "C" fn(CGRect, u32, u32, u32) -> *const CGImage;

/// `CGWindowListCreateImage`, looked up at run time since newer SDKs hide it.
fn create_image() -> Option<CreateImage> {
    static FOUND: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let f =
        *FOUND.get_or_init(|| unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"CGWindowListCreateImage".as_ptr()) } as usize);
    (f != 0).then(|| unsafe { std::mem::transmute::<usize, CreateImage>(f) })
}

fn capture(id: u32, px: u32) -> Option<egui::ColorImage> {
    let create = create_image()?;
    let null = CGRect::new(CGPoint::new(f64::INFINITY, f64::INFINITY), CGSize::new(0.0, 0.0));
    let raw = unsafe { create(null, INCLUDING_WINDOW, id, IGNORE_FRAMING | NOMINAL_RESOLUTION) };
    if raw.is_null() {
        return None;
    }
    let image: &CGImage = unsafe { &*raw };
    let result = scale(image, px as usize);
    unsafe { CFRelease(raw.cast()) };
    result
}

fn scale(image: &CGImage, height: usize) -> Option<egui::ColorImage> {
    const RGBA: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let (w, h) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    if w < 2 || h < 2 || height == 0 {
        return None;
    }
    let height = height.min(h);
    let width = ((w as f64 * height as f64 / h as f64).round() as usize).max(1);
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let mut rgba = vec![0u8; width * height * 4];
    let context =
        unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), width, height, 8, width * 4, Some(&space), RGBA) }?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as CGFloat, height as CGFloat));
    CGContext::draw_image(Some(&context), rect, Some(image));
    drop(context);
    Some(egui::ColorImage::from_rgba_premultiplied([width, height], &rgba))
}

// Acting on one window goes through Accessibility, matched to the window list by title and frame.

type AxElement = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AxElement;
    fn AXUIElementCopyAttributeValue(element: AxElement, attribute: *const c_void, value: *mut *const c_void) -> i32;
    fn AXUIElementSetAttributeValue(element: AxElement, attribute: *const c_void, value: *const c_void) -> i32;
    fn AXUIElementPerformAction(element: AxElement, action: *const c_void) -> i32;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> bool;
    fn _AXUIElementGetWindow(element: AxElement, id: *mut u32) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    static kCFBooleanFalse: *const c_void;
    static kCFBooleanTrue: *const c_void;
}

fn name(s: &NSString) -> *const c_void {
    s as *const NSString as *const c_void
}

fn copy(element: AxElement, attribute: &NSString) -> Option<*const c_void> {
    let mut out = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, name(attribute), &mut out) };
    (error == 0 && !out.is_null()).then_some(out)
}

#[repr(C)]
#[derive(Default)]
struct Xy {
    x: f64,
    y: f64,
}

/// Runs `f` on the Accessibility element for window `win` of `pid`, if it can be found.
fn with_window(pid: i32, win: &Win, f: impl FnOnce(AxElement)) -> bool {
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return false;
    }
    // A busy app answers late or not at all: never wait on it for long.
    unsafe { AXUIElementSetMessagingTimeout(app, 0.5) };
    let mut done = false;
    if let Some(list) = copy(app, ns_string!("AXWindows")) {
        let count = unsafe { CFArrayGetCount(list) };
        let mut best: Option<AxElement> = None;
        for i in 0..count {
            let element = unsafe { CFArrayGetValueAtIndex(list, i) };
            let mut id = 0u32;
            if unsafe { _AXUIElementGetWindow(element, &mut id) } == 0 && id == win.id {
                best = Some(element);
                break;
            }
            // Fall back to the title, then the position.
            let title = copy(element, ns_string!("AXTitle")).map(|t| {
                let s = unsafe { &*(t as *const NSString) }.to_string();
                unsafe { CFRelease(t) };
                s
            });
            let at = copy(element, ns_string!("AXPosition")).map(|v| {
                let mut p = Xy::default();
                unsafe { AXValueGetValue(v, 1, &mut p as *mut Xy as *mut c_void) };
                unsafe { CFRelease(v) };
                p
            });
            let same_title = !win.title.is_empty() && title.as_deref() == Some(win.title.as_str());
            let same_place = at.is_some_and(|p| {
                (p.x as f32 - win.frame.min.x).abs() < 2.0 && (p.y as f32 - win.frame.min.y).abs() < 2.0
            });
            if best.is_none() && (same_title || same_place) {
                best = Some(element);
            }
        }
        if let Some(element) = best {
            f(element);
            done = true;
        }
        unsafe { CFRelease(list) };
    }
    unsafe { CFRelease(app) };
    done
}

/// One window of one app, found once and kept while a minimize or restore works on it, so each step is a single
/// question to the app rather than a search through its windows. Every question waits half a second at most.
pub struct Handle {
    pid: i32,
    app: AxElement,
    window: AxElement,
}

// SAFETY: Accessibility elements are CoreFoundation objects, usable from any thread; each handle is used by one
// thread at a time.
unsafe impl Send for Handle {}

impl Handle {
    /// The window with this id, or nothing when the app doesn't list it (or doesn't answer).
    pub fn find(pid: i32, id: u32) -> Option<Self> {
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return None;
        }
        unsafe { AXUIElementSetMessagingTimeout(app, 0.5) };
        let mut window = None;
        if let Some(list) = copy(app, ns_string!("AXWindows")) {
            for i in 0..unsafe { CFArrayGetCount(list) } {
                let element = unsafe { CFArrayGetValueAtIndex(list, i) };
                let mut found = 0u32;
                if unsafe { _AXUIElementGetWindow(element, &mut found) } == 0 && found == id {
                    window = Some(unsafe { CFRetain(element) });
                    break;
                }
            }
            unsafe { CFRelease(list) };
        }
        match window {
            Some(window) => Some(Self { pid, app, window }),
            None => {
                unsafe { CFRelease(app) };
                None
            }
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CFRelease(self.window) };
        unsafe { CFRelease(self.app) };
    }
}

impl crate::flow::Ax for Handle {
    fn frame(&self) -> Option<Rect> {
        let read = |attribute: &NSString, kind: u32| {
            copy(self.window, attribute).map(|v| {
                let mut p = Xy::default();
                unsafe { AXValueGetValue(v, kind, &mut p as *mut Xy as *mut c_void) };
                unsafe { CFRelease(v) };
                p
            })
        };
        let (at, size) = (read(ns_string!("AXPosition"), 1)?, read(ns_string!("AXSize"), 2)?);
        Some(Rect::from_min_size(pos2(at.x as f32, at.y as f32), vec2(size.x as f32, size.y as f32)))
    }

    fn move_to(&self, to: egui::Pos2) {
        set_xy(self.window, ns_string!("AXPosition"), 1, f64::from(to.x), f64::from(to.y));
    }

    fn resize(&self, size: egui::Vec2) {
        set_xy(self.window, ns_string!("AXSize"), 2, f64::from(size.x), f64::from(size.y));
    }

    fn set_minimized(&self, minimized: bool) {
        let value = unsafe { if minimized { kCFBooleanTrue } else { kCFBooleanFalse } };
        unsafe { AXUIElementSetAttributeValue(self.window, name(ns_string!("AXMinimized")), value) };
    }

    fn minimized(&self) -> Option<bool> {
        copy(self.window, ns_string!("AXMinimized")).map(|v| {
            let yes = v == unsafe { kCFBooleanTrue };
            unsafe { CFRelease(v) };
            yes
        })
    }

    fn raise(&self) {
        unsafe {
            AXUIElementSetAttributeValue(self.window, name(ns_string!("AXMinimized")), kCFBooleanFalse);
            AXUIElementPerformAction(self.window, name(ns_string!("AXRaise")));
            AXUIElementSetAttributeValue(self.app, name(ns_string!("AXFrontmost")), kCFBooleanTrue);
        }
        crate::mac::activate(self.pid);
    }
}

fn set_xy(element: AxElement, attribute: &NSString, kind: u32, x: f64, y: f64) {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXValueCreate(kind: u32, value: *const c_void) -> *const c_void;
    }
    let xy = Xy { x, y };
    let value = unsafe { AXValueCreate(kind, &xy as *const Xy as *const c_void) };
    if !value.is_null() {
        unsafe { AXUIElementSetAttributeValue(element, name(attribute), value) };
        unsafe { CFRelease(value) };
    }
}

/// Brings one window to the front (restoring it if it's minimized) and its app with it.
pub fn raise(pid: i32, win: &Win) {
    with_window(pid, win, |w| unsafe {
        AXUIElementSetAttributeValue(w, name(ns_string!("AXMinimized")), kCFBooleanFalse);
        AXUIElementPerformAction(w, name(ns_string!("AXRaise")));
    });
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if !app.is_null() {
        unsafe { AXUIElementSetAttributeValue(app, name(ns_string!("AXFrontmost")), kCFBooleanTrue) };
        unsafe { CFRelease(app) };
    }
    crate::mac::activate(pid);
}

/// Presses one window's close button.
pub fn close(pid: i32, win: &Win) {
    with_window(pid, win, |w| {
        if let Some(button) = copy(w, ns_string!("AXCloseButton")) {
            unsafe { AXUIElementPerformAction(button, name(ns_string!("AXPress"))) };
            unsafe { CFRelease(button) };
        }
    });
}

/// Every app's ordinary windows, cheaply: (pid, window id, on screen). No titles, so it needs no permission.
pub fn every_window() -> Vec<(i32, u32, bool)> {
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ALL | EXCLUDE_DESKTOP, 0) };
        if raw.is_null() {
            return Vec::new();
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let wins = list
            .iter()
            .filter_map(|entry| {
                let entry = dict(&entry)?;
                let num = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok();
                if num(ns_string!("kCGWindowLayer"))?.intValue() != 0 {
                    return None;
                }
                let pid = num(ns_string!("kCGWindowOwnerPID"))?.intValue();
                let id = num(ns_string!("kCGWindowNumber"))?.intValue() as u32;
                let on_screen = num(ns_string!("kCGWindowIsOnscreen")).is_some_and(|n| n.boolValue());
                Some((pid, id, on_screen))
            })
            .collect();
        unsafe { CFRelease(raw) };
        wins
    })
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementSetMessagingTimeout(element: AxElement, seconds: f32) -> i32;
}

/// Every app's real windows for the switcher, front to back: (pid, window id, on screen). Small helper windows and
/// invisible ones are left out.
/// A smaller copy of a picture, `height` pixels high at most, averaged so it stays smooth.
pub fn smaller(image: &egui::ColorImage, height: usize) -> egui::ColorImage {
    let [w, h] = image.size;
    if h <= height || w == 0 || h == 0 {
        return image.clone();
    }
    let step = h.div_ceil(height);
    let (nw, nh) = ((w / step).max(1), (h / step).max(1));
    let mut out = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 4];
            for dy in 0..step {
                for dx in 0..step {
                    let c = image.pixels[(y * step + dy).min(h - 1) * w + (x * step + dx).min(w - 1)];
                    for (s, v) in sum.iter_mut().zip(c.to_array()) {
                        *s += u32::from(v);
                    }
                }
            }
            let n = (step * step) as u32;
            let [r, g, b, a] = sum.map(|s| (s / n) as u8);
            out.push(egui::Color32::from_rgba_premultiplied(r, g, b, a));
        }
    }
    egui::ColorImage::new([nw, nh], out)
}

/// Whether an app is full screen on display `display` (0 is the main display): an ordinary window on screen there
/// covering all of it, menu bar and all, as a full-screen app's Space has. Only the window list is read, so it's cheap
/// to ask often.
pub fn fullscreen_on(display: u32) -> bool {
    let display = if display == 0 { unsafe { CGMainDisplayID() } } else { display };
    // A full-screen app has a Space of its own. On a Mac with a notch its window starts below the notch band, so it
    // never covers the whole display and only the Space says it's full screen.
    if let Some(full) = space_is_fullscreen(display) {
        return full;
    }
    covered(display)
}

/// The type of the main display's current Space, from the window server: 4 is a full-screen app's Space.
fn space_is_fullscreen(display: u32) -> Option<bool> {
    type Connection = unsafe extern "C" fn() -> i32;
    type CopySpaces = unsafe extern "C" fn(i32) -> *const c_void;
    type DisplayUuid = unsafe extern "C" fn(u32) -> *const c_void;
    const FULLSCREEN_SPACE: i32 = 4;
    let find = |name: &std::ffi::CStr| {
        let f = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
        (!f.is_null()).then_some(f)
    };
    let connection: Connection = unsafe { std::mem::transmute(find(c"CGSMainConnectionID")?) };
    let copy: CopySpaces = unsafe { std::mem::transmute(find(c"CGSCopyManagedDisplaySpaces")?) };
    let main_uuid = find(c"CGDisplayCreateUUIDFromDisplayID").and_then(|f| {
        let uuid_of: DisplayUuid = unsafe { std::mem::transmute(f) };
        let uuid = unsafe { uuid_of(display) };
        if uuid.is_null() {
            return None;
        }
        let text = unsafe { CFUUIDCreateString(std::ptr::null(), uuid) };
        unsafe { CFRelease(uuid) };
        if text.is_null() {
            return None;
        }
        let name = unsafe { &*(text as *const NSString) }.to_string();
        unsafe { CFRelease(text) };
        Some(name)
    });
    autoreleasepool(|_| {
        let raw = unsafe { copy(connection()) };
        if raw.is_null() {
            return None;
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let displays: Vec<(String, Option<i32>)> = list
            .iter()
            .filter_map(|entry| {
                let entry = dict(&entry)?;
                let id = entry.objectForKey(ns_string!("Display Identifier"))?.downcast::<NSString>().ok()?.to_string();
                let current = entry.objectForKey(ns_string!("Current Space"))?;
                let kind = dict(&current)?.objectForKey(ns_string!("type"))?.downcast::<NSNumber>().ok()?;
                Some((id, Some(kind.intValue())))
            })
            .collect();
        unsafe { CFRelease(raw) };
        main_space_type(&displays, main_uuid.as_deref()).map(|kind| kind == FULLSCREEN_SPACE)
    })
}

/// Picks the main display's entry: the one named by its UUID, the shared "Main" entry when displays share Spaces, or
/// the only entry there is.
fn main_space_type(displays: &[(String, Option<i32>)], main_uuid: Option<&str>) -> Option<i32> {
    let by_uuid = main_uuid.and_then(|uuid| displays.iter().find(|(id, _)| id.eq_ignore_ascii_case(uuid)));
    let main = by_uuid
        .or_else(|| displays.iter().find(|(id, _)| id == "Main"))
        .or_else(|| (displays.len() == 1).then(|| &displays[0]));
    main.and_then(|(_, kind)| *kind)
}

/// The older test: a normal window covering the whole display.
fn covered(display: u32) -> bool {
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    let screen = unsafe { CGDisplayBounds(display) };
    if screen.size.width < 1.0 {
        return false;
    }
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
        if raw.is_null() {
            return false;
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let found = list.iter().any(|entry| {
            (|| {
                let entry = dict(&entry)?;
                let num = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok();
                if num(ns_string!("kCGWindowLayer"))?.intValue() != 0 {
                    return None;
                }
                if num(ns_string!("kCGWindowAlpha")).is_some_and(|a| a.doubleValue() <= 0.01) {
                    return None;
                }
                let bounds = entry.objectForKey(ns_string!("kCGWindowBounds"))?;
                let bounds = dict(&bounds)?;
                let f = |key: &NSString| bounds.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.doubleValue());
                let (x, y, w, h) =
                    (f(ns_string!("X"))?, f(ns_string!("Y"))?, f(ns_string!("Width"))?, f(ns_string!("Height"))?);
                let near = |a: f64, b: f64| (a - b).abs() < 1.0;
                Some(
                    near(x, screen.origin.x)
                        && near(y, screen.origin.y)
                        && near(w, screen.size.width)
                        && near(h, screen.size.height),
                )
            })()
            .unwrap_or(false)
        });
        unsafe { CFRelease(raw) };
        found
    })
}

pub fn switcher_windows() -> Vec<(i32, u32, bool)> {
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ALL | EXCLUDE_DESKTOP, 0) };
        if raw.is_null() {
            return Vec::new();
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let wins = list
            .iter()
            .filter_map(|entry| {
                let entry = dict(&entry)?;
                let num = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok();
                if num(ns_string!("kCGWindowLayer"))?.intValue() != 0 {
                    return None;
                }
                if num(ns_string!("kCGWindowAlpha")).is_some_and(|a| a.doubleValue() <= 0.01) {
                    return None;
                }
                let bounds = entry.objectForKey(ns_string!("kCGWindowBounds"))?;
                let bounds = dict(&bounds)?;
                let f = |key: &NSString| bounds.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.doubleValue());
                if f(ns_string!("Width"))? < 120.0 || f(ns_string!("Height"))? < 80.0 {
                    return None;
                }
                let pid = num(ns_string!("kCGWindowOwnerPID"))?.intValue();
                let id = num(ns_string!("kCGWindowNumber"))?.intValue() as u32;
                let on_screen = num(ns_string!("kCGWindowIsOnscreen")).is_some_and(|n| n.boolValue());
                Some((pid, id, on_screen))
            })
            .collect();
        unsafe { CFRelease(raw) };
        wins
    })
}

/// `pid`'s minimized windows, read through Accessibility (titles and all, with no screen recording needed).
pub fn minimized_of(pid: i32) -> Vec<Win> {
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return Vec::new();
    }
    // An app that hangs mustn't hold the watch up for long.
    unsafe { AXUIElementSetMessagingTimeout(app, 0.25) };
    let mut out = Vec::new();
    if let Some(list) = copy(app, ns_string!("AXWindows")) {
        let count = unsafe { CFArrayGetCount(list) };
        for i in 0..count {
            let element = unsafe { CFArrayGetValueAtIndex(list, i) };
            let minimized = copy(element, ns_string!("AXMinimized")).is_some_and(|v| {
                let yes = v == unsafe { kCFBooleanTrue };
                unsafe { CFRelease(v) };
                yes
            });
            let mut id = 0u32;
            if !minimized || unsafe { _AXUIElementGetWindow(element, &mut id) } != 0 || id == 0 {
                continue;
            }
            let title = copy(element, ns_string!("AXTitle"))
                .map(|t| {
                    let s = unsafe { &*(t as *const NSString) }.to_string();
                    unsafe { CFRelease(t) };
                    s
                })
                .unwrap_or_default();
            let point = |attribute: &NSString, kind: u32| {
                copy(element, attribute).map(|v| {
                    let mut p = Xy::default();
                    unsafe { AXValueGetValue(v, kind, &mut p as *mut Xy as *mut c_void) };
                    unsafe { CFRelease(v) };
                    p
                })
            };
            let at = point(ns_string!("AXPosition"), 1).unwrap_or_default();
            let size = point(ns_string!("AXSize"), 2).unwrap_or_default();
            let frame = Rect::from_min_size(pos2(at.x as f32, at.y as f32), vec2(size.x as f32, size.y as f32));
            out.push(Win { id, title, frame, on_screen: false });
        }
        unsafe { CFRelease(list) };
    }
    unsafe { CFRelease(app) };
    out
}

fn win_with_id(id: u32) -> Win {
    Win { id, title: String::new(), frame: Rect::NOTHING, on_screen: false }
}

/// Where window `id` of `pid` is, from Accessibility: global top-left points.
pub fn frame_of(pid: i32, id: u32) -> Option<Rect> {
    let mut frame = None;
    with_window(pid, &win_with_id(id), |w| {
        let read = |attribute: &NSString, kind: u32| {
            copy(w, attribute).map(|v| {
                let mut p = Xy::default();
                unsafe { AXValueGetValue(v, kind, &mut p as *mut Xy as *mut c_void) };
                unsafe { CFRelease(v) };
                p
            })
        };
        if let (Some(at), Some(size)) = (read(ns_string!("AXPosition"), 1), read(ns_string!("AXSize"), 2)) {
            frame = Some(Rect::from_min_size(pos2(at.x as f32, at.y as f32), vec2(size.x as f32, size.y as f32)));
        }
    });
    frame
}

/// Moves window `id` of `pid` so its top left corner is at `to`, global top-left points.
pub fn move_to(pid: i32, id: u32, to: egui::Pos2) -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXValueCreate(kind: u32, value: *const c_void) -> *const c_void;
    }
    with_window(pid, &win_with_id(id), |w| {
        let at = Xy { x: f64::from(to.x), y: f64::from(to.y) };
        let value = unsafe { AXValueCreate(1, &at as *const Xy as *const c_void) };
        if !value.is_null() {
            unsafe { AXUIElementSetAttributeValue(w, name(ns_string!("AXPosition")), value) };
            unsafe { CFRelease(value) };
        }
    })
}

/// Minimizes window `id` of `pid`, or brings it back.
pub fn set_minimized(pid: i32, id: u32, minimized: bool) -> bool {
    with_window(pid, &win_with_id(id), |w| unsafe {
        let value = if minimized { kCFBooleanTrue } else { kCFBooleanFalse };
        AXUIElementSetAttributeValue(w, name(ns_string!("AXMinimized")), value);
    })
}

/// What `rect` of the screen (global top-left points) would show without window `skip` and without this dock's own
/// windows: a stand-in to cover that part of the screen with while macOS animates `skip` there.
pub fn screen_without(rect: Rect, skip: u32) -> Option<egui::ColorImage> {
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    type FromArray = unsafe extern "C" fn(CGRect, *const c_void, u32) -> *const CGImage;
    unsafe extern "C" {
        fn CFArrayCreate(
            alloc: *const c_void,
            values: *const *const c_void,
            count: isize,
            calls: *const c_void,
        ) -> *const c_void;
    }
    let f = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"CGWindowListCreateImageFromArray".as_ptr()) };
    if f.is_null() {
        return None;
    }
    let from_array = unsafe { std::mem::transmute::<*mut c_void, FromArray>(f) };
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY, 0) };
        if raw.is_null() {
            return None;
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let me = std::process::id() as i32;
        let ids: Vec<*const c_void> = list
            .iter()
            .filter_map(|entry| {
                let entry = dict(&entry)?;
                let num = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok();
                let id = num(ns_string!("kCGWindowNumber"))?.intValue() as u32;
                let pid = num(ns_string!("kCGWindowOwnerPID"))?.intValue();
                (id != skip && pid != me).then_some(id as usize as *const c_void)
            })
            .collect();
        unsafe { CFRelease(raw) };
        let array = unsafe { CFArrayCreate(std::ptr::null(), ids.as_ptr(), ids.len() as isize, std::ptr::null()) };
        if array.is_null() {
            return None;
        }
        let area = CGRect::new(
            CGPoint::new(rect.min.x.into(), rect.min.y.into()),
            CGSize::new(rect.width().into(), rect.height().into()),
        );
        let image = unsafe { from_array(area, array, 0) };
        unsafe { CFRelease(array) };
        if image.is_null() {
            return None;
        }
        let picture = scale(unsafe { &*image }, usize::MAX);
        unsafe { CFRelease(image.cast()) };
        picture
    })
}

/// The main screen, `size` points from its top left, as it shows now, `px` pixels tall.
pub fn screen_picture(size: egui::Vec2, px: u32) -> Option<egui::ColorImage> {
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    autoreleasepool(|_| {
        let create = create_image()?;
        let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(size.x.into(), size.y.into()));
        let raw = unsafe { create(rect, ON_SCREEN_ONLY, 0, NOMINAL_RESOLUTION) };
        if raw.is_null() {
            return None;
        }
        let image: &CGImage = unsafe { &*raw };
        let result = scale(image, px as usize);
        unsafe { CFRelease(raw.cast()) };
        result
    })
}

/// A picture of window `id`, `px` pixels tall, taken now. Works for a minimized window too, from its last look.
pub fn picture(id: u32, px: u32) -> Option<egui::ColorImage> {
    autoreleasepool(|_| capture(id, px))
}

/// The frame of the frontmost ordinary window under `at` (global top-left points), from the window list alone: cheap
/// enough to look at on every click.
pub fn top_window_at(at: egui::Pos2) -> Option<Rect> {
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
        if raw.is_null() {
            return None;
        }
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let me = std::process::id() as i32;
        let found = list.iter().find_map(|entry| {
            let entry = dict(&entry)?;
            let num = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok();
            let bounds = entry.objectForKey(ns_string!("kCGWindowBounds"))?;
            let bounds = dict(&bounds)?;
            let f =
                |key: &NSString| bounds.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.doubleValue() as f32);
            let frame = Rect::from_min_size(
                pos2(f(ns_string!("X"))?, f(ns_string!("Y"))?),
                vec2(f(ns_string!("Width"))?, f(ns_string!("Height"))?),
            );
            if !frame.contains(at) {
                return None;
            }
            // The dock's own windows (its full-screen overlay among them) take no clicks there: look past them.
            if num(ns_string!("kCGWindowOwnerPID"))?.intValue() == me {
                return None;
            }
            // Windows above the ordinary ones (the notch's see-through panel, the menu bar) are looked past: the
            // Accessibility check that follows sees what's really under the point.
            (num(ns_string!("kCGWindowLayer"))?.intValue() == 0).then_some(frame)
        });
        unsafe { CFRelease(raw) };
        found
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_main_display_space() {
        let one = [("37D8832A-2D66".to_owned(), Some(4))];
        assert_eq!(main_space_type(&one, None), Some(4), "the only display");
        assert_eq!(main_space_type(&one, Some("37d8832a-2d66")), Some(4), "UUIDs match whatever their case");
        let two = [("AAAA".to_owned(), Some(0)), ("BBBB".to_owned(), Some(4))];
        assert_eq!(
            main_space_type(&two, Some("AAAA")),
            Some(0),
            "a full-screen app on the other display doesn't count"
        );
        assert_eq!(main_space_type(&two, Some("BBBB")), Some(4));
        assert_eq!(main_space_type(&two, None), None, "unsure, so the window test decides");
        let shared = [("Main".to_owned(), Some(4))];
        assert_eq!(main_space_type(&shared, Some("CCCC")), Some(4), "displays sharing Spaces");
    }
}
