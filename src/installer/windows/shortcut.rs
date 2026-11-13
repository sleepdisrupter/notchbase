//! Start menu and desktop shortcuts, made through the shell's `IShellLinkW`.

use std::io;
use std::path::Path;

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{HSTRING, Interface};

use crate::platform::Apartment;

const DESCRIPTION: &str = "A fluid notch for Windows";

pub(super) fn create(link: &Path, exe: &Path) -> io::Result<()> {
    let _apartment = Apartment::join();
    save(link, exe).map_err(|err| io::Error::other(err.message()))
}

fn save(link: &Path, exe: &Path) -> windows::core::Result<()> {
    unsafe {
        let shell: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        shell.SetPath(&HSTRING::from(exe.as_os_str()))?;
        if let Some(dir) = exe.parent() {
            shell.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        shell.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0)?;
        shell.SetDescription(&HSTRING::from(DESCRIPTION))?;
        shell.cast::<IPersistFile>()?.Save(&HSTRING::from(link.as_os_str()), true)
    }
}
