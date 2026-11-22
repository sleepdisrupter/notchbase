//! Finding the driver's virtual monitors among Windows' displays, turning one on at a size
//! and place, or off again, and which of them Roam has in use. Everything here works without
//! administrator rights.

use std::sync::Mutex;

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig, SDC_ALLOW_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE, SDC_TOPOLOGY_EXTEND,
    SDC_USE_SUPPLIED_DISPLAY_CONFIG, SetDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, POINTL};
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
/// Windows keeps it off from then on, even after a restart.
pub fn detach(source: &Source) -> Result<(), String> {
    // The display topology first, saved: turning it off the older way only lasts until
    // Windows next sets up its displays, which brings the driver's screen back.
    let topology = detach_path(source);
    if !find(&source.name).is_some_and(|s| s.attached) {
        return Ok(());
    }
    let devmode = DEVMODEW { dmFields: DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT, ..Default::default() };
    change(source, devmode)?;
    if find(&source.name).is_some_and(|s| s.attached) {
        return Err(topology.err().unwrap_or_else(|| "Windows kept the virtual display on".into()));
    }
    Ok(())
}

/// The driver screens Roam has in use, by name: each shows one lent screen.
static IN_USE: Mutex<Vec<[u16; 32]>> = Mutex::new(Vec::new());

/// How many driver screens are in use.
pub fn in_use() -> usize {
    IN_USE.lock().map_or(0, |used| used.len())
}

/// Takes a driver screen nobody is using, chosen by `choose` from the free ones, and keeps it
/// from the others until `release`. Choosing and taking go together, so two screens asked for at
/// once never get the same one.
pub fn claim<T>(choose: impl FnOnce(&[Source], usize) -> Result<(Source, T), String>) -> Result<(Source, T), String> {
    let mut used = IN_USE.lock().map_err(|_| "Lost track of the virtual displays".to_owned())?;
    let all = virtual_sources();
    let free: Vec<Source> = all.iter().copied().filter(|s| !used.contains(&s.name)).collect();
    let (source, chosen) = choose(&free, all.len())?;
    used.push(source.name);
    Ok((source, chosen))
}

/// The screen is no longer in use.
pub fn release(source: &Source) {
    if let Ok(mut used) = IN_USE.lock() {
        used.retain(|name| *name != source.name);
    }
}

/// Turns off every driver screen that's on but not in use: left from a run that didn't tidy
/// up, the driver's own at start-up, or turned on by Windows alongside one that's wanted.
pub fn detach_unused() -> Result<(), String> {
    let used = IN_USE.lock().map(|used| used.clone()).unwrap_or_default();
    let mut result = Ok(());
    for source in virtual_sources().iter().filter(|s| s.attached && !used.contains(&s.name)) {
        result = result.and(detach(source));
    }
    result
}

/// Takes the source's path out of the active display topology, and saves that.
fn detach_path(source: &Source) -> Result<(), String> {
    let (mut paths, modes) = active_paths()?;
    let mut found = false;
    for path in &mut paths {
        if source_name(path).is_some_and(|name| text(&name) == text(&source.name)) {
            path.flags &= !DISPLAYCONFIG_PATH_ACTIVE;
            path.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
            path.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
            found = true;
        }
    }
    if !found {
        return Err("Windows has no path for the virtual display".into());
    }
    let flags = SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES | SDC_SAVE_TO_DATABASE;
    // SAFETY: the arrays are as Windows answered them, with one path turned off.
    let code = unsafe { SetDisplayConfig(Some(&paths), Some(&modes), flags) };
    if code != 0 {
        return Err(format!("Windows couldn't turn the virtual display off (code {code})"));
    }
    Ok(())
}

/// `DISPLAYCONFIG_PATH_ACTIVE`, and the index for a path without a mode.
const DISPLAYCONFIG_PATH_ACTIVE: u32 = 1;
const DISPLAYCONFIG_PATH_MODE_IDX_INVALID: u32 = u32::MAX;

/// The displays Windows has on now, as paths and their modes.
fn active_paths() -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), String> {
    // The sizes can change between the two calls when a display comes or goes: try again.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: plain counts.
        let sized = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) };
        if sized != ERROR_SUCCESS {
            return Err(format!("Windows didn't list its displays (code {})", sized.0));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: the arrays are as long as the counts say.
        let listed = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if listed == ERROR_SUCCESS {
            paths.truncate(path_count as usize);
            modes.truncate(mode_count as usize);
            return Ok((paths, modes));
        }
        if listed.0 != 122 {
            // Anything but ERROR_INSUFFICIENT_BUFFER is final.
            return Err(format!("Windows didn't list its displays (code {})", listed.0));
        }
    }
    Err("Windows' displays kept changing".into())
}

/// The `\\.\DISPLAYn` name of a path's source.
fn source_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<[u16; 32]> {
    let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
    name.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
    name.header.adapterId = path.sourceInfo.adapterId;
    name.header.id = path.sourceInfo.id;
    // SAFETY: `name` is sized as its header says.
    (unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } == 0).then_some(name.viewGdiDeviceName)
}
