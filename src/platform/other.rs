//! Other systems have no notch, hooks or capture. These stand-ins let the app build and its tests run on Linux.

use std::path::{Path, PathBuf};

use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event_loop::{EventLoop, EventLoopBuilder};
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowAttributes, WindowLevel};

use super::{Capture, KnownDir, Placement};

pub const MODIFIER_LABELS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Super"];
pub const KEY_JOINER: &str = "+";
const NO_TITLE_BAR_OVER_THE_PICTURE: f32 = 0.0;

pub struct FrontApp;

pub struct HeldInstance;

pub fn hold_instance(_dir: &Path) -> Option<HeldInstance> {
    Some(HeldInstance)
}

pub fn focus_window(_pid: u32) -> bool {
    false
}

pub fn process_alive(_pid: u32) -> bool {
    true
}

pub fn setup_event_loop<T>(_builder: &mut EventLoopBuilder<T>) {}

pub fn let_hooks_hear_our_windows<T>(_event_loop: &EventLoop<T>) {}

pub fn pick_folder(_start: Option<PathBuf>, done: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    done(None);
}

pub fn perform_haptic(_feel: crate::platform::Feel) {}

pub fn window_origin(_window: &Window) -> Option<egui::Pos2> {
    None
}

pub fn show_on_lock_screen(_window: &Window) -> bool {
    false
}

pub fn leave_lock_screen(_window: &Window) {}

pub fn until_vblank() -> Option<std::time::Duration> {
    None
}

pub fn hardware_notch(_monitor: &MonitorHandle) -> Option<egui::Vec2> {
    None
}

pub fn notch_window_attributes(placement: &Placement, canvas: egui::Vec2) -> WindowAttributes {
    let monitor = &placement.monitor;
    let scale = monitor.scale_factor();
    let size = PhysicalSize::new((canvas.x as f64 * scale) as u32, (canvas.y as f64 * scale) as u32);
    let origin = monitor.position();
    let x = origin.x + (monitor.size().width as i32 - size.width as i32) / 2;
    base_attributes("notchbase")
        .with_active(false)
        .with_inner_size(size)
        .with_position(PhysicalPosition::new(x, origin.y))
}

pub fn configure_notch_window(_window: &Window, _placement: &Placement, _canvas: egui::Vec2) {}

pub fn overlay_window_attributes(monitor: &MonitorHandle) -> WindowAttributes {
    base_attributes("notchbase picker").with_inner_size(monitor.size()).with_position(monitor.position())
}

pub fn configure_overlay_window(_window: &Window, _monitor: &MonitorHandle) {}

pub fn dim_window_attributes(monitor: &MonitorHandle) -> WindowAttributes {
    overlay_window_attributes(monitor).with_title("notchbase recording")
}

pub fn configure_dim_window(_window: &Window, _monitor: &MonitorHandle) {}

pub fn annotate_window_attributes(size: egui::Vec2, least: egui::Vec2) -> WindowAttributes {
    WindowAttributes::default()
        .with_title("Annotate")
        .with_resizable(true)
        .with_visible(false)
        .with_inner_size(LogicalSize::new(f64::from(size.x), f64::from(size.y)))
        .with_min_inner_size(LogicalSize::new(f64::from(least.x), f64::from(least.y)))
}

pub fn configure_annotate_window(_window: &Window) {}

pub fn annotate_title_inset(_window: &Window) -> f32 {
    NO_TITLE_BAR_OVER_THE_PICTURE
}

pub fn keep_on_top(_window: &Window) {}

pub fn raise_window(_window: &Window) {}

pub fn set_click_through(window: &Window, click_through: bool) {
    let _ = window.set_cursor_hittest(!click_through);
}

pub fn begin_keyboard_focus(window: &Window) {
    window.focus_window();
}

pub fn end_keyboard_focus(_window: &Window) {}

pub fn held_modifiers() -> egui::Modifiers {
    egui::Modifiers::NONE
}

pub fn any_key_held() -> bool {
    false
}

pub fn mouse_held() -> bool {
    false
}

pub fn pressed_within(_seconds: f64) -> bool {
    false
}

pub struct PressesElsewhere;

impl PressesElsewhere {
    pub fn take_press(&self) -> bool {
        false
    }
}

pub fn watch_presses_elsewhere() -> Option<PressesElsewhere> {
    None
}

pub fn cursor_in_window(_window: &Window) -> Option<egui::Pos2> {
    None
}

pub fn capture_monitor(_monitor: &MonitorHandle) -> Result<Capture, String> {
    Err("screen capture is not implemented on this platform yet".into())
}

pub fn screen_capture_access(_request: bool) -> bool {
    true
}

pub fn remember_front_app() -> Option<FrontApp> {
    None
}

pub fn restore_front_app(_app: FrontApp) {}

pub fn running_apps() -> Vec<std::path::PathBuf> {
    Vec::new()
}

pub fn front_app_name() -> Option<String> {
    None
}

pub fn on_console_quit(_quit: impl Fn() + Send + Sync + 'static) {}

pub fn exit_now() -> ! {
    std::process::exit(0)
}

pub fn videos_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_VIDEOS_DIR").map(PathBuf::from).or_else(|| known_dir(KnownDir::Videos))
}

pub fn known_dir(dir: KnownDir) -> Option<PathBuf> {
    let name = match dir {
        KnownDir::Home => "",
        KnownDir::Pictures => "Pictures",
        KnownDir::Videos => "Videos",
        KnownDir::Desktop => "Desktop",
        KnownDir::Downloads => "Downloads",
        KnownDir::Documents => "Documents",
    };
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(name))
}

pub fn open(target: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open").arg(target).spawn().map(drop).map_err(|err| err.to_string())
}

pub fn default_local_data_dir() -> Option<PathBuf> {
    default_data_dir()
}

pub fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|dir| dir.join("notchbase"))
}

fn base_attributes(title: &str) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
}

pub const BLURS_BEHIND: bool = false;

pub struct Backdrop;

impl Backdrop {
    pub fn new(_window: &winit::window::Window) -> Self {
        Self
    }

    pub fn shape(&mut self, _outlines: Vec<Vec<egui::Pos2>>, _panes: &[crate::ui::erase::Pane]) {}
}
