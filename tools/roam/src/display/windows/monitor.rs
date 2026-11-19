//! Finding the driver's virtual monitors among Windows' displays, and turning one on at a size
//! and place, or off again. Everything here works without administrator rights.

use windows::Win32::Devices::Display::{SDC_APPLY, SDC_TOPOLOGY_EXTEND, SetDisplayConfig};
use windows::Win32::Foundation::POINTL;
use windows::Win32::Graphics::Gdi::{
    CDS_NORESET, CDS_TYPE, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION,
    ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplayDevicesW, EnumDisplaySettingsW,
};
use windows::core::PCWSTR;

use crate::layout::Rect;

/// What the driver's adapter may be called in the list of displays: its own name, and the
/// names of the sample driver it grew from, which some versions still show.
const ADAPTERS: [&str; 3] = ["Virtual Display Driver", "IddSampleDriver", "MttVDD"];

/// One of Windows' display sources, `\\.\DISPLAY3` and the like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    pub name: [u16; 32],
    pub attached: bool,
}

impl Source {
    fn name(&self) -> PCWSTR {
        PCWSTR(self.name.as_ptr())
    }
}

fn text(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// The driver's display sources, on or off.
pub fn virtual_sources() -> Vec<Source> {
    let mut found = Vec::new();
    for index in 0.. {
        let mut device = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        // SAFETY: `device` is sized as the call expects.
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) }.as_bool() {
            break;
        }
        let name = text(&device.DeviceString);
        // Its device id (Root\MttVDD) holds whatever Windows calls it.
        let id = text(&device.DeviceID).to_ascii_lowercase();
        if ADAPTERS.iter().any(|adapter| name.contains(adapter)) || id.contains("mttvdd") {
            found.push(Source {
                name: device.DeviceName,
                attached: device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != Default::default(),
            });
        }
    }
    found
}

/// The source by its name, as it stands now.
pub fn find(name: &[u16; 32]) -> Option<Source> {
    virtual_sources().into_iter().find(|s| s.name == *name)
}

fn settings(source: &Source, mode: ENUM_DISPLAY_SETTINGS_MODE) -> Option<DEVMODEW> {
    let mut devmode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY: `devmode` is sized as the call expects.
    unsafe { EnumDisplaySettingsW(source.name(), mode, &mut devmode) }.as_bool().then_some(devmode)
}

/// The sizes and refresh rates the source offers.
pub fn modes(source: &Source) -> Vec<[u32; 3]> {
    let mut modes = Vec::new();
    for index in 0u32.. {
        let Some(mode) = settings(source, ENUM_DISPLAY_SETTINGS_MODE(index)) else { break };
        let mode = [mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency];
        if !modes.contains(&mode) {
            modes.push(mode);
        }
    }
    modes
}

/// Where the source is on the desktop, in pixels, while it's on.
pub fn rect(source: &Source) -> Option<Rect> {
    let current = find(&source.name)?;
    if !current.attached {
        return None;
    }
    let mode = settings(&current, ENUM_CURRENT_SETTINGS)?;
    // SAFETY: for displays, the position half of the union is the one in use.
    let at = unsafe { mode.Anonymous1.Anonymous2.dmPosition };
    (mode.dmPelsWidth > 0)
        .then(|| Rect::new(f64::from(at.x), f64::from(at.y), f64::from(mode.dmPelsWidth), f64::from(mode.dmPelsHeight)))
}

fn change(source: &Source, mut devmode: DEVMODEW) -> Result<(), String> {
    devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    let flags = CDS_TYPE(CDS_UPDATEREGISTRY.0 | CDS_NORESET.0);
    // SAFETY: `devmode` lives for the call; the second call applies what the first stored.
    let stored = unsafe { ChangeDisplaySettingsExW(source.name(), Some(&devmode), None, flags, None) };
    if stored != DISP_CHANGE_SUCCESSFUL {
        return Err(format!("Windows didn't take the display's settings (code {})", stored.0));
    }
    // SAFETY: applies the settings stored above.
    let applied = unsafe { ChangeDisplaySettingsExW(PCWSTR::null(), None, None, CDS_TYPE(0), None) };
    if applied != DISP_CHANGE_SUCCESSFUL {
        return Err(format!("Windows couldn't change the displays (code {})", applied.0));
    }
    Ok(())
}

/// Turns the source on, `size` pixels at `hz`, its top left at `at`.
pub fn attach(source: &Source, size: [u32; 2], hz: u32, at: [i32; 2]) -> Result<(), String> {
    let mut devmode = DEVMODEW {
        dmFields: DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
        dmPelsWidth: size[0],
        dmPelsHeight: size[1],
        dmDisplayFrequency: hz,
        ..Default::default()
    };
    devmode.Anonymous1.Anonymous2.dmPosition = POINTL { x: at[0], y: at[1] };
    let first = change(source, devmode);
    if find(&source.name).is_some_and(|s| s.attached) {
        return first;
    }
    // Some systems only bring a new display in through the display topology: extend the
    // desktop onto every connected display, then size and place it.
    // SAFETY: no paths given: Windows works out the extended topology itself.
    let code = unsafe { SetDisplayConfig(None, None, SDC_APPLY | SDC_TOPOLOGY_EXTEND) };
    if code != 0 {
        return first.and(Err(format!("Windows couldn't extend the desktop (code {code})")));
    }
    change(source, devmode)
}

/// Moves the source's top left to `at`.
pub fn place(source: &Source, at: [i32; 2]) -> Result<(), String> {
    let mut devmode = DEVMODEW { dmFields: DM_POSITION, ..Default::default() };
    devmode.Anonymous1.Anonymous2.dmPosition = POINTL { x: at[0], y: at[1] };
    change(source, devmode)
}

/// Turns the source off: it leaves the desktop, and its windows move to the other displays.
pub fn detach(source: &Source) -> Result<(), String> {
    let devmode = DEVMODEW { dmFields: DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT, ..Default::default() };
    change(source, devmode)
}
