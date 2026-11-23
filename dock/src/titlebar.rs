//! Windows that fill the screen stop above the dock in one step, as they do with the real Dock.
//!
//! With the system Dock hidden, macOS zooms a window to the whole screen; correcting it afterwards shows two resizes.
//! So a double-click on a standard title bar is caught before the app sees it, and the window is sized once, to the
//! area above the dock (or back to where it was). Everything else that fills a window (the green button, Window > Fill,
//! tiling) is corrected the moment the app reports the resize, rather than on a poll.
//!
//! Needs Accessibility, which `avoid_windows` already asks for. The event tap runs on a thread of its own, so a busy
//! frame in the dock never delays a click anywhere; the window watch runs on the main run loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use objc2_foundation::{NSString, ns_string};

type AxElement = *const c_void;
type AxObserver = *const c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Xy {
    x: f64,
    y: f64,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> AxElement;
    fn AXUIElementCreateApplication(pid: i32) -> AxElement;
    fn AXUIElementCopyElementAtPosition(element: AxElement, x: f32, y: f32, out: *mut AxElement) -> i32;
    fn AXUIElementCopyAttributeValue(element: AxElement, attribute: *const c_void, value: *mut *const c_void) -> i32;
    fn AXUIElementSetAttributeValue(element: AxElement, attribute: *const c_void, value: *const c_void) -> i32;
    fn AXUIElementGetPid(element: AxElement, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AxElement, seconds: f32) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> *const c_void;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> bool;
    fn AXObserverCreate(
        pid: i32,
        callback: extern "C" fn(AxObserver, AxElement, *const c_void, *mut c_void),
        out: *mut AxObserver,
    ) -> i32;
    fn AXObserverAddNotification(
        observer: AxObserver,
        element: AxElement,
        notification: *const c_void,
        refcon: *mut c_void,
    ) -> i32;
    fn AXObserverGetRunLoopSource(observer: AxObserver) -> *const c_void;
    fn _AXUIElementGetWindow(element: AxElement, id: *mut u32) -> i32;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events: u64,
        callback: extern "C" fn(*const c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user: *mut c_void,
    ) -> *const c_void;
    fn CGEventTapEnable(tap: *const c_void, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventGetLocation(event: *mut c_void) -> Xy;
    fn CGEventGetFlags(event: *mut c_void) -> u64;
    fn CGEventKeyboardGetUnicodeString(event: *mut c_void, max: u64, actual: *mut u64, text: *mut u16);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(boolean: *const c_void) -> bool;
    fn CFMachPortCreateRunLoopSource(allocator: *const c_void, port: *const c_void, order: isize) -> *const c_void;
    fn CFRunLoopGetMain() -> *const c_void;
    fn CFRunLoopGetCurrent() -> *const c_void;
    fn CFRunLoopRun();
    fn CFRunLoopAddSource(run_loop: *const c_void, source: *const c_void, mode: *const c_void);
    fn CFRunLoopRemoveSource(run_loop: *const c_void, source: *const c_void, mode: *const c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

const SESSION_TAP: u32 = 1;
const HEAD: u32 = 0;
const ACTIVE: u32 = 0;
const MOUSE_DOWN: u32 = 1;
const MOUSE_UP: u32 = 2;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER: u32 = 0xFFFF_FFFF;
const KEY_DOWN: u32 = 10;
const FLAGS_CHANGED: u32 = 12;
const CLICK_STATE: u32 = 1;
const KEYCODE: u32 = 9;
/// kCGKeyboardEventAutorepeat: non-zero on a held key's repeats.
const AUTOREPEAT: u32 = 8;
const KEY_M: i64 = 46;
const COMMAND: u64 = 0x10_0000;
const OTHER_MODIFIERS: u64 = 0x2_0000 | 0x4_0000 | 0x8_0000;
/// Where the traffic lights sit in a window's top left corner, generously.
const TRAFFIC_LIGHTS: (f32, f32) = (96.0, 56.0);
const POINT: u32 = 1;
const SIZE: u32 = 2;
/// How far down from a window's top a double-click counts as the title bar: a title bar with a toolbar under it.
const TITLE_BAR: f64 = 52.0;

/// The area windows fill: the screen under the menu bar, down to just above the dock. Global top-left points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    /// The screen's very top and bottom: a window filled by macOS reaches the bottom.
    pub screen_top: f64,
    pub screen_bottom: f64,
}

impl Area {
    /// How much of a window at `pos` of `size` is on this area's screen, in square points.
    fn overlap(&self, pos: Xy, size: Xy) -> f64 {
        let w = (pos.x + size.x).min(self.right) - pos.x.max(self.left);
        let h = (pos.y + size.y).min(self.screen_bottom) - pos.y.max(self.screen_top);
        w.max(0.0) * h.max(0.0)
    }
}

/// The area a window fills: on the screen most of it is on, else the one under its middle. Global top-left points.
fn area_for(areas: &[Area], pos: Xy, size: Xy) -> Option<Area> {
    let best = areas.iter().max_by(|a, b| a.overlap(pos, size).total_cmp(&b.overlap(pos, size)))?;
    if best.overlap(pos, size) > 0.0 {
        return Some(*best);
    }
    let middle = Xy { x: pos.x + size.x * 0.5, y: pos.y + size.y * 0.5 };
    areas
        .iter()
        .copied()
        .find(|a| (a.left..a.right).contains(&middle.x) && (a.screen_top..a.screen_bottom).contains(&middle.y))
}

/// The area windows fill on each screen, shared with the tap's thread. Empty leaves windows alone.
static AREA: Mutex<Vec<Area>> = Mutex::new(Vec::new());
/// The event tap, once made.
static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static STARTED: AtomicBool = AtomicBool::new(false);
/// The mouse-up of a double-click that was taken over, so the app never sees half of it.
static SWALLOW_UP: AtomicBool = AtomicBool::new(false);
/// Where windows were before they were filled, to put them back on the next double-click.
/// A window's position and size, by its app and window number.
type Frames = HashMap<(i32, u32), (Xy, Xy)>;
static BEFORE: Mutex<Option<Frames>> = Mutex::new(None);
/// Whether minimizing is caught, so the dock can play its own animation. Off while the dock is away.
static CATCH_MINIMIZE: AtomicBool = AtomicBool::new(false);
/// While the dock moves a window itself, the window-change watch leaves windows alone.
static HANDS_OFF: AtomicBool = AtomicBool::new(false);
/// Windows asked to minimize, (pid, window number), for the main thread to take.
static MINIMIZE: Mutex<Vec<(i32, u32)>> = Mutex::new(Vec::new());
type Wake = Box<dyn Fn() + Send + Sync>;
static WAKE: std::sync::OnceLock<Wake> = std::sync::OnceLock::new();

/// Catches the yellow button, Cmd+M and a title-bar double-click set to minimize, and hands the window to the main
/// thread (`take_minimizes`) instead of letting macOS minimize it, so the dock can draw the move itself.
pub fn catch_minimize(on: bool) {
    CATCH_MINIMIZE.store(on, Ordering::Relaxed);
}

/// While the dock is minimizing or restoring a window, nothing else here touches windows: apps answer slowly while
/// they animate, and the moves the dock makes itself aren't for fitting.
pub fn hands_off(on: bool) {
    HANDS_OFF.store(on, Ordering::Relaxed);
}

/// Called (from the tap's thread) whenever a window is handed over.
pub fn on_minimize(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Box::new(wake));
}

pub fn take_minimizes() -> Vec<(i32, u32)> {
    std::mem::take(&mut *MINIMIZE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn hand_over(window: AxElement, pid: i32) -> bool {
    let mut id = 0u32;
    if unsafe { _AXUIElementGetWindow(window, &mut id) } != 0 || id == 0 {
        return false;
    }
    MINIMIZE.lock().unwrap_or_else(|e| e.into_inner()).push((pid, id));
    if let Some(wake) = WAKE.get() {
        wake();
    }
    true
}

/// A window that minimizes the ordinary way: a standard window, not full screen, with a working minimize button.
fn minimizable(window: AxElement) -> bool {
    if text(window, ns_string!("AXSubrole")) != "AXStandardWindow"
        || copy_bool(window, ns_string!("AXFullScreen")) == Some(true)
        || copy_bool(window, ns_string!("AXMinimized")) == Some(true)
    {
        return false;
    }
    let Some(button) = copy(window, ns_string!("AXMinimizeButton")) else { return false };
    let enabled = copy_bool(button, ns_string!("AXEnabled")) != Some(false);
    unsafe { CFRelease(button) };
    enabled
}

/// A click on a window's yellow button: hands the window over and returns true.
fn minimize_button_click(at: Xy) -> bool {
    let point = egui::pos2(at.x as f32, at.y as f32);
    // Only a click in a window's top left corner is looked at closely, so other clicks never wait on an app.
    let Some(frame) = crate::windows::top_window_at(point) else { return false };
    if point.x - frame.min.x > TRAFFIC_LIGHTS.0 || point.y - frame.min.y > TRAFFIC_LIGHTS.1 {
        return false;
    }
    let system = unsafe { AXUIElementCreateSystemWide() };
    unsafe { AXUIElementSetMessagingTimeout(system, 0.08) };
    let mut hit: AxElement = std::ptr::null();
    let found = unsafe { AXUIElementCopyElementAtPosition(system, point.x, point.y, &mut hit) } == 0;
    unsafe { CFRelease(system) };
    if !found || hit.is_null() {
        return false;
    }
    let mut done = false;
    if text(hit, ns_string!("AXSubrole")) == "AXMinimizeButton"
        && let Some(window) = copy(hit, ns_string!("AXWindow"))
    {
        let mut pid = 0;
        unsafe { AXUIElementGetPid(hit, &mut pid) };
        done = pid != std::process::id() as i32 && minimizable(window) && hand_over(window, pid);
        unsafe { CFRelease(window) };
    }
    unsafe { CFRelease(hit) };
    done
}

/// The front app's focused window, if it minimizes the ordinary way: (pid, window number).
pub fn focused_minimizable() -> Option<(i32, u32)> {
    let system = unsafe { AXUIElementCreateSystemWide() };
    unsafe { AXUIElementSetMessagingTimeout(system, 0.2) };
    let app = copy(system, ns_string!("AXFocusedApplication"));
    unsafe { CFRelease(system) };
    let app = app?;
    let mut pid = 0;
    unsafe { AXUIElementGetPid(app, &mut pid) };
    let window = copy(app, ns_string!("AXFocusedWindow"));
    unsafe { CFRelease(app) };
    let window = window?;
    let mut id = 0u32;
    let ok = minimizable(window) && unsafe { _AXUIElementGetWindow(window, &mut id) } == 0 && id != 0;
    unsafe { CFRelease(window) };
    ok.then_some((pid, id))
}

/// Cmd+M: hands the front app's focused window over and returns true.
fn command_m(event: *mut c_void) -> bool {
    if unsafe { CGEventGetIntegerValueField(event, KEYCODE) } != KEY_M {
        return false;
    }
    // A held Cmd+M repeats: only the first press minimizes (the window is still up for a moment after it).
    if unsafe { CGEventGetIntegerValueField(event, AUTOREPEAT) } != 0 {
        return true;
    }
    let flags = unsafe { CGEventGetFlags(event) };
    if flags & COMMAND == 0 || flags & OTHER_MODIFIERS != 0 {
        return false;
    }
    let system = unsafe { AXUIElementCreateSystemWide() };
    unsafe { AXUIElementSetMessagingTimeout(system, 0.08) };
    let app = copy(system, ns_string!("AXFocusedApplication"));
    unsafe { CFRelease(system) };
    let Some(app) = app else { return false };
    let mut pid = 0;
    unsafe { AXUIElementGetPid(app, &mut pid) };
    let window = copy(app, ns_string!("AXFocusedWindow"));
    unsafe { CFRelease(app) };
    let Some(window) = window else { return false };
    let done = pid != std::process::id() as i32 && minimizable(window) && hand_over(window, pid);
    unsafe { CFRelease(window) };
    done
}

fn areas() -> Vec<Area> {
    AREA.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The front app being watched for windows resizing: its pid, observer and run loop source. Main thread only.
type Watching = Option<(i32, AxObserver, *const c_void)>;

thread_local! {
    static WATCHING: RefCell<Watching> = const { RefCell::new(None) };
}

/// Starts catching title-bar double-clicks. Returns false when Accessibility isn't allowed yet; call again later.
pub fn start() -> bool {
    if STARTED.load(Ordering::Relaxed) {
        return true;
    }
    if !unsafe { AXIsProcessTrusted() } {
        return false;
    }
    STARTED.store(true, Ordering::Relaxed);
    let spawned = std::thread::Builder::new().name("title bar clicks".into()).spawn(|| {
        let mask = (1u64 << MOUSE_DOWN) | (1u64 << MOUSE_UP) | (1u64 << KEY_DOWN) | (1u64 << FLAGS_CHANGED);
        let tap = unsafe { CGEventTapCreate(SESSION_TAP, HEAD, ACTIVE, mask, on_event, std::ptr::null_mut()) };
        if tap.is_null() {
            STARTED.store(false, Ordering::Relaxed);
            return;
        }
        TAP.store(tap.cast_mut(), Ordering::Relaxed);
        unsafe {
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            CFRelease(source);
            CGEventTapEnable(tap, true);
            CFRunLoopRun();
        }
    });
    if spawned.is_err() {
        STARTED.store(false, Ordering::Relaxed);
    }
    STARTED.load(Ordering::Relaxed)
}

/// Updates the area windows fill on each screen; none (the dock away, or avoiding off) leaves windows alone.
pub fn set_areas(areas: Vec<Area>) {
    *AREA.lock().unwrap_or_else(|e| e.into_inner()) = areas;
}

/// Watches `pid`'s windows for resizes, so ones that fill the screen are corrected at once.
pub fn watch(pid: Option<i32>) {
    WATCHING.with(|w| {
        let mut watching = w.borrow_mut();
        if watching.map(|w| w.0) == pid {
            return;
        }
        if let Some((_, observer, source)) = watching.take() {
            unsafe {
                CFRunLoopRemoveSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
                CFRelease(observer);
            }
        }
        let Some(pid) = pid.filter(|_| unsafe { AXIsProcessTrusted() }) else { return };
        let mut observer: AxObserver = std::ptr::null();
        if unsafe { AXObserverCreate(pid, on_window_change, &mut observer) } != 0 || observer.is_null() {
            return;
        }
        let app = unsafe { AXUIElementCreateApplication(pid) };
        for name in [ns_string!("AXWindowResized"), ns_string!("AXWindowMoved")] {
            unsafe { AXObserverAddNotification(observer, app, attr(name), std::ptr::null_mut()) };
        }
        unsafe { CFRelease(app) };
        let source = unsafe { AXObserverGetRunLoopSource(observer) };
        unsafe { CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes) };
        *watching = Some((pid, observer, source));
    });
}

extern "C" fn on_window_change(_: AxObserver, window: AxElement, _: *const c_void, _: *mut c_void) {
    let areas = areas();
    if areas.is_empty() || HANDS_OFF.load(Ordering::Relaxed) {
        return;
    }
    // Someone dragging a window's edge is left alone.
    if crate::mac::mouse_buttons_held() {
        return;
    }
    fit_if_filled(window, &areas);
}

/// Shortens a window that runs to the bottom of the screen, under the dock, so it ends above it.
fn fit_if_filled(window: AxElement, areas: &[Area]) {
    if copy_bool(window, ns_string!("AXFullScreen")) == Some(true) {
        return;
    }
    let (Some(at), Some(size)) =
        (value(window, ns_string!("AXPosition"), POINT), value(window, ns_string!("AXSize"), SIZE))
    else {
        return;
    };
    let Some(area) = area_for(areas, at, size) else { return };
    let middle = at.x + size.x * 0.5;
    let bottom = at.y + size.y;
    if !(area.left..area.right).contains(&middle)
        || bottom < area.screen_bottom - 2.0
        || bottom <= area.bottom + 1.0
        || at.y >= area.bottom - 120.0
    {
        return;
    }
    set_value(window, ns_string!("AXSize"), SIZE, Xy { x: size.x, y: area.bottom - at.y });
}

extern "C" fn on_event(_: *const c_void, kind: u32, event: *mut c_void, _: *mut c_void) -> *mut c_void {
    if kind == TAP_DISABLED_BY_TIMEOUT || kind == TAP_DISABLED_BY_USER {
        // macOS turns a slow tap off; turn it back on.
        let tap = TAP.load(Ordering::Relaxed);
        if !tap.is_null() {
            unsafe { CGEventTapEnable(tap, true) };
        }
        return event;
    }
    if kind == MOUSE_UP {
        let swallow = SWALLOW_UP.swap(false, Ordering::Relaxed) | crate::switcher::on_mouse_up();
        return if swallow { std::ptr::null_mut() } else { event };
    }
    if kind == FLAGS_CHANGED {
        crate::switcher::on_flags(unsafe { CGEventGetFlags(event) });
        return event;
    }
    let catching = CATCH_MINIMIZE.load(Ordering::Relaxed);
    if kind == KEY_DOWN {
        let code = unsafe { CGEventGetIntegerValueField(event, KEYCODE) };
        let flags = unsafe { CGEventGetFlags(event) };
        if crate::switcher::on_key(code, flags) {
            return std::ptr::null_mut();
        }
        if crate::stack::catching_keys() && crate::stack::on_key(code, flags, &typed(event)) {
            return std::ptr::null_mut();
        }
        return if catching && command_m(event) { std::ptr::null_mut() } else { event };
    }
    if kind == MOUSE_DOWN {
        let at = unsafe { CGEventGetLocation(event) };
        if crate::switcher::on_mouse_down(egui::pos2(at.x as f32, at.y as f32)) {
            return std::ptr::null_mut();
        }
    }
    if kind != MOUSE_DOWN {
        return event;
    }
    let clicks = unsafe { CGEventGetIntegerValueField(event, CLICK_STATE) };
    if clicks == 1 && catching && minimize_button_click(unsafe { CGEventGetLocation(event) }) {
        SWALLOW_UP.store(true, Ordering::Relaxed);
        return std::ptr::null_mut();
    }
    if clicks != 2 {
        return event;
    }
    let areas = areas();
    if areas.is_empty() {
        return event;
    }
    let at = unsafe { CGEventGetLocation(event) };
    if double_click_title_bar(at, &areas) {
        SWALLOW_UP.store(true, Ordering::Relaxed);
        std::ptr::null_mut()
    } else {
        event
    }
}

/// What double-clicking a title bar does, from System Settings > Desktop & Dock.
fn action() -> &'static str {
    let defaults = objc2_foundation::NSUserDefaults::standardUserDefaults();
    let action = defaults.stringForKey(ns_string!("AppleActionOnDoubleClick")).map(|s| s.to_string());
    match action.as_deref() {
        Some("Minimize") => "minimize",
        Some("None") => "none",
        // Older systems kept only the minimize switch.
        None if defaults.boolForKey(ns_string!("AppleMiniaturizeOnDoubleClick")) => "minimize",
        // Zoom (the default) and Fill both fill the screen for nearly every app.
        _ => "fill",
    }
}

/// If `at` is a double-click on a standard window's title bar, fills that window above the dock (or puts it back)
/// and returns true. Anything it isn't sure about goes to the app as usual.
fn double_click_title_bar(at: Xy, areas: &[Area]) -> bool {
    if action() == "minimize" && CATCH_MINIMIZE.load(Ordering::Relaxed) {
        let Some((window, pid)) = title_bar_at(at) else { return false };
        let done = minimizable(window) && hand_over(window, pid);
        unsafe { CFRelease(window) };
        return done;
    }
    if action() != "fill" {
        return false;
    }
    let Some((window, pid)) = title_bar_at(at) else { return false };
    let done = toggle_fill(window, pid, at, areas);
    unsafe { CFRelease(window) };
    done
}

/// The standard window whose bare title bar is at `at`, and its app. The caller releases the window.
fn title_bar_at(at: Xy) -> Option<(AxElement, i32)> {
    let system = unsafe { AXUIElementCreateSystemWide() };
    unsafe { AXUIElementSetMessagingTimeout(system, 0.08) };
    let mut hit: AxElement = std::ptr::null();
    let found = unsafe { AXUIElementCopyElementAtPosition(system, at.x as f32, at.y as f32, &mut hit) } == 0;
    unsafe { CFRelease(system) };
    if !found || hit.is_null() {
        return None;
    }
    let result = (|| {
        let mut pid = 0;
        unsafe { AXUIElementGetPid(hit, &mut pid) };
        if pid == std::process::id() as i32 {
            return None;
        }
        // Only the bare title bar: the window itself, its title, or an empty toolbar strip. Buttons, tabs, fields
        // and anything else in it keep their own double-click.
        let role = text(hit, ns_string!("AXRole"));
        let window = if role == "AXWindow" { Some(retain(hit)) } else { copy(hit, ns_string!("AXWindow")) }?;
        let plain = match role.as_str() {
            "AXWindow" | "AXToolbar" => true,
            "AXStaticText" => copy(hit, ns_string!("AXParent")).is_some_and(|parent| {
                let is_window = text(parent, ns_string!("AXRole")) == "AXWindow";
                unsafe { CFRelease(parent) };
                is_window
            }),
            _ => false,
        };
        let top = value(window, ns_string!("AXPosition"), POINT).map_or(f64::MAX, |p| at.y - p.y);
        let ok = plain
            && (0.0..=TITLE_BAR).contains(&top)
            && text(window, ns_string!("AXSubrole")) == "AXStandardWindow"
            && copy_bool(window, ns_string!("AXFullScreen")) != Some(true);
        if ok {
            Some((window, pid))
        } else {
            unsafe { CFRelease(window) };
            None
        }
    })();
    unsafe { CFRelease(hit) };
    result
}

/// What's at a point on screen and whether a double-click there would be taken over, for checking apps by hand.
pub fn probe(x: f64, y: f64) -> String {
    let at = Xy { x, y };
    let system = unsafe { AXUIElementCreateSystemWide() };
    let mut hit: AxElement = std::ptr::null();
    let found = unsafe { AXUIElementCopyElementAtPosition(system, x as f32, y as f32, &mut hit) } == 0;
    unsafe { CFRelease(system) };
    if !found || hit.is_null() {
        return "nothing there".into();
    }
    let role = text(hit, ns_string!("AXRole"));
    let sub = text(hit, ns_string!("AXSubrole"));
    unsafe { CFRelease(hit) };
    let taken = title_bar_at(at).map(|(w, _)| unsafe { CFRelease(w) }).is_some();
    format!("{role} {sub}: {}", if taken { "filled in one step" } else { "left to the app" })
}

/// Fills the window to the area in one step, or puts a filled one back where it was.
fn toggle_fill(window: AxElement, pid: i32, at: Xy, areas: &[Area]) -> bool {
    let (Some(pos), Some(size)) =
        (value(window, ns_string!("AXPosition"), POINT), value(window, ns_string!("AXSize"), SIZE))
    else {
        return false;
    };
    if at.y - pos.y > TITLE_BAR || at.y < pos.y {
        return false;
    }
    // The screen the window is on, not the main one: its menu bar, and the dock only if the dock is there.
    let Some(area) = area_for(areas, pos, size) else { return false };
    let mut id = 0u32;
    unsafe { _AXUIElementGetWindow(window, &mut id) };
    let key = (pid, id);
    let fill_at = Xy { x: area.left, y: area.top };
    let fill_size = Xy { x: area.right - area.left, y: area.bottom - area.top };
    let near = |a: Xy, b: Xy| (a.x - b.x).abs() < 3.0 && (a.y - b.y).abs() < 3.0;
    if near(pos, fill_at) && near(size, fill_size) {
        let before = BEFORE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().remove(&key);
        let Some((pos, size)) = before else { return true };
        set_value(window, ns_string!("AXSize"), SIZE, size);
        set_value(window, ns_string!("AXPosition"), POINT, pos);
        set_value(window, ns_string!("AXSize"), SIZE, size);
        return true;
    }
    BEFORE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().insert(key, (pos, size));
    // Moving first lets the size fit; sizing again covers apps that clamp the size to the old position.
    set_value(window, ns_string!("AXPosition"), POINT, fill_at);
    set_value(window, ns_string!("AXSize"), SIZE, fill_size);
    if value(window, ns_string!("AXSize"), SIZE).is_some_and(|now| !near(now, fill_size)) {
        set_value(window, ns_string!("AXSize"), SIZE, fill_size);
    }
    true
}

fn attr(name: &NSString) -> *const c_void {
    name as *const NSString as *const c_void
}

fn copy(element: AxElement, name: &NSString) -> Option<AxElement> {
    let mut out = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, attr(name), &mut out) };
    (error == 0 && !out.is_null()).then_some(out)
}

fn retain(element: AxElement) -> AxElement {
    unsafe extern "C" {
        fn CFRetain(cf: *const c_void) -> *const c_void;
    }
    unsafe { CFRetain(element) }
}

fn text(element: AxElement, name: &NSString) -> String {
    let Some(v) = copy(element, name) else { return String::new() };
    let s = if unsafe { CFGetTypeID(v) == CFStringGetTypeID() } {
        unsafe { &*(v as *const NSString) }.to_string()
    } else {
        String::new()
    };
    unsafe { CFRelease(v) };
    s
}

fn copy_bool(element: AxElement, name: &NSString) -> Option<bool> {
    let v = copy(element, name)?;
    let b = (unsafe { CFGetTypeID(v) } == unsafe { CFBooleanGetTypeID() }).then(|| unsafe { CFBooleanGetValue(v) });
    unsafe { CFRelease(v) };
    b
}

fn value(element: AxElement, name: &NSString, kind: u32) -> Option<Xy> {
    let v = copy(element, name)?;
    let mut out = Xy::default();
    let ok = unsafe { AXValueGetValue(v, kind, &mut out as *mut Xy as *mut c_void) };
    unsafe { CFRelease(v) };
    ok.then_some(out)
}

fn set_value(element: AxElement, name: &NSString, kind: u32, to: Xy) {
    let v = unsafe { AXValueCreate(kind, &to as *const Xy as *const c_void) };
    if v.is_null() {
        return;
    }
    unsafe { AXUIElementSetAttributeValue(element, attr(name), v) };
    unsafe { CFRelease(v) };
}

/// Whether this app may use Accessibility.
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// The system Dock's icon list in screen points (x, y from the top left, width, height), as Accessibility sees it.
pub fn dock_list_frame(dock_pid: i32) -> Option<[f64; 4]> {
    unsafe extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    }
    let app = unsafe { AXUIElementCreateApplication(dock_pid) };
    if app.is_null() {
        return None;
    }
    let children = copy(app, ns_string!("AXChildren"));
    unsafe { CFRelease(app) };
    let children = children?;
    let mut found = None;
    for i in 0..unsafe { CFArrayGetCount(children) } {
        let child = unsafe { CFArrayGetValueAtIndex(children, i) };
        if text(child, ns_string!("AXRole")) == "AXList"
            && let (Some(at), Some(size)) =
                (value(child, ns_string!("AXPosition"), POINT), value(child, ns_string!("AXSize"), SIZE))
        {
            found = Some([at.x, at.y, size.x, size.y]);
            break;
        }
    }
    unsafe { CFRelease(children) };
    found
}

/// The system Dock's items: (subrole, title, [x, y, w, h]) in global top-left points.
pub fn dock_items(dock_pid: i32) -> Vec<(String, String, [f64; 4])> {
    unsafe extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    }
    let mut out = Vec::new();
    let app = unsafe { AXUIElementCreateApplication(dock_pid) };
    if app.is_null() {
        return out;
    }
    let children = copy(app, ns_string!("AXChildren"));
    unsafe { CFRelease(app) };
    let Some(children) = children else { return out };
    for i in 0..unsafe { CFArrayGetCount(children) } {
        let child = unsafe { CFArrayGetValueAtIndex(children, i) };
        if text(child, ns_string!("AXRole")) != "AXList" {
            continue;
        }
        if let Some(items) = copy(child, ns_string!("AXChildren")) {
            for n in 0..unsafe { CFArrayGetCount(items) } {
                let item = unsafe { CFArrayGetValueAtIndex(items, n) };
                if let (Some(at), Some(size)) =
                    (value(item, ns_string!("AXPosition"), POINT), value(item, ns_string!("AXSize"), SIZE))
                {
                    out.push((
                        text(item, ns_string!("AXSubrole")),
                        text(item, ns_string!("AXTitle")),
                        [at.x, at.y, size.x, size.y],
                    ));
                }
            }
            unsafe { CFRelease(items) };
        }
    }
    unsafe { CFRelease(children) };
    out
}

/// The badges the system Dock shows on its apps (unread counts and the like), even while it's hidden: each app's
/// bundle path, its title, and the badge's text. Needs Accessibility.
pub fn dock_badges(dock_pid: i32) -> Vec<crate::badges::Badge> {
    unsafe extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFURLCopyFileSystemPath(url: *const c_void, style: isize) -> *const c_void;
        fn CFURLGetTypeID() -> usize;
    }
    const POSIX_PATHS: isize = 0;
    let path_of = |item: AxElement| {
        let url = copy(item, ns_string!("AXURL"))?;
        let path = (unsafe { CFGetTypeID(url) } == unsafe { CFURLGetTypeID() })
            .then(|| unsafe { CFURLCopyFileSystemPath(url, POSIX_PATHS) })
            .filter(|path| !path.is_null());
        unsafe { CFRelease(url) };
        let path = path?;
        let text = unsafe { &*(path as *const NSString) }.to_string();
        unsafe { CFRelease(path) };
        Some(std::path::PathBuf::from(text))
    };
    let mut out = Vec::new();
    let app = unsafe { AXUIElementCreateApplication(dock_pid) };
    if app.is_null() {
        return out;
    }
    let children = copy(app, ns_string!("AXChildren"));
    unsafe { CFRelease(app) };
    let Some(children) = children else { return out };
    for i in 0..unsafe { CFArrayGetCount(children) } {
        let child = unsafe { CFArrayGetValueAtIndex(children, i) };
        if text(child, ns_string!("AXRole")) != "AXList" {
            continue;
        }
        let Some(items) = copy(child, ns_string!("AXChildren")) else { continue };
        for n in 0..unsafe { CFArrayGetCount(items) } {
            let item = unsafe { CFArrayGetValueAtIndex(items, n) };
            let label = text(item, ns_string!("AXStatusLabel"));
            if label.trim().is_empty() || text(item, ns_string!("AXSubrole")) != "AXApplicationDockItem" {
                continue;
            }
            out.push(crate::badges::Badge {
                path: path_of(item),
                title: text(item, ns_string!("AXTitle")),
                label: label.trim().to_owned(),
            });
        }
        unsafe { CFRelease(items) };
    }
    unsafe { CFRelease(children) };
    out
}

/// Real key presses, as if typed, for trying the switcher by hand. Words: `cmd` (Cmd down), `up` (Cmd up), `tab`,
/// `shift-tab`, `esc`, `left`, `right`, `wait` (a quarter second).
pub fn post_keys(script: &str) {
    unsafe extern "C" {
        fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
        fn CGEventSetFlags(event: *mut c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }
    let post = |key: u16, down: bool, flags: u64| {
        let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), key, down) };
        if !event.is_null() {
            unsafe { CGEventSetFlags(event, flags) };
            unsafe { CGEventPost(0, event) };
            unsafe { CFRelease(event) };
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    };
    const SHIFT: u64 = 0x2_0000;
    let mut cmd = 0;
    for word in script.split_whitespace() {
        let tap = |key: u16, flags: u64| {
            post(key, true, flags);
            post(key, false, flags);
        };
        match word {
            "cmd" => {
                cmd = COMMAND;
                post(55, true, cmd);
            }
            "up" => {
                cmd = 0;
                post(55, false, 0);
            }
            "tab" => tap(48, cmd),
            "shift-tab" => tap(48, cmd | SHIFT),
            "esc" => tap(53, cmd),
            "left" => tap(123, cmd),
            "right" => tap(124, cmd),
            "wait" => std::thread::sleep(std::time::Duration::from_millis(250)),
            _ => {}
        }
    }
    // Never leave Cmd down.
    if cmd != 0 {
        post(55, false, 0);
    }
}

/// A real left click at `at` (global top-left points), posted as if from the mouse, for trying things by hand.
pub fn post_click(at: egui::Pos2) {
    unsafe extern "C" {
        fn CGEventCreateMouseEvent(source: *const c_void, kind: u32, at: Xy, button: u32) -> *mut c_void;
        fn CGEventPost(tap: u32, event: *mut c_void);
    }
    const MOVED: u32 = 5;
    let at = Xy { x: f64::from(at.x), y: f64::from(at.y) };
    for (kind, wait) in [(MOVED, 120), (MOUSE_DOWN, 70), (MOUSE_UP, 0)] {
        let event = unsafe { CGEventCreateMouseEvent(std::ptr::null(), kind, at, 0) };
        if !event.is_null() {
            unsafe { CGEventPost(0, event) };
            unsafe { CFRelease(event) };
        }
        std::thread::sleep(std::time::Duration::from_millis(wait));
    }
}

/// The middle of the front app's focused window's minimize button, global top-left points.
pub fn focused_minimize_button() -> Option<egui::Pos2> {
    let system = unsafe { AXUIElementCreateSystemWide() };
    let app = copy(system, ns_string!("AXFocusedApplication"));
    unsafe { CFRelease(system) };
    let app = app?;
    let window = copy(app, ns_string!("AXFocusedWindow"));
    unsafe { CFRelease(app) };
    let window = window?;
    let button = copy(window, ns_string!("AXMinimizeButton"));
    unsafe { CFRelease(window) };
    let button = button?;
    let at = value(button, ns_string!("AXPosition"), POINT);
    let size = value(button, ns_string!("AXSize"), SIZE);
    unsafe { CFRelease(button) };
    let (at, size) = (at?, size?);
    Some(egui::pos2((at.x + size.x / 2.0) as f32, (at.y + size.y / 2.0) as f32))
}

/// The text a key event types.
fn typed(event: *mut c_void) -> String {
    let mut text = [0u16; 8];
    let mut len = 0u64;
    unsafe { CGEventKeyboardGetUnicodeString(event, text.len() as u64, &mut len, text.as_mut_ptr()) };
    String::from_utf16_lossy(&text[..(len as usize).min(text.len())])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(left: f64, top: f64, w: f64, h: f64, menu: f64, dock: f64) -> Area {
        Area { left, top: top + menu, right: left + w, bottom: top + h - dock, screen_top: top, screen_bottom: top + h }
    }

    #[test]
    fn a_window_fills_the_screen_most_of_it_is_on() {
        // The laptop (main, with the dock) and a monitor to its left and a little higher.
        let laptop = area(0.0, 0.0, 1470.0, 956.0, 33.0, 80.0);
        let monitor = area(-1920.0, -111.0, 1920.0, 1200.0, 25.0, 0.0);
        let areas = [laptop, monitor];
        let at = |x, y| Xy { x, y };
        assert_eq!(area_for(&areas, at(-1500.0, 100.0), at(800.0, 600.0)), Some(monitor));
        assert_eq!(area_for(&areas, at(200.0, 100.0), at(800.0, 600.0)), Some(laptop));
        // Straddling: the side with more of it wins.
        assert_eq!(area_for(&areas, at(-300.0, 100.0), at(800.0, 600.0)), Some(laptop));
        assert_eq!(area_for(&areas, at(-600.0, 100.0), at(800.0, 600.0)), Some(monitor));
        // Above the main screen's top: only the monitor reaches there.
        assert_eq!(area_for(&areas, at(-900.0, -100.0), at(400.0, 50.0)), Some(monitor));
        assert_eq!(area_for(&areas, at(5000.0, 100.0), at(400.0, 300.0)), None, "on no screen");
        // The monitor fills from its own menu bar to its very bottom; the laptop stops above the dock.
        assert_eq!((monitor.top, monitor.bottom), (-86.0, 1089.0));
        assert_eq!((laptop.top, laptop.bottom), (33.0, 876.0));
    }
}
