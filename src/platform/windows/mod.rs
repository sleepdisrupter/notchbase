//! The Windows side of the platform module. Haptics, the lock screen and the backdrop are Mac only and do nothing here.

mod annotate;
mod app_windows;
mod capture;
mod console;
mod focus;
mod folder;
mod front_app;
mod instance;
mod interaction;
mod pacing;
mod process;
mod system;
mod window;
mod window_style;

pub use annotate::{annotate_title_inset, annotate_window_attributes, configure_annotate_window};
pub use app_windows::{shell_process, windowed_processes};
pub use capture::{capture_monitor, screen_capture_access};
pub use console::{exit_now, on_console_quit};
pub use focus::focus_window;
pub use folder::pick_folder;
pub use front_app::{FrontApp, front_app_name, remember_front_app, restore_front_app, running_apps};
pub use instance::{HeldInstance, hold_instance};
pub use interaction::{
    PressesElsewhere, any_key_held, begin_keyboard_focus, cursor_in_window, end_keyboard_focus, held_modifiers,
    mouse_held, pressed_within, set_click_through, watch_presses_elsewhere,
};
pub use pacing::until_vblank;

pub fn perform_haptic(_feel: crate::platform::Feel) {}

pub fn window_origin(_window: &winit::window::Window) -> Option<egui::Pos2> {
    None
}

pub fn show_on_lock_screen(_window: &winit::window::Window) -> bool {
    false
}

pub fn leave_lock_screen(_window: &winit::window::Window) {}
pub use process::process_alive;
pub use system::{
    KEY_JOINER, MODIFIER_LABELS, default_data_dir, default_local_data_dir, hardware_notch, known_dir,
    let_hooks_hear_our_windows, local_time, open, run_as_admin, setup_event_loop, videos_dir, virtual_display,
};
pub use window::{
    configure_dim_window, configure_notch_window, configure_overlay_window, dim_window_attributes, keep_on_top,
    notch_window_attributes, overlay_window_attributes, raise_window,
};

pub const BLURS_BEHIND: bool = false;

pub struct Backdrop;

impl Backdrop {
    pub fn new(_window: &winit::window::Window) -> Self {
        Self
    }

    pub fn shape(&mut self, _outlines: Vec<Vec<egui::Pos2>>, _panes: &[crate::ui::erase::Pane]) {}
}
