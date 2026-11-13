//! Everything the dock asks of AppKit: running apps and their changes, icons, opening and quitting, the Trash,
//! the pointer, the screen, the dock window itself and the frame of an app's front window.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread as _, MainThreadMarker};
use objc2_app_kit::{
    NSApplicationActivationPolicy, NSColor, NSColorSpace, NSEvent, NSEventMask, NSImage, NSRunningApplication,
    NSScreen, NSView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWorkspace,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceDidHideApplicationNotification,
    NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidTerminateApplicationNotification,
    NSWorkspaceDidUnhideApplicationNotification,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSFileManager, NSNotification, NSNumber, NSPoint, NSRect, NSSize, NSString, NSURL,
    NSUserDefaults, ns_string,
};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::window::Window;

/// One above kCGDockWindowLevel (20), where the system Dock lives: in Mission Control the system Dock draws a
/// full-screen window at 20, and this one stays in front of it, covering the tiny system Dock.
pub const DOCK_LEVEL: isize = 21;
/// Just under the dock, above every app window: the launch effect draws here.
pub const EFFECT_LEVEL: isize = 19;

#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub pid: i32,
    pub path: Option<PathBuf>,
    pub name: String,
    pub active: bool,
    pub hidden: bool,
}

/// Apps that show in the Dock (regular activation policy), in launch order, without this one.
pub fn running_apps() -> Vec<Running> {
    autoreleasepool(|_| {
        let me = std::process::id() as i32;
        let apps = NSWorkspace::sharedWorkspace().runningApplications();
        apps.iter()
            .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .filter(|app| app.processIdentifier() != me)
            .map(|app| Running {
                pid: app.processIdentifier(),
                path: app.bundleURL().and_then(|url| url_path(&url)),
                name: app.localizedName().map(|n| n.to_string()).unwrap_or_default(),
                active: app.isActive(),
                hidden: app.isHidden(),
            })
            .collect()
    })
}

/// Calls `changed` whenever an app launches, quits, activates, hides or shows again. Keep the returned
/// observers alive for as long as the calls should come.
pub fn watch_apps(changed: impl Fn() + 'static) -> Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>> {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let block = RcBlock::new(move |_: NonNull<NSNotification>| changed());
    let names = unsafe {
        [
            NSWorkspaceDidLaunchApplicationNotification,
            NSWorkspaceDidTerminateApplicationNotification,
            NSWorkspaceDidActivateApplicationNotification,
            NSWorkspaceDidHideApplicationNotification,
            NSWorkspaceDidUnhideApplicationNotification,
        ]
    };
    names
        .into_iter()
        .map(|name| unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) })
        .collect()
}

pub fn unwatch(observers: &[Retained<ProtocolObject<dyn NSObjectProtocol>>]) {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    for observer in observers {
        let object: &AnyObject = observer.as_ref();
        unsafe { center.removeObserver(object) };
    }
}

fn url_path(url: &NSURL) -> Option<PathBuf> {
    url.path().map(|p| PathBuf::from(p.to_string()))
}

fn ns_path(path: &Path) -> Option<Retained<NSString>> {
    path.to_str().map(NSString::from_str)
}

/// The name Finder shows for a file, without the ".app".
pub fn display_name(path: &Path) -> String {
    let fallback =
        || path.file_stem().or(path.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(ns) = ns_path(path) else { return fallback() };
    let name = NSFileManager::defaultManager().displayNameAtPath(&ns).to_string();
    match name.strip_suffix(".app") {
        Some(stem) => stem.to_owned(),
        None if name.is_empty() => fallback(),
        None => name,
    }
}

/// The Finder icon of a file, drawn into a square of `px` pixels.
pub fn icon_for_path(path: &Path, px: u32) -> Option<egui::ColorImage> {
    // A link (such as /Applications/Safari.app, which points into the system's cryptex) would get Finder's alias arrow.
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    autoreleasepool(|_| {
        let ns = ns_path(&real)?;
        let image = NSWorkspace::sharedWorkspace().iconForFile(&ns);
        rasterize(&image, px)
    })
}

/// The icon a running app shows, which is right even for apps outside a bundle.
pub fn icon_for_pid(pid: i32, px: u32) -> Option<egui::ColorImage> {
    autoreleasepool(|_| {
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        rasterize(&*app.icon()?, px)
    })
}

pub fn icon_from_file(path: &Path, px: u32) -> Option<egui::ColorImage> {
    autoreleasepool(|_| {
        let ns = ns_path(path)?;
        let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &ns)?;
        rasterize(&image, px)
    })
}

fn rasterize(image: &NSImage, px: u32) -> Option<egui::ColorImage> {
    let side = px as CGFloat;
    let mut wanted = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
    let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut wanted, None, None) }?;
    draw_square(&cg, px as usize)
}

fn draw_square(image: &CGImage, side: usize) -> Option<egui::ColorImage> {
    const RGBA: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    if side == 0 {
        return None;
    }
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let mut rgba = vec![0u8; side * side * 4];
    let context =
        unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), side, side, 8, side * 4, Some(&space), RGBA) }?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let (w, h) = (CGImage::width(Some(image)) as CGFloat, CGImage::height(Some(image)) as CGFloat);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    // Fit, keeping the aspect: icons are square, folder previews may not be.
    let s = side as CGFloat;
    let k = (s / w).min(s / h);
    let (dw, dh) = (w * k, h * k);
    let rect = CGRect::new(CGPoint::new((s - dw) * 0.5, (s - dh) * 0.5), CGSize::new(dw, dh));
    CGContext::draw_image(Some(&context), rect, Some(image));
    drop(context);
    Some(egui::ColorImage::from_rgba_premultiplied([side, side], &rgba))
}

fn open_command(args: &[&std::ffi::OsStr]) {
    let spawned = Command::new("/usr/bin/open")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        // Reap it off the main thread so no zombie is left behind.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(err) => eprintln!("notchbase-dock: open failed: {err}"),
    }
}

/// Opens a file, a folder or an app. For a running app this is what the Dock does on a click: it comes forward
/// and opens a new window when it has none.
pub fn open(path: &Path) {
    open_command(&[path.as_os_str()]);
}

pub fn open_with(app: &Path, files: &[PathBuf]) {
    let mut args: Vec<&std::ffi::OsStr> = vec!["-a".as_ref(), app.as_os_str()];
    args.extend(files.iter().map(|f| f.as_os_str()));
    open_command(&args);
}

pub fn reveal(path: &Path) {
    open_command(&["-R".as_ref(), path.as_os_str()]);
}

pub fn quit(pid: i32) {
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.terminate();
    }
}

/// Brings an app to the front.
pub fn activate(pid: i32) {
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        #[allow(deprecated, reason = "the only way an accessory app can hand focus to another one")]
        app.activateWithOptions(objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
}

/// Quits an app without asking it, for apps that have stopped answering.
pub fn force_quit(pid: i32) {
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.forceTerminate();
    }
}

/// Whether Option is held down right now.
pub fn option_held() -> bool {
    NSEvent::modifierFlags_class().contains(objc2_app_kit::NSEventModifierFlags::Option)
}

/// Brings an app forward and spreads out all its windows (App Exposé), as the Dock's Show All Windows does.
pub fn app_expose(pid: i32) {
    activate(pid);
    std::thread::spawn(|| {
        // Once the app is in front, so Exposé shows its windows rather than ours.
        std::thread::sleep(std::time::Duration::from_millis(160));
        type Send = unsafe extern "C" fn(*const c_void, i32) -> i32;
        let f = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"CoreDockSendNotification".as_ptr()) };
        if f.is_null() {
            return;
        }
        let send = unsafe { std::mem::transmute::<*mut c_void, Send>(f) };
        let name = NSString::from_str("com.apple.expose.front.awake");
        unsafe { send(&*name as *const NSString as *const c_void, 0) };
    });
}

pub fn hide(pid: i32) {
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.hide();
    }
}

pub fn trash_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Trash"))
}

/// Moves a file to the Trash, like Finder does.
pub fn move_to_trash(path: &Path) -> bool {
    let Some(ns) = ns_path(path) else { return false };
    let url = NSURL::fileURLWithPath(&ns);
    NSFileManager::defaultManager().trashItemAtURL_resultingItemURL_error(&url, None).is_ok()
}

pub fn empty_trash() {
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", "tell application \"Finder\" to empty the trash"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut child| std::thread::spawn(move || child.wait()));
}

/// Whether the Trash holds anything. Reading ~/.Trash can need Full Disk Access, so the Dock's own note of it is
/// the fallback.
pub fn trash_is_full() -> bool {
    if let Some(dir) = trash_dir()
        && let Ok(mut entries) = std::fs::read_dir(dir)
    {
        return entries.any(|e| e.is_ok_and(|e| e.file_name() != ".DS_Store"));
    }
    dock_pref("trash-full").and_then(|v| v.downcast::<NSNumber>().ok()).is_some_and(|n| n.boolValue())
}

/// Whether the Mac is in Dark Mode right now. Read from the global domain on disk, which Auto appearance keeps
/// current, so a switch shows up without a restart.
pub fn dark_mode() -> bool {
    autoreleasepool(|_| {
        NSUserDefaults::standardUserDefaults()
            .persistentDomainForName(ns_string!("NSGlobalDomain"))
            .and_then(|domain| domain.objectForKey(ns_string!("AppleInterfaceStyle")))
            .and_then(|v| v.downcast::<NSString>().ok())
            .is_some_and(|style| style.to_string() == "Dark")
    })
}

fn dock_pref(key: &str) -> Option<Retained<AnyObject>> {
    let domain = NSUserDefaults::standardUserDefaults().persistentDomainForName(ns_string!("com.apple.dock"))?;
    domain.objectForKey(&NSString::from_str(key))
}

/// What the system Dock holds today: its pinned apps, the folders and files on its right, and its sizes.
pub struct DockPrefs {
    pub apps: Vec<PathBuf>,
    pub others: Vec<PathBuf>,
    pub tile_size: Option<f32>,
    pub large_size: Option<f32>,
    pub magnification: Option<bool>,
}

pub fn dock_prefs() -> Option<DockPrefs> {
    autoreleasepool(|_| {
        let domain = NSUserDefaults::standardUserDefaults().persistentDomainForName(ns_string!("com.apple.dock"))?;
        let tiles = |key: &str| -> Vec<PathBuf> {
            let Some(list) = domain.objectForKey(&NSString::from_str(key)) else { return Vec::new() };
            let Ok(list) = list.downcast::<NSArray>() else { return Vec::new() };
            list.iter().filter_map(|tile| tile_path(&tile)).collect()
        };
        let number = |key: &str| -> Option<Retained<NSNumber>> {
            domain.objectForKey(&NSString::from_str(key)).and_then(|v| v.downcast::<NSNumber>().ok())
        };
        Some(DockPrefs {
            apps: tiles("persistent-apps"),
            others: tiles("persistent-others"),
            tile_size: number("tilesize").map(|n| n.doubleValue() as f32),
            large_size: number("largesize").map(|n| n.doubleValue() as f32),
            magnification: number("magnification").map(|n| n.boolValue()),
        })
    })
}

fn tile_path(tile: &AnyObject) -> Option<PathBuf> {
    let tile = dict(tile)?;
    let data = tile.objectForKey(ns_string!("tile-data"))?;
    let data = dict(&data)?;
    let file = data.objectForKey(ns_string!("file-data"))?;
    let file = dict(&file)?;
    let url = file.objectForKey(ns_string!("_CFURLString"))?.downcast::<NSString>().ok()?;
    let url = NSURL::URLWithString(&url)?;
    url_path(&url)
}

fn dict(object: &AnyObject) -> Option<&NSDictionary<NSString, AnyObject>> {
    let is_dict: bool = unsafe { objc2::msg_send![object, isKindOfClass: objc2::class!(NSDictionary)] };
    is_dict.then(|| unsafe { &*(object as *const AnyObject as *const NSDictionary<NSString, AnyObject>) })
}

/// The pointer in global points with the origin at the top left of the primary screen.
pub fn pointer() -> egui::Pos2 {
    let at = NSEvent::mouseLocation();
    let top = primary_screen_frame().map_or(0.0, |f| f.origin.y + f.size.height);
    egui::pos2(at.x as f32, (top - at.y) as f32)
}

pub fn mouse_buttons_held() -> bool {
    NSEvent::pressedMouseButtons() != 0
}

/// The screen with the menu bar, where the Dock lives.
pub fn primary_screen_frame() -> Option<CGRect> {
    let mtm = MainThreadMarker::new()?;
    NSScreen::screens(mtm).firstObject().map(|s| s.frame())
}

/// A screen in global top-left points (y down from the main screen's top), as AX and Core Graphics place windows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    /// Its frame in AppKit's bottom-left points, as windows are placed.
    pub frame: CGRect,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    /// How far its menu bar (and notch) reaches down.
    pub menu: f64,
    /// Its Core Graphics display number (0 when unknown).
    pub id: u32,
}

/// Every screen, the main one first.
pub fn screens() -> Vec<Screen> {
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    let all = NSScreen::screens(mtm);
    let Some(main) = all.firstObject() else { return Vec::new() };
    let main_top = main.frame().origin.y + main.frame().size.height;
    all.iter().map(|s| Screen { id: display_id(&s), ..screen_from(s.frame(), s.visibleFrame(), main_top) }).collect()
}

/// A screen's Core Graphics display number.
fn display_id(screen: &NSScreen) -> u32 {
    let description = screen.deviceDescription();
    let key = NSString::from_str("NSScreenNumber");
    description.objectForKey(&key).and_then(|n| n.downcast::<NSNumber>().ok()).map_or(0, |n| n.unsignedIntValue())
}

/// A screen from AppKit's frames for it (bottom-left points, y up from the main screen's bottom).
pub fn screen_from(frame: CGRect, visible: CGRect, main_top: f64) -> Screen {
    let top = main_top - (frame.origin.y + frame.size.height);
    Screen {
        frame,
        left: frame.origin.x,
        top,
        right: frame.origin.x + frame.size.width,
        bottom: top + frame.size.height,
        menu: ((frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height)).max(0.0),
        id: 0,
    }
}

pub fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    MainThreadMarker::new()?;
    let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
}

/// A borderless window on every Space, never in the window cycle, at `level`, framed in Cocoa coordinates.
pub fn place_window(window: &Window, level: isize, frame: CGRect) {
    let Some(ns) = ns_window(window) else { return };
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::IgnoresCycle;
    if ns.collectionBehavior() != behavior {
        ns.setCollectionBehavior(behavior);
        ns.setHasShadow(false);
        ns.setOpaque(false);
        ns.setBackgroundColor(Some(&NSColor::clearColor()));
        ns.setHidesOnDeactivate(false);
        ns.setCanHide(false);
        ns.setMovable(false);
        ns.setAnimationBehavior(NSWindowAnimationBehavior::None);
        ns.setColorSpace(Some(&NSColorSpace::sRGBColorSpace()));
    }
    if ns.level() != level {
        ns.setLevel(level);
    }
    if ns.frame() != frame {
        ns.setFrame_display(frame, true);
    }
}

pub fn show_window(window: &Window, shown: bool) {
    let Some(ns) = ns_window(window) else { return };
    if shown {
        ns.orderFrontRegardless();
    } else {
        ns.orderOut(None);
    }
}

/// Hears presses that land outside this app, so an open menu can close.
pub struct PressesElsewhere(Retained<AnyObject>);

impl PressesElsewhere {
    pub fn new(pressed: impl Fn() + 'static) -> Option<Self> {
        let handler = RcBlock::new(move |_: NonNull<NSEvent>| pressed());
        let mask = NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
        NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler).map(Self)
    }
}

impl Drop for PressesElsewhere {
    fn drop(&mut self) {
        unsafe { NSEvent::removeMonitor(&self.0) };
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
    fn CGWindowListCreate(option: u32, relative_to: u32) -> *const c_void;
    fn CGWindowListCreateDescriptionFromArray(windows: *const c_void) -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFArrayCreate(
        alloc: *const c_void,
        values: *const *const c_void,
        count: isize,
        callbacks: *const c_void,
    ) -> *const c_void;
}

const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;

/// The frame of the frontmost normal window `pid` shows, in global top-left points. Needs no permission: only
/// window names are private.
pub fn front_window_frame(pid: i32) -> Option<egui::Rect> {
    autoreleasepool(|_| {
        let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
        if raw.is_null() {
            return None;
        }
        // CFArray and NSArray are toll-free bridged; the list is front to back.
        let list: &NSArray<AnyObject> = unsafe { &*(raw as *const NSArray<AnyObject>) };
        let found = list.iter().find_map(|entry| {
            let entry = dict(&entry)?;
            let int = |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.intValue());
            if int(ns_string!("kCGWindowOwnerPID"))? != pid || int(ns_string!("kCGWindowLayer"))? != 0 {
                return None;
            }
            // Windows still invisible (alpha 0) haven't shown yet.
            let alpha = entry.objectForKey(ns_string!("kCGWindowAlpha"))?.downcast::<NSNumber>().ok()?.doubleValue();
            if alpha < 0.05 {
                return None;
            }
            let bounds = entry.objectForKey(ns_string!("kCGWindowBounds"))?;
            let bounds = dict(&bounds)?;
            let num =
                |key: &NSString| bounds.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.doubleValue() as f32);
            let (x, y) = (num(ns_string!("X"))?, num(ns_string!("Y"))?);
            let (w, h) = (num(ns_string!("Width"))?, num(ns_string!("Height"))?);
            (w > 80.0 && h > 60.0).then(|| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)))
        });
        unsafe { CFRelease(raw) };
        found
    })
}

/// Watches for the system Dock putting a window on screen: Mission Control, App Exposé, Launchpad, or the Dock itself
/// shown over a full-screen app. Only the window ids are listed on each check, which is cheap; a window's owner is
/// looked up once, when it first shows.
#[derive(Default)]
pub struct SystemDockWatch {
    owners: std::collections::HashMap<u32, i32>,
    dock_pid: Option<i32>,
    /// The system Dock's icons on the last check that found it showing, in global top-left points; none without
    /// Accessibility.
    pub frame: Option<[f64; 4]>,
}

impl SystemDockWatch {
    pub fn showing(&mut self) -> bool {
        self.frame = None;
        let pid = match self.dock_pid {
            Some(pid) if unsafe { libc::kill(pid, 0) } == 0 => pid,
            _ => {
                let Some(pid) = running_pid("com.apple.dock") else { return false };
                self.owners.clear();
                self.dock_pid = Some(pid);
                pid
            }
        };
        let ids = unsafe { CGWindowListCreate(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
        if ids.is_null() {
            return false;
        }
        let count = unsafe { CFArrayGetCount(ids) };
        let on_screen: Vec<u32> =
            (0..count).map(|i| unsafe { CFArrayGetValueAtIndex(ids, i) } as usize as u32).collect();
        unsafe { CFRelease(ids) };
        let new: Vec<*const c_void> = on_screen
            .iter()
            .filter(|id| !self.owners.contains_key(id))
            .map(|&id| id as usize as *const c_void)
            .collect();
        if !new.is_empty() {
            autoreleasepool(|_| {
                let array =
                    unsafe { CFArrayCreate(std::ptr::null(), new.as_ptr(), new.len() as isize, std::ptr::null()) };
                if array.is_null() {
                    return;
                }
                let info = unsafe { CGWindowListCreateDescriptionFromArray(array) };
                unsafe { CFRelease(array) };
                if info.is_null() {
                    return;
                }
                let list: &NSArray<AnyObject> = unsafe { &*(info as *const NSArray<AnyObject>) };
                for entry in list.iter() {
                    let Some(entry) = dict(&entry) else { continue };
                    let int =
                        |key: &NSString| entry.objectForKey(key)?.downcast::<NSNumber>().ok().map(|n| n.intValue());
                    if let (Some(id), Some(owner)) =
                        (int(ns_string!("kCGWindowNumber")), int(ns_string!("kCGWindowOwnerPID")))
                    {
                        self.owners.insert(id as u32, owner);
                    }
                }
                unsafe { CFRelease(info) };
            });
        }
        self.owners.retain(|id, _| on_screen.contains(id));
        if !on_screen.iter().any(|id| self.owners.get(id) == Some(&pid)) {
            return false;
        }
        // A restarted Dock keeps an empty full-screen window up, just like Mission Control's. Its icons tell them
        // apart: they have a size only while they're actually showing. Without Accessibility, trust the window.
        if !crate::titlebar::trusted() {
            return true;
        }
        self.frame = crate::titlebar::dock_list_frame(pid).filter(|f| f[2] > 0.0 && f[3] > 0.0);
        self.frame.is_some()
    }
}

/// The pid of a running app by bundle id.
pub fn running_pid(bundle_id: &str) -> Option<i32> {
    let apps =
        objc2_app_kit::NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    apps.iter().next().map(|app| app.processIdentifier())
}

/// The frontmost app's pid.
pub fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| app.processIdentifier())
}

/// Words in menus that name the file manager and the Trash, which differ on Windows.
pub const SHOW_IN_FILES: &str = "Show in Finder";
pub const OPEN_IN_FILES: &str = "Open in Finder";
pub const MOVE_TO_TRASH: &str = "Move to Trash";
pub const EMPTY_TRASH: &str = "Empty Trash";
pub const EMPTY_TRASH_CONFIRM: &str = "Click to Empty Trash for Good";
pub const TRASH_NAME: &str = "Trash";

/// Whether a file is an app the dock can keep: an `.app` bundle.
pub fn is_app(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "app")
}

/// The Trash's own icon, empty or full.
pub fn trash_icon(full: bool, px: u32) -> Option<egui::ColorImage> {
    let name = if full { "FullTrashIcon.icns" } else { "TrashIcon.icns" };
    icon_from_file(&PathBuf::from("/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources").join(name), px)
}

/// The home folder.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
