//! macOS: an event tap hears fn (or the right ⌥ or ⌘), AudioQueue records the microphone at
//! 16 kHz, Accessibility reads the text around the cursor, and the words go in as keystrokes
//! that carry the text, leaving the clipboard alone. All of it needs the Accessibility
//! permission, which notchbase already asks for its volume and media keys.

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use objc2::msg_send;
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyClass;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSSound, NSWorkspace};
use objc2_foundation::NSString;

use super::{Cue, Hotkey, KeySink, Platform, Problem, Recording, Surroundings, Tape, Typed};
use crate::sound::{Converter, RATE, level};
use crate::trigger::Key;

type Ref = *mut c_void;

type TapCallback = extern "C" fn(proxy: Ref, kind: u32, event: Ref, user: *mut c_void) -> Ref;
type InputCallback = extern "C" fn(
    user: *mut c_void,
    queue: Ref,
    buffer: *mut AudioQueueBuffer,
    start: *const c_void,
    count: u32,
    descriptions: *const c_void,
);

#[repr(C)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioQueueBuffer {
    capacity: u32,
    data: *mut c_void,
    size: u32,
    user: *mut c_void,
    description_capacity: u32,
    descriptions: *mut c_void,
    description_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: TapCallback, user: *mut c_void)
    -> Ref;
    fn CGEventTapEnable(tap: Ref, enable: bool);
    fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
    fn CGEventGetFlags(event: Ref) -> u64;
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSourceCreate(state: i32) -> Ref;
    fn CGEventCreateKeyboardEvent(source: Ref, key: u16, down: bool) -> Ref;
    fn CGEventSetFlags(event: Ref, flags: u64);
    fn CGEventKeyboardSetUnicodeString(event: Ref, length: usize, text: *const u16);
    fn CGEventPost(tap: u32, event: Ref);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: Ref;
    fn CFRunLoopGetCurrent() -> Ref;
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: Ref);
    fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
    fn CFMachPortInvalidate(port: Ref);
    fn CFRunLoopAddSource(run_loop: Ref, source: Ref, mode: Ref);
    fn CFRelease(object: Ref);
    fn CFRetain(object: Ref) -> Ref;
    fn CFGetTypeID(object: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringCreateWithBytes(allocator: Ref, bytes: *const u8, length: isize, encoding: u32, bom: bool) -> Ref;
    fn CFStringGetLength(string: Ref) -> isize;
    fn CFStringGetCharacters(string: Ref, range: CFRange, buffer: *mut u16);
    fn CFNumberGetValue(number: Ref, kind: isize, value: *mut c_void) -> bool;
    fn CFPreferencesCopyAppValue(key: Ref, app: Ref) -> Ref;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> Ref;
    fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, value: *mut Ref) -> i32;
    fn AXUIElementIsAttributeSettable(element: Ref, attribute: Ref, settable: *mut bool) -> i32;
    fn AXUIElementSetMessagingTimeout(element: Ref, seconds: f32) -> i32;
    fn AXValueGetValue(value: Ref, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioQueueNewInput(
        format: *const AudioStreamBasicDescription,
        callback: InputCallback,
        user: *mut c_void,
        run_loop: Ref,
        mode: Ref,
        flags: u32,
        queue: *mut Ref,
    ) -> i32;
    fn AudioQueueAllocateBuffer(queue: Ref, size: u32, buffer: *mut *mut AudioQueueBuffer) -> i32;
    fn AudioQueueEnqueueBuffer(
        queue: Ref,
        buffer: *mut AudioQueueBuffer,
        count: u32,
        descriptions: *const c_void,
    ) -> i32;
    fn AudioQueueStart(queue: Ref, start: *const c_void) -> i32;
    fn AudioQueueStop(queue: Ref, immediate: bool) -> i32;
    fn AudioQueueDispose(queue: Ref, immediate: bool) -> i32;
}

const HID_TAP: u32 = 0;
const SESSION_TAP: u32 = 1;
const HEAD: u32 = 0;
const ACTIVE: u32 = 0;
const KEY_DOWN: u32 = 10;
const FLAGS_CHANGED: u32 = 12;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_INPUT: u32 = 0xFFFF_FFFF;
const KEYCODE: u32 = 9;
const AUTOREPEAT: u32 = 8;
const HID_STATE: i32 = 1;
const PRIVATE_STATE: i32 = -1;

const FN_CODE: i64 = 63;
const RIGHT_OPTION_CODE: i64 = 61;
const RIGHT_COMMAND_CODE: i64 = 54;
const ESCAPE_CODE: i64 = 53;
const RETURN_CODE: u16 = 36;
const SECONDARY_FN: u64 = 0x80_0000;
const RIGHT_OPTION: u64 = 0x40;
const RIGHT_COMMAND: u64 = 0x10;

const LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const SIGNED_PACKED: u32 = 0x4 | 0x8;
/// Rates to ask AudioQueue for. It converts to 16 kHz itself on most Macs; the others are
/// converted here.
const RATES: [u32; 3] = [16_000, 48_000, 44_100];
/// Each buffer holds 50 ms, so the meter moves smoothly and nothing waits long.
const BUFFER_MS: u32 = 50;
const BUFFERS: usize = 4;

const UTF8: u32 = 0x0800_0100;
const NUMBER_INT: isize = 9;
const RANGE_TYPE: u32 = 4;
const AX_TIMEOUT: f32 = 0.25;
/// Text read on each side of the cursor.
const BEFORE_CHARS: isize = 1500;
const AFTER_CHARS: isize = 400;
/// The most characters one keystroke carries.
const CHUNK: usize = 20;
const CUE_VOLUME: f32 = 0.22;
const EDITABLE_ROLES: [&str; 5] = ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField", "AXWebArea"];

const ACCESSIBILITY: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
const MICROPHONE: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
const KEYBOARD: &str = "x-apple.systempreferences:com.apple.Keyboard-Settings.extension";

#[derive(Default)]
pub struct Native {
    tap: Option<KeyTap>,
}

impl Platform for Native {
    fn watch(&mut self, hotkey: Hotkey, keys: KeySink, listening: Arc<AtomicBool>) -> Result<(), Problem> {
        self.tap = None;
        if !unsafe { AXIsProcessTrusted() } {
            return Err(accessibility());
        }
        self.tap = Some(KeyTap::install(hotkey, keys, listening).ok_or_else(accessibility)?);
        Ok(())
    }

    fn hotkey_down(&self, hotkey: Hotkey) -> Option<bool> {
        let flags = unsafe { CGEventSourceFlagsState(HID_STATE) };
        Some(flags & mask(hotkey)? != 0)
    }

    fn listen(&mut self, tape: Tape) -> Result<Box<dyn Recording>, String> {
        Microphone::start(tape).map(|microphone| Box::new(microphone) as Box<dyn Recording>)
    }

    fn surroundings(&self, read_text: bool) -> Surroundings {
        autoreleasepool(|_| {
            let app = NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .and_then(|app| app.localizedName())
                .map(|name| name.to_string())
                .unwrap_or_default();
            let focus = Focus::now();
            let visible = focus.as_ref().and_then(Focus::window_title).unwrap_or_default();
            let (before, after) = match focus.as_ref().filter(|_| read_text).and_then(Focus::around_cursor) {
                Some((before, after)) => (Some(before), Some(after)),
                None => (None, None),
            };
            Surroundings { app, before, after, visible }
        })
    }

    fn type_text(&self, text: &str, reread: bool) -> Result<Typed, String> {
        let focus = Focus::now();
        if let Some(focus) = focus.as_ref().filter(|focus| !focus.editable()) {
            let role = string_attribute(focus.element.0, "AXRole").unwrap_or_default();
            // Nothing to type into: leave the words on the clipboard instead.
            autoreleasepool(|_| {
                let pasteboard = NSPasteboard::generalPasteboard();
                pasteboard.clearContents();
                pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
            });
            return Err(format!("no text field ({role})"));
        }
        if focus.is_none() {
            crate::log::note("no focused element: typing anyway");
        }
        keystrokes(text);
        let reread = focus.filter(|_| reread).map(|focus| {
            let read: Box<dyn FnMut() -> Option<String> + Send> = Box::new(move || focus.value());
            read
        });
        Ok(Typed { reread })
    }

    fn cue(&self, cue: Cue) {
        autoreleasepool(|_| {
            let name = match cue {
                Cue::Start => "Tink",
                Cue::Stop => "Pop",
            };
            if let Some(sound) = NSSound::soundNamed(&NSString::from_str(name)) {
                sound.setVolume(CUE_VOLUME);
                sound.play();
            }
        });
    }

    fn problem(&self, hotkey: Hotkey) -> Option<Problem> {
        if !unsafe { AXIsProcessTrusted() } {
            return Some(accessibility());
        }
        if microphone_denied() {
            return Some(Problem {
                says: "Allow notchbase to use the microphone".into(),
                fix: Some(MICROPHONE.into()),
                button: "Open",
                blocks: true,
            });
        }
        if hotkey == Hotkey::Fn && globe_does_something() {
            return Some(Problem {
                says: "Set \"Press 🌐 key to\" to Do Nothing, so fn only dictates".into(),
                fix: Some(KEYBOARD.into()),
                button: "Fix",
                blocks: false,
            });
        }
        None
    }
}

fn accessibility() -> Problem {
    Problem {
        says: "Allow notchbase in Accessibility to dictate".into(),
        fix: Some(ACCESSIBILITY.into()),
        button: "Open",
        blocks: true,
    }
}

fn mask(hotkey: Hotkey) -> Option<u64> {
    match hotkey {
        Hotkey::Fn => Some(SECONDARY_FN),
        Hotkey::RightOption => Some(RIGHT_OPTION),
        Hotkey::RightCommand => Some(RIGHT_COMMAND),
        _ => None,
    }
}

fn code(hotkey: Hotkey) -> Option<i64> {
    match hotkey {
        Hotkey::Fn => Some(FN_CODE),
        Hotkey::RightOption => Some(RIGHT_OPTION_CODE),
        Hotkey::RightCommand => Some(RIGHT_COMMAND_CODE),
        _ => None,
    }
}

/// A CoreFoundation object this side owns one reference to.
struct Owned(Ref);

unsafe impl Send for Owned {}

impl Owned {
    fn new(object: Ref) -> Option<Self> {
        // `then`, not `then_some`: an `Owned` built eagerly around NULL would be
        // dropped on the spot and CFRelease(NULL) kills the process.
        (!object.is_null()).then(|| Self(object))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn cf_string(text: &str) -> Option<Owned> {
    Owned::new(unsafe { CFStringCreateWithBytes(null_mut(), text.as_ptr(), text.len() as isize, UTF8, false) })
}

fn is_string(object: Ref) -> bool {
    !object.is_null() && unsafe { CFGetTypeID(object) == CFStringGetTypeID() }
}

/// Part of a CFString, by UTF-16 units.
fn string_part(string: Ref, from: isize, to: isize) -> String {
    let length = unsafe { CFStringGetLength(string) };
    let (from, to) = (from.clamp(0, length), to.clamp(0, length));
    if to <= from {
        return String::new();
    }
    let mut units = vec![0u16; (to - from) as usize];
    unsafe { CFStringGetCharacters(string, CFRange { location: from, length: to - from }, units.as_mut_ptr()) };
    String::from_utf16_lossy(&units)
}

fn attribute(element: Ref, name: &str) -> Option<Owned> {
    let name = cf_string(name)?;
    let mut value = null_mut();
    let status = unsafe { AXUIElementCopyAttributeValue(element, name.0, &mut value) };
    if status == 0 { Owned::new(value) } else { None }
}

fn string_attribute(element: Ref, name: &str) -> Option<String> {
    let value = attribute(element, name).filter(|value| is_string(value.0))?;
    Some(string_part(value.0, 0, isize::MAX))
}

/// The focused element of the frontmost app, through Accessibility.
struct Focus {
    element: Owned,
}

impl Focus {
    fn now() -> Option<Self> {
        let system = Owned::new(unsafe { AXUIElementCreateSystemWide() })?;
        unsafe { AXUIElementSetMessagingTimeout(system.0, AX_TIMEOUT) };
        let element = attribute(system.0, "AXFocusedUIElement")?;
        unsafe { AXUIElementSetMessagingTimeout(element.0, AX_TIMEOUT) };
        Some(Self { element })
    }

    fn editable(&self) -> bool {
        let role = string_attribute(self.element.0, "AXRole").unwrap_or_default();
        if EDITABLE_ROLES.contains(&role.as_str()) {
            return true;
        }
        let Some(name) = cf_string("AXValue") else { return true };
        let mut settable = false;
        let status = unsafe { AXUIElementIsAttributeSettable(self.element.0, name.0, &mut settable) };
        // An app that doesn't answer may still take a paste.
        status != 0 || settable || role.is_empty()
    }

    fn window_title(&self) -> Option<String> {
        let window = attribute(self.element.0, "AXWindow")?;
        string_attribute(window.0, "AXTitle")
    }

    fn value(&self) -> Option<String> {
        string_attribute(self.element.0, "AXValue")
    }

    /// The text before and after the cursor (or the selection, which the words replace).
    fn around_cursor(&self) -> Option<(String, String)> {
        let value = attribute(self.element.0, "AXValue").filter(|value| is_string(value.0))?;
        let range = attribute(self.element.0, "AXSelectedTextRange")?;
        let mut selected = CFRange::default();
        if !unsafe { AXValueGetValue(range.0, RANGE_TYPE, (&raw mut selected).cast()) } {
            return None;
        }
        let end = selected.location + selected.length;
        let before = string_part(value.0, selected.location - BEFORE_CHARS, selected.location);
        let after = string_part(value.0, end, end + AFTER_CHARS);
        Some((before, after))
    }
}

/// Types `text` as keystrokes that carry the characters themselves, so it works whatever the
/// keyboard layout, needs no Paste menu, and leaves the clipboard alone. A line break is Return.
fn keystrokes(text: &str) {
    unsafe {
        let source = CGEventSourceCreate(PRIVATE_STATE);
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                key(source, RETURN_CODE, &[]);
            }
            let units: Vec<u16> = line.encode_utf16().collect();
            let mut from = 0;
            while from < units.len() {
                let mut to = (from + CHUNK).min(units.len());
                // Never split a surrogate pair between two keystrokes.
                if to < units.len() && (0xDC00..0xE000).contains(&units[to]) {
                    to -= 1;
                }
                key(source, 0, &units[from..to]);
                from = to;
            }
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
}

/// One key pressed and let go, with no modifiers, typing `units` if there are any.
unsafe fn key(source: Ref, code: u16, units: &[u16]) {
    for down in [true, false] {
        let event = unsafe { CGEventCreateKeyboardEvent(source, code, down) };
        if event.is_null() {
            continue;
        }
        unsafe {
            CGEventSetFlags(event, 0);
            if !units.is_empty() {
                CGEventKeyboardSetUnicodeString(event, units.len(), units.as_ptr());
            }
            CGEventPost(SESSION_TAP, event);
            CFRelease(event);
        }
    }
    // Some apps drop keystrokes that come all at once.
    thread::sleep(Duration::from_millis(2));
}

fn microphone_denied() -> bool {
    autoreleasepool(|_| {
        let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return false };
        let audio = NSString::from_str("soun");
        let status: isize = unsafe { msg_send![class, authorizationStatusForMediaType: &*audio] };
        // Restricted or denied.
        status == 1 || status == 2
    })
}

/// Whether the Globe key does something of its own when pressed alone. Unset means the system's
/// default, which shows the emoji picker or switches the input source.
fn globe_does_something() -> bool {
    let (Some(key), Some(app)) = (cf_string("AppleFnUsageType"), cf_string("com.apple.HIToolbox")) else {
        return false;
    };
    let Some(value) = Owned::new(unsafe { CFPreferencesCopyAppValue(key.0, app.0) }) else { return true };
    if unsafe { CFGetTypeID(value.0) } != unsafe { CFNumberGetTypeID() } {
        return false;
    }
    let mut usage: i32 = 0;
    unsafe { CFNumberGetValue(value.0, NUMBER_INT, (&raw mut usage).cast()) };
    usage != 0
}

struct Watched {
    hotkey: Hotkey,
    keys: KeySink,
    listening: Arc<AtomicBool>,
    held: AtomicBool,
    port: AtomicPtr<c_void>,
}

struct Loop(Ref);

unsafe impl Send for Loop {}

struct KeyTap {
    run_loop: Loop,
    join: Option<JoinHandle<()>>,
}

impl KeyTap {
    fn install(hotkey: Hotkey, keys: KeySink, listening: Arc<AtomicBool>) -> Option<Self> {
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let watched =
            Watched { hotkey, keys, listening, held: AtomicBool::new(false), port: AtomicPtr::new(null_mut()) };
        let join = thread::Builder::new()
            .name("notchbase dictation key".into())
            .spawn(move || tap_thread(watched, &ready_tx))
            .ok()?;
        match ready.recv() {
            Ok(Some(run_loop)) => Some(Self { run_loop, join: Some(join) }),
            _ => {
                let _ = join.join();
                None
            }
        }
    }
}

impl Drop for KeyTap {
    fn drop(&mut self) {
        unsafe { CFRunLoopStop(self.run_loop.0) };
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn tap_thread(watched: Watched, ready: &SyncSender<Option<Loop>>) {
    let watched = Box::new(watched);
    let user = (&raw const *watched).cast_mut().cast::<c_void>();
    let events = (1 << KEY_DOWN) | (1 << FLAGS_CHANGED);
    let port = unsafe { CGEventTapCreate(HID_TAP, HEAD, ACTIVE, events, tapped, user) };
    if port.is_null() {
        let _ = ready.send(None);
        return;
    }
    watched.port.store(port, Ordering::Release);
    let source = unsafe { CFMachPortCreateRunLoopSource(null_mut(), port, 0) };
    if source.is_null() {
        unsafe { CFRelease(port) };
        let _ = ready.send(None);
        return;
    }
    let run_loop = unsafe { CFRunLoopGetCurrent() };
    unsafe {
        CFRetain(run_loop);
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
        CGEventTapEnable(port, true);
    }
    let _ = ready.send(Some(Loop(run_loop)));
    unsafe { CFRunLoopRun() };
    watched.port.store(null_mut(), Ordering::Release);
    unsafe {
        CGEventTapEnable(port, false);
        CFMachPortInvalidate(port);
        CFRelease(source);
        CFRelease(port);
        CFRelease(run_loop);
    }
    if watched.held.load(Ordering::Relaxed) {
        (watched.keys)(Key::Up);
    }
    drop(watched);
}

extern "C" fn tapped(_proxy: Ref, kind: u32, event: Ref, user: *mut c_void) -> Ref {
    let Some(watched) = (unsafe { user.cast::<Watched>().as_ref() }) else { return event };
    if kind == TAP_DISABLED_BY_TIMEOUT || kind == TAP_DISABLED_BY_INPUT {
        let port = watched.port.load(Ordering::Acquire);
        if !port.is_null() {
            unsafe { CGEventTapEnable(port, true) };
        }
        return event;
    }
    let keycode = unsafe { CGEventGetIntegerValueField(event, KEYCODE) };
    match kind {
        FLAGS_CHANGED => {
            let (Some(code), Some(mask)) = (code(watched.hotkey), mask(watched.hotkey)) else { return event };
            if keycode != code {
                return event;
            }
            let down = unsafe { CGEventGetFlags(event) } & mask != 0;
            if watched.held.swap(down, Ordering::Relaxed) != down {
                (watched.keys)(if down { Key::Down } else { Key::Up });
            }
            // fn on its own is dictation's: the system never sees it, so a press or double press
            // can't open the emoji picker, switch the input source or call up Siri on the way.
            // Keys pressed with fn still carry it.
            if watched.hotkey == Hotkey::Fn { null_mut() } else { event }
        }
        KEY_DOWN => {
            if unsafe { CGEventGetIntegerValueField(event, AUTOREPEAT) } != 0 {
                return event;
            }
            if keycode == ESCAPE_CODE && watched.listening.load(Ordering::Relaxed) {
                (watched.keys)(Key::Escape);
                return null_mut();
            }
            if watched.held.load(Ordering::Relaxed) {
                (watched.keys)(Key::Other);
            }
            event
        }
        _ => event,
    }
}

/// What the AudioQueue's callback writes into.
struct Sink {
    tape: Tape,
    converter: Option<Mutex<Converter>>,
    running: AtomicBool,
}

struct Microphone {
    queue: Ref,
    sink: Box<Sink>,
}

unsafe impl Send for Microphone {}

impl Recording for Microphone {}

impl Microphone {
    fn start(tape: Tape) -> Result<Self, String> {
        let mut last = 0;
        for rate in RATES {
            match Self::open(tape.clone(), rate) {
                Ok(microphone) => return Ok(microphone),
                Err(status) => last = status,
            }
        }
        Err(format!("Couldn't start the microphone ({last})"))
    }

    fn open(tape: Tape, rate: u32) -> Result<Self, i32> {
        let format = AudioStreamBasicDescription {
            sample_rate: f64::from(rate),
            format_id: LINEAR_PCM,
            format_flags: SIGNED_PACKED,
            bytes_per_packet: 2,
            frames_per_packet: 1,
            bytes_per_frame: 2,
            channels_per_frame: 1,
            bits_per_channel: 16,
            reserved: 0,
        };
        let converter = (rate as usize != RATE).then(|| Mutex::new(Converter::new(rate, 1)));
        let sink = Box::new(Sink { tape, converter, running: AtomicBool::new(true) });
        let user = (&raw const *sink).cast_mut().cast::<c_void>();
        let mut queue = null_mut();
        let status = unsafe { AudioQueueNewInput(&format, heard, user, null_mut(), null_mut(), 0, &mut queue) };
        if status != 0 || queue.is_null() {
            return Err(status);
        }
        let microphone = Self { queue, sink };
        let bytes = rate * BUFFER_MS / 1000 * 2;
        for _ in 0..BUFFERS {
            let mut buffer = null_mut();
            let status = unsafe { AudioQueueAllocateBuffer(queue, bytes, &mut buffer) };
            if status != 0 || buffer.is_null() {
                return Err(status);
            }
            unsafe { AudioQueueEnqueueBuffer(queue, buffer, 0, null()) };
        }
        let status = unsafe { AudioQueueStart(queue, null()) };
        if status != 0 {
            return Err(status);
        }
        Ok(microphone)
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        self.sink.running.store(false, Ordering::Release);
        unsafe {
            AudioQueueStop(self.queue, true);
            AudioQueueDispose(self.queue, true);
        }
    }
}

extern "C" fn heard(
    user: *mut c_void,
    queue: Ref,
    buffer: *mut AudioQueueBuffer,
    _start: *const c_void,
    _count: u32,
    _descriptions: *const c_void,
) {
    let (Some(sink), Some(filled)) = (unsafe { user.cast::<Sink>().as_ref() }, unsafe { buffer.as_ref() }) else {
        return;
    };
    if !filled.data.is_null() && filled.size >= 2 {
        let samples = unsafe { std::slice::from_raw_parts(filled.data.cast::<i16>(), filled.size as usize / 2) };
        match &sink.converter {
            Some(converter) => {
                let mut out = Vec::with_capacity(samples.len() / 2);
                converter.lock().unwrap_or_else(PoisonError::into_inner).push(samples, &mut out);
                sink.tape.push(&out);
                sink.tape.set_level(level(&out));
            }
            None => {
                sink.tape.push(samples);
                sink.tape.set_level(level(samples));
            }
        }
    }
    if sink.running.load(Ordering::Acquire) {
        unsafe { AudioQueueEnqueueBuffer(queue, buffer, 0, null()) };
    }
}
