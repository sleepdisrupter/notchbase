//! Everything that differs between macOS and Windows. Each system has its own folder, used here as `imp`, and
//! `other.rs` stands in on the rest so the app still builds and tests on Linux.

#[cfg(not(target_os = "macos"))]
mod capture;
mod chime;
mod haptics;
mod hotkeys;
mod instance;
mod local_time;
mod paths;
mod piped;
mod placement;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::other as imp;

#[cfg(not(target_os = "macos"))]
pub use self::capture::{Capture, to_srgb_image};
pub use self::chime::{Ringer, chime};
pub use self::haptics::{Feel, haptic};
pub use self::hotkeys::{hotkey_caption, hotkey_labels};
#[cfg(not(target_os = "macos"))]
pub use self::imp::capture_monitor;
pub use self::imp::{
    BLURS_BEHIND, Backdrop, FrontApp, PressesElsewhere, any_key_held, begin_keyboard_focus, configure_notch_window,
    cursor_in_window, end_keyboard_focus, exit_now, focus_window, front_app_name, held_modifiers, known_dir,
    leave_lock_screen, let_hooks_hear_our_windows, mouse_held, notch_window_attributes, on_console_quit, open,
    pick_folder, pressed_within, process_alive, remember_front_app, restore_front_app, running_apps,
    screen_capture_access, set_click_through, setup_event_loop, show_on_lock_screen, until_vblank, videos_dir,
    watch_presses_elsewhere, window_origin,
};
pub use self::imp::{
    annotate_title_inset, annotate_window_attributes, configure_annotate_window, configure_dim_window,
    configure_overlay_window, dim_window_attributes, keep_on_top, overlay_window_attributes, raise_window,
};
#[cfg(windows)]
pub use self::imp::{local_time, run_as_admin, shell_process, windowed_processes};
#[cfg(all(test, any(windows, target_os = "macos")))]
pub use self::instance::claim_instance_in;
pub use self::instance::{InstanceLock, claim_instance};
#[cfg(not(windows))]
pub use self::local_time::local_time;
pub use self::local_time::{LocalTime, clock_24h, utc_offset_secs};
pub use self::paths::{KnownDir, config_file, config_file_in, data_dir, local_data_dir, tools_dir, tools_dirs};
pub use self::piped::input_is_piped;
pub use self::placement::{Placement, Style, placements};
#[cfg(windows)]
pub use notchbase_wasapi::Apartment;
