//! What the app and the installer agree on: flags, install locations and the uninstall key.

mod bundle;
mod choices;
mod destination;
#[cfg(windows)]
mod registry;

pub use bundle::{BUNDLE, BUNDLE_BINARY, app_in, bundle_version, unzip};
pub use choices::Choices;
pub use destination::{mac_destination, windows_destination};
#[cfg(windows)]
pub use registry::registered_version;

pub const INSTALL: &str = "--install";
pub const UNINSTALL: &str = "--uninstall";
pub const ERROR_PREFIX: &str = "notchbase: ";
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\notchbase";
pub const DISPLAY_VERSION: &str = "DisplayVersion";
// Process creation flag that keeps child processes from flashing a console window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_flags_and_error_prefix() {
        assert_eq!((INSTALL, UNINSTALL), ("--install", "--uninstall"));
        assert_eq!(format!("{ERROR_PREFIX}Access is denied."), "notchbase: Access is denied.");
    }

    #[test]
    fn uninstall_key_and_version_name() {
        assert!(UNINSTALL_KEY.starts_with(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\"));
        assert!(UNINSTALL_KEY.ends_with(r"\notchbase"));
        assert_eq!(DISPLAY_VERSION, "DisplayVersion");
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }
}
