//! Mac basics: an accessory app with no Dock icon or menu, the known folders, Application Support and /usr/bin/open.

use std::path::PathBuf;

use winit::event_loop::{EventLoop, EventLoopBuilder};
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS as _};

use crate::platform::KnownDir;

pub const MODIFIER_LABELS: [&str; 4] = ["⌃", "⌥", "⇧", "⌘"];
pub const KEY_JOINER: &str = "";

pub fn until_vblank() -> Option<std::time::Duration> {
    None
}

pub fn setup_event_loop<T>(builder: &mut EventLoopBuilder<T>) {
    builder
        .with_activation_policy(ActivationPolicy::Accessory)
        .with_default_menu(false)
        .with_activate_ignoring_other_apps(false);
}

pub fn let_hooks_hear_our_windows<T>(_event_loop: &EventLoop<T>) {}

pub fn videos_dir() -> Option<PathBuf> {
    known_dir(KnownDir::Videos)
}

pub fn known_dir(dir: KnownDir) -> Option<PathBuf> {
    let name = match dir {
        KnownDir::Home => "",
        KnownDir::Pictures => "Pictures",
        KnownDir::Videos => "Movies",
        KnownDir::Desktop => "Desktop",
        KnownDir::Downloads => "Downloads",
        KnownDir::Documents => "Documents",
    };
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(name))
}

pub fn open(target: &str) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open").arg(target).spawn().map(drop).map_err(|err| err.to_string())
}

pub fn default_local_data_dir() -> Option<PathBuf> {
    default_data_dir()
}

pub fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/notchbase"))
}
