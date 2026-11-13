//! Keeps the notch on the lock screen by moving its window into a space above it, using private SkyLight calls.

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock, PoisonError};

use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSNumber};
use winit::window::Window;

use super::appkit::ns_window;

const FRAMEWORK: &CStr = c"/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/SkyLight";
const ABOVE_THE_LOCK_SCREEN: i32 = 400;
const MOVE_FROM_EVERY_SPACE: i32 = 7;
const EVERY_SPACE: i32 = 7;

type MainConnection = unsafe extern "C" fn() -> i32;
type SpaceCreate = unsafe extern "C" fn(i32, i32, i32) -> u64;
type SpaceSetLevel = unsafe extern "C" fn(i32, u64, i32) -> i32;
type ChangeSpaces = unsafe extern "C" fn(i32, *const c_void) -> i32;
type SpaceDestroy = unsafe extern "C" fn(i32, u64);
type MoveWindows = unsafe extern "C" fn(i32, u64, *const c_void, i32) -> i32;
type MoveHome = unsafe extern "C" fn(i32, *const c_void, u64);
type ActiveSpace = unsafe extern "C" fn(i32) -> u64;
type SpacesOf = unsafe extern "C" fn(i32, i32, *const c_void) -> *mut NSArray<NSNumber>;

static LAYER: Mutex<Layer> = Mutex::new(Layer { space: None, homes: Vec::new() });

struct Layer {
    space: Option<u64>,
    homes: Vec<(isize, u64)>,
}

struct SkyLight {
    connection: i32,
    space_create: SpaceCreate,
    set_level: SpaceSetLevel,
    show_spaces: ChangeSpaces,
    hide_spaces: ChangeSpaces,
    space_destroy: SpaceDestroy,
    move_windows: MoveWindows,
    move_home: MoveHome,
    active_space: ActiveSpace,
    spaces_of: SpacesOf,
}

pub fn show_on_lock_screen(window: &Window) -> bool {
    let (Some(sky), Some(ns_window)) = (skylight(), ns_window(window)) else { return false };
    let mut layer = layer();
    let Some(space) = layer.space.or_else(|| sky.raise()) else { return false };
    layer.space = Some(space);
    let (number, windows) = (ns_window.windowNumber(), list(ns_window.windowNumber() as u64));
    let home = sky.home_of(&windows, space);
    unsafe { (sky.move_windows)(sky.connection, space, as_ptr(&windows), MOVE_FROM_EVERY_SPACE) };
    layer.homes.retain(|(window, _)| *window != number);
    layer.homes.push((number, home));
    true
}

pub fn leave_lock_screen(window: &Window) {
    let (Some(sky), Some(ns_window)) = (skylight(), ns_window(window)) else { return };
    let mut layer = layer();
    let number = ns_window.windowNumber();
    if let Some(at) = layer.homes.iter().position(|(window, _)| *window == number) {
        let (_, home) = layer.homes.swap_remove(at);
        unsafe { (sky.move_home)(sky.connection, as_ptr(&list(number as u64)), home) };
    }
    if layer.homes.is_empty()
        && let Some(space) = layer.space.take()
    {
        sky.lower(space);
    }
}

impl SkyLight {
    fn raise(&self) -> Option<u64> {
        unsafe {
            let space = (self.space_create)(self.connection, 1, 0);
            if space == 0 {
                return None;
            }
            (self.set_level)(self.connection, space, ABOVE_THE_LOCK_SCREEN);
            (self.show_spaces)(self.connection, as_ptr(&list(space)));
            Some(space)
        }
    }

    fn lower(&self, space: u64) {
        unsafe {
            (self.hide_spaces)(self.connection, as_ptr(&list(space)));
            (self.space_destroy)(self.connection, space);
        }
    }

    fn home_of(&self, windows: &NSArray<NSNumber>, layer: u64) -> u64 {
        let spaces = unsafe { Retained::from_raw((self.spaces_of)(self.connection, EVERY_SPACE, as_ptr(windows))) };
        let home = spaces.and_then(|spaces| spaces.firstObject()).map(|space| space.unsignedLongLongValue());
        home.filter(|space| *space != layer).unwrap_or_else(|| unsafe { (self.active_space)(self.connection) })
    }
}

fn list(number: u64) -> Retained<NSArray<NSNumber>> {
    NSArray::from_retained_slice(&[NSNumber::new_u64(number)])
}

fn as_ptr(list: &NSArray<NSNumber>) -> *const c_void {
    std::ptr::from_ref(list).cast()
}

fn layer() -> std::sync::MutexGuard<'static, Layer> {
    LAYER.lock().unwrap_or_else(PoisonError::into_inner)
}

fn skylight() -> Option<&'static SkyLight> {
    static SKYLIGHT: OnceLock<Option<SkyLight>> = OnceLock::new();
    SKYLIGHT.get_or_init(open).as_ref()
}

fn open() -> Option<SkyLight> {
    let library = NonNull::new(unsafe { libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_NOW) })?;
    let symbol = |name: &CStr| Some(NonNull::new(unsafe { libc::dlsym(library.as_ptr(), name.as_ptr()) })?.as_ptr());
    let main_connection = symbol(c"SLSMainConnectionID")?;
    let space_create = symbol(c"SLSSpaceCreate")?;
    let set_level = symbol(c"SLSSpaceSetAbsoluteLevel")?;
    let show_spaces = symbol(c"SLSShowSpaces")?;
    let hide_spaces = symbol(c"SLSHideSpaces")?;
    let space_destroy = symbol(c"SLSSpaceDestroy")?;
    let move_windows = symbol(c"SLSSpaceAddWindowsAndRemoveFromSpaces")?;
    let move_home = symbol(c"SLSMoveWindowsToManagedSpace")?;
    let active_space = symbol(c"SLSGetActiveSpace")?;
    let spaces_of = symbol(c"SLSCopySpacesForWindows")?;
    unsafe {
        let main_connection = std::mem::transmute::<*mut c_void, MainConnection>(main_connection);
        Some(SkyLight {
            connection: main_connection(),
            space_create: std::mem::transmute::<*mut c_void, SpaceCreate>(space_create),
            set_level: std::mem::transmute::<*mut c_void, SpaceSetLevel>(set_level),
            show_spaces: std::mem::transmute::<*mut c_void, ChangeSpaces>(show_spaces),
            hide_spaces: std::mem::transmute::<*mut c_void, ChangeSpaces>(hide_spaces),
            space_destroy: std::mem::transmute::<*mut c_void, SpaceDestroy>(space_destroy),
            move_windows: std::mem::transmute::<*mut c_void, MoveWindows>(move_windows),
            move_home: std::mem::transmute::<*mut c_void, MoveHome>(move_home),
            active_space: std::mem::transmute::<*mut c_void, ActiveSpace>(active_space),
            spaces_of: std::mem::transmute::<*mut c_void, SpacesOf>(spaces_of),
        })
    }
}
