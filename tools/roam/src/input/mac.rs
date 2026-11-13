//! macOS: an event tap hears the mouse and keyboard (and swallows them while the cursor is on
//! another computer), and events posted at the HID level play input back as if a real mouse and
//! keyboard made it. Both need the Accessibility permission, which notchbase already asks for
//! its volume and media keys.

#![allow(non_upper_case_globals)]

use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

use super::{Input, Platform, Play, Sink, Swipe, Watchdog};
use crate::keys;
use crate::layout::Rect;

type Ref = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

type TapCallback = extern "C" fn(proxy: Ref, kind: u32, event: Ref, user: *mut c_void) -> Ref;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: TapCallback, user: *mut c_void)
    -> Ref;
    fn CGEventTapEnable(tap: Ref, enable: bool);
    fn CGEventGetLocation(event: Ref) -> CGPoint;
    fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
    fn CGEventGetDoubleValueField(event: Ref, field: u32) -> f64;
    fn CGEventSetIntegerValueField(event: Ref, field: u32, value: i64);
    fn CGEventSetDoubleValueField(event: Ref, field: u32, value: f64);
    fn CGEventGetFlags(event: Ref) -> u64;
    fn CGEventSetFlags(event: Ref, flags: u64);
    fn CGEventSetType(event: Ref, kind: u32);
    fn CGEventCreateMouseEvent(source: Ref, kind: u32, at: CGPoint, button: u32) -> Ref;
    fn CGEventCreate(source: Ref) -> Ref;
    fn CGEventCreateKeyboardEvent(source: Ref, key: u16, down: bool) -> Ref;
    fn CGEventCreateScrollWheelEvent2(source: Ref, units: u32, count: u32, one: i32, two: i32, three: i32) -> Ref;
    fn CGEventPost(tap: u32, event: Ref);
    fn CGEventSourceCreate(state: i32) -> Ref;
    fn CGEventSourceSetLocalEventsSuppressionInterval(source: Ref, seconds: f64);
    fn CGWarpMouseCursorPosition(at: CGPoint) -> i32;
    fn CGAssociateMouseAndMouseCursorPosition(connected: bool) -> i32;
    fn CGDisplayHideCursor(display: u32) -> i32;
    fn CGDisplayShowCursor(display: u32) -> i32;
    fn CGCursorIsVisible() -> u32;
    fn CGMainDisplayID() -> u32;
    fn CGGetActiveDisplayList(most: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    // Lets a background app hide the cursor. Long used by remote-control apps.
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(connection: i32, target: i32, key: Ref, value: Ref) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: Ref;
    static kCFBooleanTrue: Ref;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFRunLoopGetCurrent() -> Ref;
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: Ref);
    fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
    fn CFMachPortInvalidate(port: Ref);
    fn CFRunLoopAddSource(run_loop: Ref, source: Ref, mode: Ref);
    fn CFRelease(object: Ref);
    fn CFRetain(object: Ref) -> Ref;
    fn CFStringCreateWithBytes(allocator: Ref, bytes: *const u8, length: isize, encoding: u32, bom: bool) -> Ref;
    fn CFDictionaryCreate(
        allocator: Ref,
        keys: *const Ref,
        values: *const Ref,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> Ref;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: Ref;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: Ref) -> bool;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const std::ffi::c_char) -> Ref;
    fn IOServiceGetMatchingService(main: u32, matching: Ref) -> u32;
    fn IOServiceOpen(service: u32, task: u32, kind: u32, connect: *mut u32) -> i32;
    fn IOServiceClose(connect: u32) -> i32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOHIDGetModifierLockState(connect: u32, selector: i32, state: *mut bool) -> i32;
    fn IOHIDSetModifierLockState(connect: u32, selector: i32, state: bool) -> i32;
}

unsafe extern "C" {
    static mach_task_self_: u32;
    fn pthread_set_qos_class_self_np(class: u32, priority: i32) -> i32;
}

const QOS_USER_INTERACTIVE: u32 = 0x21;

const SESSION_TAP: u32 = 1;
const HID_TAP: u32 = 0;

const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MOVED: u32 = 5;
const LEFT_DRAGGED: u32 = 6;
const RIGHT_DRAGGED: u32 = 7;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const FLAGS_CHANGED: u32 = 12;
const SCROLL: u32 = 22;
const OTHER_DOWN: u32 = 25;
const OTHER_UP: u32 = 26;
const OTHER_DRAGGED: u32 = 27;
/// The system's trackpad gestures, and the swipes that drive Mission Control and Spaces.
const GESTURE: u32 = 29;
const DOCK_CONTROL: u32 = 30;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_INPUT: u32 = 0xFFFF_FFFF;

const CLICK_STATE: u32 = 1;
const BUTTON_NUMBER: u32 = 3;
const DELTA_X: u32 = 4;
const DELTA_Y: u32 = 5;
const KEYCODE: u32 = 9;
const SCROLL_FIXED_1: u32 = 93;
const SCROLL_FIXED_2: u32 = 94;
const SCROLL_POINT_1: u32 = 96;
const SCROLL_POINT_2: u32 = 97;
const SCROLL_CONTINUOUS: u32 = 88;
const USER_DATA: u32 = 42;
const GESTURE_HID_TYPE: u32 = 110;
const SWIPE_MOTION: u32 = 123;
const SWIPE_PROGRESS: u32 = 124;
const GESTURE_PHASE: u32 = 132;
/// The gesture HID type of a Mission Control or Spaces swipe.
const DOCK_SWIPE: i64 = 23;
const MOTION_HORIZONTAL: i64 = 1;
const MOTION_VERTICAL: i64 = 2;
const PHASE_BEGAN: i64 = 1;
const PHASE_ENDED: i64 = 4;

/// Marks the events Roam posts, so its own tap lets them by.
const MARK: i64 = 0x526F_616D; // "Roam"

const SHIFT: u64 = 0x2_0000;
const CONTROL: u64 = 0x4_0000;
const ALTERNATE: u64 = 0x8_0000;
const COMMAND: u64 = 0x10_0000;
const NONCOALESCED: u64 = 0x100;
const ALPHA_SHIFT: u64 = 0x1_0000;
const NUMERIC_PAD: u64 = 0x20_0000;
const SECONDARY_FN: u64 = 0x80_0000;

/// The extra flags a real keyboard puts on some keys. Apps and shortcuts check them: without
/// the function flag, ⌃↑ doesn't open Mission Control and ⌥⌘← doesn't switch tabs.
fn key_flags(code: u16) -> u64 {
    match code {
        // The arrows.
        123..=126 => NUMERIC_PAD | SECONDARY_FN,
        // Help, Home, Page Up, Forward Delete, End, Page Down and the function keys.
        114..=122 | 96..=101 | 103 | 105..=107 | 109 | 111 | 113 | 64 | 79 | 80 | 90 => SECONDARY_FN,
        // The keypad.
        65 | 67 | 69 | 71 | 75 | 76 | 78 | 81..=89 | 91 | 92 => NUMERIC_PAD,
        _ => 0,
    }
}

/// Each modifier key: its HID code, the family flag, and its own left/right device flag.
const MODIFIERS: &[(u16, u64, u64)] = &[
    (keys::LEFT_CTRL, CONTROL, 0x0001),
    (keys::LEFT_SHIFT, SHIFT, 0x0002),
    (keys::RIGHT_SHIFT, SHIFT, 0x0004),
    (keys::LEFT_GUI, COMMAND, 0x0008),
    (keys::RIGHT_GUI, COMMAND, 0x0010),
    (keys::LEFT_ALT, ALTERNATE, 0x0020),
    (keys::RIGHT_ALT, ALTERNATE, 0x0040),
    (keys::RIGHT_CTRL, CONTROL, 0x2000),
];

const CAPS_LOCK: u16 = 0x39;
/// One wheel notch (120) scrolls this many pixels.
const PIXELS_PER_NOTCH: f32 = 40.0;
const DOUBLE_CLICK: f64 = 0.5;

static WATCHDOG: Watchdog = Watchdog::new();

/// Flips Caps Lock the way the key itself does. A posted Caps Lock key press doesn't turn it on
/// or off, so the lock state is set directly.
/// Returns whether it's on now.
fn toggle_caps_lock() -> bool {
    caps_lock(true)
}

/// Whether Caps Lock is on, flipping it first when `flip`.
fn caps_lock(flip: bool) -> bool {
    let mut on = false;
    // SAFETY: IOKit calls on the HID system service; everything opened is closed again.
    unsafe {
        let service = IOServiceGetMatchingService(0, IOServiceMatching(c"IOHIDSystem".as_ptr()));
        if service == 0 {
            return false;
        }
        let mut connect = 0u32;
        if IOServiceOpen(service, mach_task_self_, 1, &mut connect) == 0 {
            if IOHIDGetModifierLockState(connect, 1, &mut on) == 0 && flip {
                on = !on;
                IOHIDSetModifierLockState(connect, 1, on);
            }
            IOServiceClose(connect);
        }
        IOObjectRelease(service);
    }
    on
}

pub fn urgent_thread() {
    // SAFETY: changes only the calling thread's scheduling class.
    unsafe { pthread_set_qos_class_self_np(QOS_USER_INTERACTIVE, 0) };
}

/// What the tap callback needs. Lives as long as the tap.
struct Tap {
    sink: Sink,
    captured: Arc<AtomicBool>,
    port: AtomicPtr<c_void>,
    /// A second tap, at the session level, for the system's gestures.
    gestures: AtomicPtr<c_void>,
    local: Mutex<HashSet<u16>>,
    swipe: Mutex<Swiping>,
}

/// A Mission Control or Spaces swipe so far.
#[derive(Default)]
struct Swiping {
    motion: i64,
    progress: f64,
}

impl Swiping {
    /// Follows one event of the swipe. The swipe, once it ends.
    fn follow(&mut self, phase: i64, motion: i64, progress: f64) -> Option<Swipe> {
        if phase == PHASE_BEGAN {
            *self = Swiping::default();
        }
        if motion != 0 {
            self.motion = motion;
        }
        self.progress += progress;
        if phase != PHASE_ENDED {
            return None;
        }
        let swiping = std::mem::take(self);
        match swiping.motion {
            _ if swiping.progress == 0.0 => None,
            MOTION_HORIZONTAL => Some(if swiping.progress > 0.0 { Swipe::Right } else { Swipe::Left }),
            MOTION_VERTICAL => Some(if swiping.progress < 0.0 { Swipe::Up } else { Swipe::Down }),
            _ => None,
        }
    }
}

struct Running {
    run_loop: usize,
    tap: Arc<Tap>,
}

pub struct Mac {
    captured: Arc<AtomicBool>,
    hidden: bool,
    running: Option<Running>,
    source: usize,
    // Playing input back.
    at: CGPoint,
    buttons: u8,
    modifiers: HashSet<u16>,
    last_click: Option<(u8, Instant, CGPoint, i64)>,
    scroll_carry: [f32; 2],
    caps: bool,
    activity: Option<Activity>,
    /// Where the cursor was hidden when another computer's cursor went back to it.
    departed: Option<CGPoint>,
    /// Fast Wi-Fi: the helper was offered this run, and the flag is being kept fresh.
    wifi_offered: bool,
    wifi_on: bool,
}

struct Activity(Retained<ProtocolObject<dyn NSObjectProtocol>>);

// The token is only kept and handed back to NSProcessInfo, which is thread safe.
unsafe impl Send for Activity {}

impl Mac {
    pub fn new() -> Self {
        // SAFETY: plain CoreGraphics calls with valid arguments.
        let source = unsafe {
            let source = CGEventSourceCreate(1);
            if !source.is_null() {
                CGEventSourceSetLocalEventsSuppressionInterval(source, 0.0);
            }
            source as usize
        };
        Self {
            captured: Arc::new(AtomicBool::new(false)),
            hidden: false,
            running: None,
            source,
            at: CGPoint::default(),
            buttons: 0,
            modifiers: HashSet::new(),
            last_click: None,
            scroll_carry: [0.0; 2],
            caps: caps_lock(false),
            activity: None,
            departed: None,
            wifi_offered: false,
            wifi_on: false,
        }
    }

    fn source(&self) -> Ref {
        self.source as Ref
    }

    fn flags(&self) -> u64 {
        let mut flags = NONCOALESCED;
        for (hid, family, device) in MODIFIERS {
            if self.modifiers.contains(hid) {
                flags |= family | device;
            }
        }
        if self.caps {
            flags |= ALPHA_SHIFT;
        }
        flags
    }

    fn post(&self, event: Ref) {
        if event.is_null() {
            return;
        }
        // SAFETY: `event` was just created and is released once posted.
        unsafe {
            CGEventSetIntegerValueField(event, USER_DATA, MARK);
            CGEventPost(HID_TAP, event);
            CFRelease(event);
        }
    }

    fn show_cursor(&mut self, show: bool) {
        if show == !self.hidden {
            return;
        }
        // SAFETY: plain CoreGraphics calls; hiding and showing are kept balanced.
        unsafe {
            if show {
                CGDisplayShowCursor(CGMainDisplayID());
            } else {
                allow_hiding_in_background();
                CGDisplayHideCursor(CGMainDisplayID());
            }
        }
        self.hidden = !show;
    }
}

fn allow_hiding_in_background() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        let key = b"SetsCursorInBackground";
        // SAFETY: a CFString made from valid UTF-8, then handed to the window server.
        unsafe {
            let name =
                CFStringCreateWithBytes(std::ptr::null_mut(), key.as_ptr(), key.len() as isize, 0x0800_0100, false);
            if !name.is_null() {
                let connection = _CGSDefaultConnection();
                CGSSetConnectionProperty(connection, connection, name, kCFBooleanTrue);
                CFRelease(name);
            }
        }
    });
}

/// Where the cursor is now.
fn cursor_now() -> Option<CGPoint> {
    // SAFETY: an empty event made only to read the cursor's location, then released.
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let at = CGEventGetLocation(event);
        CFRelease(event);
        Some(at)
    }
}

/// Fast Wi-Fi. AirDrop and Handoff keep a second Wi-Fi link (awdl0) that takes the radio off
/// the network about twice a second, for up to a quarter of a second: a cursor sent over Wi-Fi
/// stutters each time. Only root can switch that link off, so a tiny helper is installed once,
/// with the person's password: while Roam keeps a flag file fresh, the helper keeps awdl0 down,
/// and it brings it back up within seconds of the flag going stale (Roam ending, or quitting).
mod wifi {
    use std::path::Path;
    use std::process::Command;

    const FLAG: &str = "/tmp/notchbase-fast-wifi";
    const LABEL: &str = "app.notchbase.fast-wifi";
    const HELPER: &str = "/Library/PrivilegedHelperTools/app.notchbase.fast-wifi";
    const DAEMON: &str = "/Library/LaunchDaemons/app.notchbase.fast-wifi.plist";

    const SCRIPT: &str = r#"#!/bin/sh
# Installed by notchbase's Roam (Fast Wi-Fi). While Roam keeps the flag file fresh, AirDrop's
# Wi-Fi link (awdl0) stays off, so the Mac's radio stays on the network. When the flag goes
# stale it comes back on. Remove with: sudo launchctl bootout system/app.notchbase.fast-wifi
flag=/tmp/notchbase-fast-wifi
off=0
while :; do
    if [ -f "$flag" ] && [ $(( $(date +%s) - $(stat -f %m "$flag") )) -lt 8 ]; then
        /sbin/ifconfig awdl0 down 2>/dev/null
        off=1
    elif [ "$off" = 1 ]; then
        /sbin/ifconfig awdl0 up 2>/dev/null
        off=0
    fi
    sleep 1
done
"#;

    fn daemon_plist() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>{HELPER}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#
        )
    }

    pub fn installed() -> bool {
        Path::new(HELPER).exists() && Path::new(DAEMON).exists()
    }

    /// Asks for the password and installs the helper, off the calling thread.
    pub fn install() {
        let _ = std::thread::Builder::new().name("roam-fast-wifi".into()).spawn(|| {
            let folder = std::env::temp_dir().join(format!("notchbase-fast-wifi-{}", std::process::id()));
            if std::fs::create_dir_all(&folder).is_err() {
                return;
            }
            let script = folder.join("helper.sh");
            let plist = folder.join("daemon.plist");
            if std::fs::write(&script, SCRIPT).is_err() || std::fs::write(&plist, daemon_plist()).is_err() {
                return;
            }
            let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
            let shell = format!(
                "mkdir -p /Library/PrivilegedHelperTools &&                  install -m 755 -o root -g wheel {} {HELPER} &&                  install -m 644 -o root -g wheel {} {DAEMON} &&                  (launchctl bootout system/{LABEL} 2>/dev/null; launchctl bootstrap system {DAEMON})",
                quote(&script),
                quote(&plist),
            );
            let escaped = shell.replace('\\', "\\\\").replace('"', "\\\"");
            let apple = format!(
                "do shell script \"{escaped}\" with administrator privileges with prompt \"Roam's Fast Wi-Fi pauses AirDrop and Handoff while you share the mouse, so Wi-Fi stays steady. It needs your password once.\""
            );
            let _ = Command::new("/usr/bin/osascript").args(["-e", &apple]).status();
            let _ = std::fs::remove_dir_all(&folder);
        });
    }

    /// Keeps the flag fresh, or takes it away.
    pub fn flag(on: bool) {
        if on {
            let _ = std::fs::write(FLAG, b"");
        } else {
            let _ = std::fs::remove_file(FLAG);
        }
    }
}

impl Drop for Mac {
    fn drop(&mut self) {
        self.capture(false);
        self.reveal();
        if self.wifi_on {
            wifi::flag(false);
        }
        self.stop_listening();
        if self.source != 0 {
            // SAFETY: created in `new`, released once.
            unsafe { CFRelease(self.source()) };
        }
    }
}

extern "C" fn tapped(_proxy: Ref, kind: u32, event: Ref, user: *mut c_void) -> Ref {
    // SAFETY: `user` is the `Tap` kept alive by `Running` for as long as the tap exists.
    let tap = unsafe { &*(user as *const Tap) };
    if kind == TAP_DISABLED_BY_TIMEOUT || kind == TAP_DISABLED_BY_INPUT {
        for port in [tap.port.load(Ordering::Acquire), tap.gestures.load(Ordering::Acquire)] {
            if !port.is_null() {
                // SAFETY: the ports are valid while the tap runs.
                unsafe { CGEventTapEnable(port, true) };
            }
        }
        return event;
    }
    // SAFETY: `event` is a valid event for the duration of the callback.
    unsafe {
        if CGEventGetIntegerValueField(event, USER_DATA) == MARK {
            return event;
        }
    }
    let mut captured = tap.captured.load(Ordering::Acquire);
    if captured && WATCHDOG.stale() {
        // Roam stopped checking in: give the mouse and keyboard back now.
        tap.captured.store(false, Ordering::Release);
        captured = false;
        // SAFETY: plain CoreGraphics calls.
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(true);
            CGDisplayShowCursor(CGMainDisplayID());
        }
    }
    let swallow = if captured { std::ptr::null_mut() } else { event };
    // SAFETY: as above.
    unsafe {
        match kind {
            MOVED | LEFT_DRAGGED | RIGHT_DRAGGED | OTHER_DRAGGED => {
                let at = CGEventGetLocation(event);
                let delta = [CGEventGetDoubleValueField(event, DELTA_X), CGEventGetDoubleValueField(event, DELTA_Y)];
                (tap.sink)(Input::Move { at: [at.x, at.y], delta });
                swallow
            }
            LEFT_DOWN | LEFT_UP | RIGHT_DOWN | RIGHT_UP | OTHER_DOWN | OTHER_UP => {
                let button = match kind {
                    LEFT_DOWN | LEFT_UP => 0,
                    RIGHT_DOWN | RIGHT_UP => 1,
                    _ => CGEventGetIntegerValueField(event, BUTTON_NUMBER).clamp(2, 4) as u8,
                };
                let down = matches!(kind, LEFT_DOWN | RIGHT_DOWN | OTHER_DOWN);
                (tap.sink)(Input::Button { button, down });
                swallow
            }
            SCROLL => {
                let (dx, dy) = if CGEventGetIntegerValueField(event, SCROLL_CONTINUOUS) != 0 {
                    let scale = 120.0 / f64::from(PIXELS_PER_NOTCH);
                    (
                        -CGEventGetDoubleValueField(event, SCROLL_POINT_2) * scale,
                        CGEventGetDoubleValueField(event, SCROLL_POINT_1) * scale,
                    )
                } else {
                    (
                        -CGEventGetDoubleValueField(event, SCROLL_FIXED_2) * 120.0,
                        CGEventGetDoubleValueField(event, SCROLL_FIXED_1) * 120.0,
                    )
                };
                (tap.sink)(Input::Wheel { dx: dx as f32, dy: dy as f32 });
                swallow
            }
            // While the cursor is on another computer, the system's swipes don't act here: Mission
            // Control and Spaces swipes become the same thing over there.
            GESTURE | DOCK_CONTROL => {
                if !captured {
                    return event;
                }
                if CGEventGetIntegerValueField(event, GESTURE_HID_TYPE) == DOCK_SWIPE {
                    let phase = CGEventGetIntegerValueField(event, GESTURE_PHASE);
                    let motion = CGEventGetIntegerValueField(event, SWIPE_MOTION);
                    let progress = CGEventGetDoubleValueField(event, SWIPE_PROGRESS);
                    let ended = tap.swipe.lock().unwrap_or_else(|e| e.into_inner()).follow(phase, motion, progress);
                    if let Some(swipe) = ended {
                        (tap.sink)(Input::Swipe(swipe));
                    }
                }
                std::ptr::null_mut()
            }
            KEY_DOWN | KEY_UP => {
                let code = CGEventGetIntegerValueField(event, KEYCODE) as u16;
                let Some(hid) = keys::from_mac(code) else { return swallow };
                // Held keys repeat as more presses; the other computer types them like its own.
                key(tap, hid, kind == KEY_DOWN, captured, event)
            }
            FLAGS_CHANGED => {
                let code = CGEventGetIntegerValueField(event, KEYCODE) as u16;
                let flags = CGEventGetFlags(event);
                let Some(hid) = keys::from_mac(code) else { return swallow };
                if hid == CAPS_LOCK {
                    if !captured {
                        return event;
                    }
                    // Caps Lock only says it toggled: send a whole press.
                    (tap.sink)(Input::Key { hid, down: true });
                    (tap.sink)(Input::Key { hid, down: false });
                    return std::ptr::null_mut();
                }
                let Some((_, _, device)) = MODIFIERS.iter().find(|(h, _, _)| *h == hid) else { return swallow };
                let down = flags & device != 0;
                key(tap, hid, down, captured, event)
            }
            _ => event,
        }
    }
}

/// A key: pressed here before the cursor left, its release stays here; while captured,
/// everything else goes to the other computer.
fn key(tap: &Tap, hid: u16, down: bool, captured: bool, event: Ref) -> Ref {
    let mut local = tap.local.lock().unwrap_or_else(|e| e.into_inner());
    if !captured {
        if down {
            local.insert(hid);
        } else {
            local.remove(&hid);
        }
        return event;
    }
    if !down && local.remove(&hid) {
        return event;
    }
    drop(local);
    (tap.sink)(Input::Key { hid, down });
    std::ptr::null_mut()
}

impl Platform for Mac {
    fn screens(&mut self) -> (Vec<Rect>, f64) {
        let mut ids = [0u32; 16];
        let mut count = 0u32;
        // SAFETY: the buffer holds 16 displays and CoreGraphics writes at most that many.
        let found = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) };
        let mut displays = Vec::new();
        if found == 0 {
            for id in &ids[..(count as usize).min(16)] {
                // SAFETY: an id CoreGraphics just gave.
                let bounds = unsafe { CGDisplayBounds(*id) };
                displays.push(Rect::new(bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height));
            }
        }
        if displays.is_empty() {
            displays.push(Rect::new(0.0, 0.0, 1440.0, 900.0));
        }
        (displays, 1.0)
    }

    fn listen(&mut self, sink: Sink) -> Result<(), String> {
        self.stop_listening();
        if !trusted() {
            return Err(PERMISSION.into());
        }
        let tap = Arc::new(Tap {
            sink,
            captured: self.captured.clone(),
            port: AtomicPtr::new(std::ptr::null_mut()),
            gestures: AtomicPtr::new(std::ptr::null_mut()),
            local: Mutex::new(HashSet::new()),
            swipe: Mutex::new(Swiping::default()),
        });
        let (ready, started) = std::sync::mpsc::channel::<Result<usize, String>>();
        let keep = tap.clone();
        let spawned = std::thread::Builder::new().name("roam-tap".into()).spawn(move || {
            let mask: u64 = [
                LEFT_DOWN,
                LEFT_UP,
                RIGHT_DOWN,
                RIGHT_UP,
                MOVED,
                LEFT_DRAGGED,
                RIGHT_DRAGGED,
                KEY_DOWN,
                KEY_UP,
                FLAGS_CHANGED,
                SCROLL,
                OTHER_DOWN,
                OTHER_UP,
                OTHER_DRAGGED,
                GESTURE,
                DOCK_CONTROL,
            ]
            .iter()
            .fold(0, |mask, kind| mask | (1 << kind));
            let user = Arc::as_ptr(&keep) as *mut c_void;
            // SAFETY: the callback and its data outlive the tap; the run loop is this thread's.
            unsafe {
                pthread_set_qos_class_self_np(QOS_USER_INTERACTIVE, 0);
                // At the HID level the tap comes before the system's own shortcuts (Command
                // Tab, Spotlight, Mission Control), so those reach the other computer too.
                let mut port = CGEventTapCreate(HID_TAP, 0, 0, mask, tapped, user);
                if port.is_null() {
                    port = CGEventTapCreate(SESSION_TAP, 0, 0, mask, tapped, user);
                }
                if port.is_null() {
                    let _ = ready.send(Err(PERMISSION.into()));
                    return;
                }
                keep.port.store(port, Ordering::Release);
                let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), port, 0);
                let run_loop = CFRunLoopGetCurrent();
                CFRetain(run_loop);
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(port, true);
                // Gestures may only be seen at the session level. Whichever tap sees one first
                // decides: one that swallows it keeps it from the other.
                let gesture_mask = (1u64 << GESTURE) | (1u64 << DOCK_CONTROL);
                let gestures = CGEventTapCreate(SESSION_TAP, 0, 0, gesture_mask, tapped, user);
                let gesture_source = if gestures.is_null() {
                    std::ptr::null_mut()
                } else {
                    keep.gestures.store(gestures, Ordering::Release);
                    let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), gestures, 0);
                    CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                    CGEventTapEnable(gestures, true);
                    source
                };
                let _ = ready.send(Ok(run_loop as usize));
                CFRunLoopRun();
                if !gestures.is_null() {
                    CGEventTapEnable(gestures, false);
                    keep.gestures.store(std::ptr::null_mut(), Ordering::Release);
                    CFMachPortInvalidate(gestures);
                    CFRelease(gesture_source);
                    CFRelease(gestures);
                }
                CGEventTapEnable(port, false);
                keep.port.store(std::ptr::null_mut(), Ordering::Release);
                CFMachPortInvalidate(port);
                CFRelease(source);
                CFRelease(port);
                CFRelease(run_loop);
            }
            drop(keep);
        });
        if spawned.is_err() {
            return Err("Roam couldn't start listening to the mouse".into());
        }
        match started.recv() {
            Ok(Ok(run_loop)) => {
                self.running = Some(Running { run_loop, tap });
                Ok(())
            }
            Ok(Err(err)) => Err(err),
            Err(_) => Err("Roam couldn't start listening to the mouse".into()),
        }
    }

    fn stop_listening(&mut self) {
        if let Some(running) = self.running.take() {
            // SAFETY: the run loop was retained by the tap thread until it exits.
            unsafe { CFRunLoopStop(running.run_loop as Ref) };
            drop(running.tap);
        }
    }

    fn capture(&mut self, on: bool) {
        if on {
            WATCHDOG.pet();
        }
        // Hidden because another computer's cursor left: capturing takes the hiding over.
        self.departed = None;
        self.captured.store(on, Ordering::Release);
        // SAFETY: plain CoreGraphics call.
        unsafe { CGAssociateMouseAndMouseCursorPosition(!on) };
        self.show_cursor(!on);
    }

    fn warp(&mut self, at: [f64; 2]) {
        // SAFETY: plain CoreGraphics calls. Reconnecting the mouse right after a warp lifts the
        // short pause macOS otherwise puts on mouse input.
        unsafe {
            CGWarpMouseCursorPosition(CGPoint { x: at[0], y: at[1] });
            if !self.captured.load(Ordering::Acquire) {
                CGAssociateMouseAndMouseCursorPosition(true);
            }
        }
    }

    fn play(&mut self, play: Play) {
        match play {
            Play::Move(at) => {
                let at = CGPoint { x: at[0], y: at[1] };
                let (kind, button) = if self.buttons & 1 != 0 {
                    (LEFT_DRAGGED, 0)
                } else if self.buttons & 2 != 0 {
                    (RIGHT_DRAGGED, 1)
                } else if self.buttons != 0 {
                    (OTHER_DRAGGED, self.buttons.trailing_zeros())
                } else {
                    (MOVED, 0)
                };
                // SAFETY: a new event, posted and released by `post`.
                let event = unsafe {
                    let event = CGEventCreateMouseEvent(self.source(), kind, at, button);
                    if !event.is_null() {
                        CGEventSetDoubleValueField(event, DELTA_X, at.x - self.at.x);
                        CGEventSetDoubleValueField(event, DELTA_Y, at.y - self.at.y);
                        CGEventSetFlags(event, self.flags());
                    }
                    event
                };
                self.at = at;
                self.post(event);
            }
            Play::Button { button, down } => {
                let kind = match (button, down) {
                    (0, true) => LEFT_DOWN,
                    (0, false) => LEFT_UP,
                    (1, true) => RIGHT_DOWN,
                    (1, false) => RIGHT_UP,
                    (_, true) => OTHER_DOWN,
                    (_, false) => OTHER_UP,
                };
                if down {
                    self.buttons |= 1 << button;
                } else {
                    self.buttons &= !(1 << button);
                }
                let clicks = if down {
                    let now = Instant::now();
                    let count = match self.last_click {
                        Some((last, at, point, count))
                            if last == button
                                && now.duration_since(at).as_secs_f64() < DOUBLE_CLICK
                                && (point.x - self.at.x).hypot(point.y - self.at.y) < 5.0 =>
                        {
                            (count + 1).min(3)
                        }
                        _ => 1,
                    };
                    self.last_click = Some((button, now, self.at, count));
                    count
                } else {
                    self.last_click.map_or(1, |(_, _, _, count)| count)
                };
                // SAFETY: a new event, posted and released by `post`.
                let event = unsafe {
                    let event = CGEventCreateMouseEvent(self.source(), kind, self.at, u32::from(button));
                    if !event.is_null() {
                        CGEventSetIntegerValueField(event, CLICK_STATE, clicks);
                        CGEventSetIntegerValueField(event, BUTTON_NUMBER, i64::from(button));
                        CGEventSetFlags(event, self.flags());
                    }
                    event
                };
                self.post(event);
            }
            Play::Wheel { dx, dy } => {
                let carry = &mut self.scroll_carry;
                carry[0] += -dx / 120.0 * PIXELS_PER_NOTCH;
                carry[1] += dy / 120.0 * PIXELS_PER_NOTCH;
                let whole = [carry[0].trunc(), carry[1].trunc()];
                carry[0] -= whole[0];
                carry[1] -= whole[1];
                if whole == [0.0, 0.0] {
                    return;
                }
                // SAFETY: a new event, posted and released by `post`.
                let event =
                    unsafe { CGEventCreateScrollWheelEvent2(self.source(), 0, 2, whole[1] as i32, whole[0] as i32, 0) };
                if !event.is_null() {
                    // SAFETY: as above.
                    unsafe { CGEventSetIntegerValueField(event, SCROLL_CONTINUOUS, 1) };
                }
                self.post(event);
            }
            Play::Key { hid: CAPS_LOCK, down } => {
                if down {
                    self.caps = toggle_caps_lock();
                }
            }
            Play::Key { hid, down } => {
                let Some(code) = keys::to_mac(hid) else { return };
                let modifier = keys::is_modifier(hid);
                if modifier {
                    if down {
                        self.modifiers.insert(hid);
                    } else {
                        self.modifiers.remove(&hid);
                    }
                }
                // SAFETY: a new event, posted and released by `post`.
                let event = unsafe {
                    let event = CGEventCreateKeyboardEvent(self.source(), code, down);
                    if !event.is_null() {
                        if modifier {
                            CGEventSetType(event, FLAGS_CHANGED);
                        }
                        CGEventSetFlags(event, self.flags() | key_flags(code));
                    }
                    event
                };
                self.post(event);
            }
        }
    }

    fn alive(&mut self) {
        WATCHDOG.pet();
    }

    fn depart(&mut self) {
        if self.captured.load(Ordering::Acquire) || self.departed.is_some() || self.hidden {
            return;
        }
        let Some(at) = cursor_now() else { return };
        self.departed = Some(at);
        self.show_cursor(false);
    }

    fn reveal(&mut self) {
        if self.departed.take().is_some() {
            self.show_cursor(true);
        }
    }

    fn tick(&mut self) {
        // Someone at the Mac moved its own mouse or trackpad: give them their cursor back.
        if let Some(left) = self.departed
            && let Some(now) = cursor_now()
            && (now.x - left.x).hypot(now.y - left.y) > 0.5
        {
            self.reveal();
        }
    }

    fn steady_wifi(&mut self, on: bool) {
        if on && !self.wifi_offered && !wifi::installed() {
            self.wifi_offered = true;
            wifi::install();
        }
        if on || self.wifi_on {
            wifi::flag(on);
        }
        self.wifi_on = on;
    }

    fn arrive(&mut self) {
        if self.captured.load(Ordering::Acquire) {
            return;
        }
        self.reveal();
        // macOS keeps the cursor hidden after typing, or after another app hid it, until a real
        // mouse moves. Posted moves don't count, so show it outright. Showing from the
        // background needs the same window server permission hiding does.
        allow_hiding_in_background();
        // SAFETY: plain CoreGraphics calls; showing an already visible cursor does nothing.
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(true);
            for _ in 0..8 {
                if CGCursorIsVisible() != 0 {
                    break;
                }
                CGDisplayShowCursor(CGMainDisplayID());
            }
        }
        self.caps = caps_lock(false);
    }

    fn still_captured(&mut self) -> bool {
        let captured = self.captured.load(Ordering::Acquire);
        if !captured && self.hidden {
            // The tap let go by itself and already showed the cursor.
            self.hidden = false;
        }
        captured
    }

    fn missing_permission(&mut self) -> Option<String> {
        (!trusted()).then(|| PERMISSION.into())
    }

    fn ask_permission(&mut self) {
        // SAFETY: a one-entry CFDictionary of CF constants, released after use.
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let options = CFDictionaryCreate(
                std::ptr::null_mut(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            AXIsProcessTrustedWithOptions(options);
            if !options.is_null() {
                CFRelease(options);
            }
        }
    }

    fn fast(&mut self, on: bool) {
        // Tell macOS this is latency critical, as game streaming apps do: no App Nap, no timer
        // coalescing, while a session runs.
        if on && self.activity.is_none() {
            // SAFETY: the engine thread runs this whole time; give it priority too.
            unsafe { pthread_set_qos_class_self_np(QOS_USER_INTERACTIVE, 0) };
            let options = NSActivityOptions::LatencyCritical | NSActivityOptions::UserInitiated;
            let reason = NSString::from_str("Sharing the keyboard and mouse");
            let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &reason);
            self.activity = Some(Activity(token));
        } else if !on && let Some(Activity(token)) = self.activity.take() {
            // SAFETY: the token came from beginActivity.
            unsafe { NSProcessInfo::processInfo().endActivity(&token) };
        }
    }
}

const PERMISSION: &str = "Allow notchbase in Privacy & Security › Accessibility so Roam can use the mouse and keyboard";

fn trusted() -> bool {
    // SAFETY: a plain query.
    unsafe { AXIsProcessTrusted() }
}
