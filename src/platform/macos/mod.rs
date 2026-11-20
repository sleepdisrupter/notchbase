//! The macOS side of the platform module, mostly AppKit through the objc2 crates.

mod annotate;
mod appkit;
mod backdrop;
mod capture;
mod console;
mod focus;
mod folder;
mod front_app;
mod haptics;
mod instance;
mod interaction;
mod lock_screen;
mod notch;
mod process;
mod system;
mod window;

pub use annotate::{annotate_title_inset, annotate_window_attributes, configure_annotate_window};
pub use appkit::window_origin;
pub use backdrop::{BLURS_BEHIND, Backdrop};
pub use capture::screen_capture_access;
pub use console::{exit_now, on_console_quit};
pub use focus::focus_window;
pub use folder::pick_folder;
pub use front_app::{FrontApp, front_app_name, remember_front_app, restore_front_app, running_apps};
pub use haptics::perform_haptic;
pub use instance::{HeldInstance, hold_instance};
pub use interaction::{
    PressesElsewhere, any_key_held, begin_keyboard_focus, cursor_in_window, end_keyboard_focus, held_modifiers,
    mouse_held, pressed_within, set_click_through, watch_presses_elsewhere,
};
pub use lock_screen::{leave_lock_screen, show_on_lock_screen};
pub use notch::{hardware_notch, virtual_display};
pub use process::process_alive;
pub use system::{
    KEY_JOINER, MODIFIER_LABELS, default_data_dir, default_local_data_dir, known_dir, let_hooks_hear_our_windows, open,
    setup_event_loop, until_vblank, videos_dir,
};
pub use window::{
    configure_dim_window, configure_notch_window, configure_overlay_window, dim_window_attributes, keep_on_top,
    notch_window_attributes, overlay_window_attributes, raise_window,
};
