//! notchbase dock, the separate Dock replacement: its settings, read from and written to its own `dock.json`, and
//! starting or quitting it. The dock watches that file and takes a change within half a second. Only the keys a change
//! names are written, so the dock's pinned apps and anything else in the file stay as they are.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Map, Value};

use crate::bridge::{DockChange, DockMissionControl, DockSide, DockStackSort, DockStackView, DockView};

/// Whether this platform has the dock at all: macOS only (Windows had it until 0.33).
pub(crate) const SUPPORTED: bool = cfg!(target_os = "macos");
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BUNDLE_ID: &str = "app.notchbase.dock";
/// Where notchbase.app carries the dock, relative to its `Contents` folder.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const HELPER: &str = "Helpers/notchbase dock.app";
const FILE: &str = "dock.json";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PID_FILE: &str = "dock.pid";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Under the pid in the pid file when the dock can make way for a newer copy without showing the macOS Dock.
const CAN_HAND_OVER: &str = "handover";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// The note that asks it to (the dock removes it).
const HANDOVER_FILE: &str = "handover";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// How long an old dock gets to quit before it's stopped outright.
const QUIT_WAIT: Duration = Duration::from_secs(3);
const LOOK_EVERY: Duration = Duration::from_millis(700);

pub(crate) struct DockLink {
    dir: PathBuf,
    view: DockView,
    stamp: Option<SystemTime>,
    looked: Option<Instant>,
}

impl Default for DockLink {
    fn default() -> Self {
        Self::new(data_dir())
    }
}

impl DockLink {
    pub(crate) fn new(dir: PathBuf) -> Self {
        let mut link = Self { dir, view: DockView::default(), stamp: None, looked: None };
        link.reload();
        link
    }

    pub(crate) fn view(&self) -> &DockView {
        &self.view
    }

    /// Looks at the file again now and then, so a change made in the dock itself shows here too.
    pub(crate) fn refresh(&mut self, now: Instant) {
        if self.looked.is_some_and(|at| now.duration_since(at) < LOOK_EVERY) {
            return;
        }
        self.looked = Some(now);
        let stamp = std::fs::metadata(self.dir.join(FILE)).and_then(|m| m.modified()).ok();
        if stamp != self.stamp {
            self.reload();
        }
        self.view.running = running(&self.dir);
    }

    /// Whether the dock is running this moment, looked up now rather than at the last refresh.
    pub(crate) fn running_now(&self) -> bool {
        running(&self.dir)
    }

    /// Whether the running dock is an older copy of the one inside this notchbase.app: notchbase was updated (and
    /// the helper with it) while the old dock kept running.
    pub(crate) fn outdated(&self) -> bool {
        outdated_now(&self.dir)
    }

    /// Swaps the running dock for the copy inside this notchbase.app, off the main thread. The old one quits without
    /// putting the macOS Dock back, and the new one takes the hidden Dock over, so the real Dock never shows.
    pub(crate) fn restart(&self) {
        let dir = self.dir.clone();
        let _ = std::thread::Builder::new().name("dock restart".into()).spawn(move || {
            if make_way(&dir) {
                start();
            }
        });
    }

    /// Reads the file again now, so a page opened right away shows it as it is.
    pub(crate) fn reload_now(&mut self) {
        self.looked = None;
        self.reload();
    }

    fn reload(&mut self) {
        let path = self.dir.join(FILE);
        self.stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let object = read(&path);
        self.view = view_of(object.as_ref());
        self.view.running = running(&self.dir);
    }

    /// Applies `change`: written to `dock.json` (the dock picks it up), or the dock started or quit.
    pub(crate) fn apply(&mut self, change: &DockChange) {
        if let DockChange::Running(on) = change {
            if *on {
                start()
            } else {
                stop(&self.dir)
            }
            self.view.running = *on;
            return;
        }
        let path = self.dir.join(FILE);
        let mut object = read(&path).unwrap_or_default();
        write_change(&mut object, change);
        if write(&self.dir, &path, &object) {
            self.reload();
        }
    }
}

/// Each screen notchbase dock is up on (its display number), how much of that screen it keeps in points, and from
/// which edge, from the `dock-areas.json` it keeps while it runs. Read at most twice a second.
pub(crate) fn reserved_edges() -> Vec<(u64, f32, DockSide)> {
    type Seen = Option<(Instant, Vec<(u64, f32, DockSide)>)>;
    static SEEN: std::sync::Mutex<Seen> = std::sync::Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, bottoms)) = seen.as_ref()
        && at.elapsed() < Duration::from_millis(500)
    {
        return bottoms.clone();
    }
    let dir = data_dir();
    let bottoms = if running(&dir) {
        std::fs::read_to_string(dir.join(AREAS_FILE)).map(|text| parse_areas(&text)).unwrap_or_default()
    } else {
        Vec::new()
    };
    *seen = Some((Instant::now(), bottoms.clone()));
    bottoms
}

const AREAS_FILE: &str = "dock-areas.json";

fn parse_areas(text: &str) -> Vec<(u64, f32, DockSide)> {
    let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    rows.iter()
        .filter_map(|row| {
            let display = row.get("display")?.as_u64()?;
            let reserved = row.get("reserved")?.as_f64()? as f32;
            // Docks before 0.33 only ever stood along the bottom and didn't say.
            let side = row.get("side").and_then(Value::as_str).and_then(DockSide::from_key).unwrap_or_default();
            (reserved > 0.0 && reserved < 400.0).then_some((display, reserved, side))
        })
        .collect()
}

#[cfg(windows)]
fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("notchbase dock")
}

#[cfg(not(windows))]
fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library/Application Support/notchbase dock")
}

fn read(path: &Path) -> Option<Map<String, Value>> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Value>(&text).ok()? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

fn write(dir: &Path, path: &Path, object: &Map<String, Value>) -> bool {
    let Ok(text) = serde_json::to_string_pretty(object) else { return false };
    let _ = std::fs::create_dir_all(dir);
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text).is_ok() && std::fs::rename(&partial, path).is_ok()
}

/// The page's view of the file; anything missing or odd reads as the dock's own default.
pub(crate) fn view_of(object: Option<&Map<String, Value>>) -> DockView {
    let base = DockView::default();
    let Some(o) = object else { return base };
    let flag = |key: &str, default: bool| o.get(key).and_then(Value::as_bool).unwrap_or(default);
    let number = |key: &str, default: f32| o.get(key).and_then(Value::as_f64).map_or(default, |n| n as f32);
    let text = |key: &str| o.get(key).and_then(Value::as_str);
    // Older files said `true` or `false` for the launch effect.
    let launch_pop = match o.get("launch_effect") {
        Some(Value::Bool(on)) => *on,
        Some(Value::String(name)) => name != "off",
        _ => base.launch_pop,
    };
    DockView {
        found: true,
        running: false,
        side: text("side").and_then(DockSide::from_key).unwrap_or(base.side),
        tile_size: number("tile_size", base.tile_size).clamp(16.0, 128.0),
        magnification: flag("magnification", base.magnification),
        show_names: flag("show_names", base.show_names),
        glass: number("glass_tint_top", base.glass).clamp(0.0, 1.0),
        window_previews: flag("window_previews", base.window_previews),
        avoid_windows: flag("avoid_windows", base.avoid_windows),
        hide_in_fullscreen: flag("hide_in_fullscreen", base.hide_in_fullscreen),
        all_displays: text("displays") == Some("all"),
        mission_control: if text("mission_control") == Some("fade") {
            DockMissionControl::Fade
        } else {
            DockMissionControl::Cover
        },
        launch_pop,
        minimize_liquid: text("minimize_effect") != Some("system"),
        minimize_duration: number("minimize_duration", base.minimize_duration).clamp(0.15, 1.5),
        replace_switcher: flag("replace_switcher", base.replace_switcher),
        music_pulse: flag("music_pulse", base.music_pulse),
        show_downloads: flag("show_downloads", base.show_downloads),
        stack_view: DockStackView::ALL
            .into_iter()
            .find(|v| text("stack_view") == Some(v.key()))
            .unwrap_or(base.stack_view),
        stack_sort: DockStackSort::ALL
            .into_iter()
            .find(|v| text("stack_sort") == Some(v.key()))
            .unwrap_or(base.stack_sort),
    }
}

/// Puts one change into the file's settings, leaving every other key alone.
pub(crate) fn write_change(o: &mut Map<String, Value>, change: &DockChange) {
    let mut set = |key: &str, value: Value| {
        o.insert(key.to_owned(), value);
    };
    let number = |n: f32| Value::from((f64::from(n) * 100.0).round() / 100.0);
    match change {
        DockChange::Running(_) => {}
        DockChange::Side(side) => set("side", Value::from(side.key())),
        DockChange::TileSize(size) => {
            let size = size.clamp(16.0, 128.0).round();
            set("tile_size", number(size));
            // The magnified size keeps the same ratio it had by default.
            set("large_size", number((size * 1.5).round()));
        }
        DockChange::Magnification(on) => set("magnification", Value::Bool(*on)),
        DockChange::ShowNames(on) => set("show_names", Value::Bool(*on)),
        DockChange::Glass(dark) => {
            let dark = dark.clamp(0.0, 1.0);
            set("glass_tint_top", number(dark));
            set("glass_tint_bottom", number((dark - 0.1).max(0.0)));
        }
        DockChange::WindowPreviews(on) => set("window_previews", Value::Bool(*on)),
        DockChange::AvoidWindows(on) => set("avoid_windows", Value::Bool(*on)),
        DockChange::HideInFullscreen(on) => set("hide_in_fullscreen", Value::Bool(*on)),
        DockChange::AllDisplays(on) => set("displays", Value::from(if *on { "all" } else { "primary" })),
        DockChange::MissionControl(how) => {
            let name = if *how == DockMissionControl::Fade { "fade" } else { "cover" };
            set("mission_control", Value::from(name));
        }
        DockChange::LaunchPop(on) => set("launch_effect", Value::from(if *on { "pop" } else { "off" })),
        DockChange::MinimizeLiquid(on) => set("minimize_effect", Value::from(if *on { "liquid" } else { "system" })),
        DockChange::MinimizeDuration(seconds) => set("minimize_duration", number(seconds.clamp(0.15, 1.5))),
        DockChange::ReplaceSwitcher(on) => set("replace_switcher", Value::Bool(*on)),
        DockChange::MusicPulse(on) => set("music_pulse", Value::Bool(*on)),
        DockChange::ShowDownloads(on) => set("show_downloads", Value::Bool(*on)),
        DockChange::StackView(view) => set("stack_view", Value::from(view.key())),
        DockChange::StackSort(sort) => set("stack_sort", Value::from(sort.key())),
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pid(dir: &Path) -> Option<i32> {
    read_pid(&std::fs::read_to_string(dir.join(PID_FILE)).ok()?)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// The pid on the pid file's first line.
fn read_pid(text: &str) -> Option<i32> {
    text.lines().next()?.trim().parse().ok()
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Whether the pid file says its dock can make way without showing the macOS Dock (docks from 0.23.11 on).
fn can_hand_over(text: &str) -> bool {
    text.lines().skip(1).any(|line| line.trim() == CAN_HAND_OVER)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Whether the dock running from `running`, started at `started`, is an older copy of `bundled`, which was put in
/// place at `replaced`. An update sets the old notchbase.app aside as `notchbase.app.old` and deletes it later, so
/// the old dock runs from the same file, from its copy in the set-aside app, or from a file that's gone (`None`).
/// Either way it started before the update. A dock run from anywhere else (a build being tried out) is never
/// touched.
fn outdated(running: Option<&Path>, started: SystemTime, bundled: &Path, replaced: SystemTime) -> bool {
    let ours = running.is_none_or(|running| running == bundled || Some(running) == set_aside(bundled).as_deref());
    ours && replaced > started + Duration::from_secs(1)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Where `bundled` is in the copy of notchbase.app an update sets aside.
fn set_aside(bundled: &Path) -> Option<PathBuf> {
    // `<app>/Contents/Helpers/notchbase dock.app/Contents/MacOS/notchbase-dock`
    let app = bundled.ancestors().nth(6)?;
    let inside = bundled.strip_prefix(app).ok()?;
    let mut name = app.file_name()?.to_os_string();
    name.push(".old");
    Some(app.with_file_name(name).join(inside))
}

#[cfg(target_os = "macos")]
fn outdated_now(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    let Some(pid) = pid(dir).filter(|pid| *pid > 0) else { return false };
    let Some(bundled) = bundled().map(|app| app.join("Contents/MacOS/notchbase-dock")) else { return false };
    let Ok(meta) = std::fs::metadata(&bundled) else { return false };
    // The time the file itself was put there (an update unpacks a new one), not the build time it keeps.
    let replaced = SystemTime::UNIX_EPOCH + Duration::new(meta.ctime().max(0) as u64, meta.ctime_nsec().max(0) as u32);
    let Some((running, started)) = process(pid) else { return false };
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    outdated(running.as_deref().map(canonical).as_deref(), started, &canonical(&bundled), replaced)
}

#[cfg(not(target_os = "macos"))]
fn outdated_now(_: &Path) -> bool {
    false
}

/// A process's executable (none when its file has been deleted since it started) and when it started.
#[cfg(target_os = "macos")]
fn process(pid: i32) -> Option<(Option<PathBuf>, SystemTime)> {
    use std::os::unix::ffi::OsStrExt as _;
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let len = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    let exe = (len > 0).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&path[..len as usize])));
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let got = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    if got != size {
        return None;
    }
    let started = SystemTime::UNIX_EPOCH
        + Duration::from_secs(info.pbi_start_tvsec)
        + Duration::from_micros(info.pbi_start_tvusec);
    Some((exe, started))
}

/// Gets the running dock out of the way for a newer copy. One that can hand over is asked to quit with a note, and
/// leaves the macOS Dock hidden; an older one is stopped outright, which leaves it the same way (as after a crash,
/// which the new dock picks up from). True once it's gone.
#[cfg(target_os = "macos")]
fn make_way(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join(PID_FILE)) else { return false };
    let Some(pid) = read_pid(&text).filter(|pid| *pid > 0) else { return false };
    let gone = || unsafe { libc::kill(pid, 0) } != 0;
    let wait = |for_: Duration| {
        let until = Instant::now() + for_;
        while Instant::now() < until && !gone() {
            std::thread::sleep(Duration::from_millis(40));
        }
        gone()
    };
    if can_hand_over(&text) && std::fs::write(dir.join(HANDOVER_FILE), "").is_ok() {
        unsafe { libc::kill(pid, libc::SIGTERM) };
        if wait(QUIT_WAIT) {
            return true;
        }
    }
    unsafe { libc::kill(pid, libc::SIGKILL) };
    wait(QUIT_WAIT)
}

#[cfg(not(target_os = "macos"))]
fn make_way(_: &Path) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn running(dir: &Path) -> bool {
    pid(dir).is_some_and(|pid| pid > 0 && unsafe { libc::kill(pid, 0) } == 0)
}

#[cfg(not(target_os = "macos"))]
fn running(_: &Path) -> bool {
    false
}

/// Starts the dock (it hides the macOS Dock itself): the copy inside notchbase.app when there is one, or else wherever
/// macOS knows it, such as the standalone build in `target/release`.
fn start() {
    if !SUPPORTED {
        return;
    }
    let mut open = std::process::Command::new("/usr/bin/open");
    open.arg("-g");
    match bundled() {
        Some(app) => open.arg(app),
        None => open.args(["-b", BUNDLE_ID]),
    };
    let _ = open.spawn();
}

/// `notchbase.app/Contents/Helpers/notchbase dock.app`, beside the running notchbase.
fn bundled() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    helper_beside(&exe).filter(|app| app.join("Contents/MacOS").is_dir())
}

/// Where the helper sits for an executable in `Contents/MacOS`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn helper_beside(exe: &Path) -> Option<PathBuf> {
    let contents = exe.parent()?.parent()?;
    (contents.file_name()? == "Contents").then(|| contents.join(HELPER))
}

/// Asks the dock to quit: it puts the macOS Dock back on its way out.
#[cfg(target_os = "macos")]
fn stop(dir: &Path) {
    if let Some(pid) = pid(dir).filter(|pid| *pid > 0) {
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

#[cfg(not(target_os = "macos"))]
fn stop(_: &Path) {}

/// Windows had notchbase dock until 0.33, which made it a macOS thing only. A dock an older notchbase left running
/// is asked to quit (it puts the taskbar back on its way out), the taskbar is put back if a dock was stopped with it
/// put away, and the copies older versions wrote out are deleted. Off the main thread; nothing to do takes no time.
#[cfg(windows)]
pub(crate) fn retire() {
    let _ = std::thread::Builder::new().name("old dock".into()).spawn(|| windows::retire(&data_dir()));
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use serde_json::Value;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, STILL_ACTIVE, TRUE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        QueryFullProcessImageNameW, TerminateProcess,
    };
    use windows_sys::Win32::UI::Shell::{ABM_SETSTATE, ABS_ALWAYSONTOP, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        ANIMATIONINFO, EnumWindows, FindWindowW, GetClassNameW, SPI_SETANIMATION, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
        SW_SHOWNA, ShowWindow, SystemParametersInfoW,
    };
    use windows_sys::core::BOOL;

    use super::{QUIT_WAIT, pid};

    /// The note the old dock looks for on every tick, and quits when it finds.
    const QUIT_FILE: &str = "quit";
    /// What the old dock saved before putting the taskbar away.
    const SAVED_TASKBAR: &str = "saved-taskbar.json";

    pub(super) fn retire(dir: &Path) {
        if let Some(pid) = dock_pid(dir) {
            quit(dir, pid);
        }
        if dock_pid(dir).is_some() {
            return;
        }
        restore_taskbar(dir);
        // `%LOCALAPPDATA%\notchbase\dock`, where older versions wrote their dock out.
        if let Some(base) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            let _ = std::fs::remove_dir_all(base.join("notchbase").join("dock"));
        }
        let _ = std::fs::remove_file(dir.join(super::PID_FILE));
    }

    fn open(pid: i32, access: u32) -> Option<HANDLE> {
        if pid <= 0 {
            return None;
        }
        let handle = unsafe { OpenProcess(access, 0, pid as u32) };
        (!handle.is_null()).then_some(handle)
    }

    fn alive(pid: i32) -> bool {
        let Some(handle) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else { return false };
        let mut code = 0;
        let read = unsafe { GetExitCodeProcess(handle, &mut code) } != 0;
        unsafe { CloseHandle(handle) };
        read && code == STILL_ACTIVE as u32
    }

    fn program_of(pid: i32) -> Option<PathBuf> {
        let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut path = [0u16; 1024];
        let mut length = path.len() as u32;
        let read =
            unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length) } != 0;
        unsafe { CloseHandle(handle) };
        read.then(|| PathBuf::from(String::from_utf16_lossy(&path[..(length as usize).min(path.len())])))
    }

    /// The pid file's dock, when it's still running and still the dock (pids get reused).
    fn dock_pid(dir: &Path) -> Option<i32> {
        let pid = pid(dir).filter(|pid| alive(*pid))?;
        let stem = program_of(pid)?.file_stem()?.to_string_lossy().to_lowercase();
        (stem == "notchbase-dock").then_some(pid)
    }

    /// Asks the dock to quit, and stops it outright if it hasn't within a few seconds.
    fn quit(dir: &Path, pid: i32) {
        let _ = std::fs::write(dir.join(QUIT_FILE), "");
        let until = Instant::now() + QUIT_WAIT * 2;
        while Instant::now() < until && alive(pid) {
            std::thread::sleep(Duration::from_millis(50));
        }
        if alive(pid)
            && let Some(handle) = open(pid, PROCESS_TERMINATE)
        {
            unsafe { TerminateProcess(handle, 1) };
            unsafe { CloseHandle(handle) };
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = std::fs::remove_file(dir.join(QUIT_FILE));
    }

    /// Puts the taskbar back as the old dock found it, when it was stopped before it could do that itself.
    fn restore_taskbar(dir: &Path) {
        let path = dir.join(SAVED_TASKBAR);
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let saved = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
        let autohide = saved.get("autohide").and_then(Value::as_bool).unwrap_or(false);
        let min_animate = saved.get("min_animate").and_then(Value::as_bool).unwrap_or(true);
        for hwnd in taskbars() {
            unsafe { ShowWindow(hwnd, SW_SHOWNA) };
        }
        let mut data: APPBARDATA = unsafe { std::mem::zeroed() };
        data.cbSize = size_of::<APPBARDATA>() as u32;
        data.hWnd = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
        data.lParam = if autohide { ABS_AUTOHIDE as LPARAM } else { ABS_ALWAYSONTOP as LPARAM };
        unsafe { SHAppBarMessage(ABM_SETSTATE, &mut data) };
        let mut info = ANIMATIONINFO { cbSize: size_of::<ANIMATIONINFO>() as u32, iMinAnimate: i32::from(min_animate) };
        let flags = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
        unsafe { SystemParametersInfoW(SPI_SETANIMATION, info.cbSize, (&raw mut info).cast(), flags) };
        let _ = std::fs::remove_file(&path);
    }

    /// The taskbar on the main screen and the ones on the others.
    fn taskbars() -> Vec<HWND> {
        unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
            let mut name = [0u16; 64];
            let len = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32) }.max(0) as usize;
            if String::from_utf16_lossy(&name[..len]) == "Shell_SecondaryTrayWnd" {
                unsafe { &mut *(found as *mut Vec<HWND>) }.push(hwnd);
            }
            TRUE
        }
        let mut found = Vec::new();
        let main = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
        if !main.is_null() {
            found.push(main);
        }
        unsafe { EnumWindows(Some(visit), (&raw mut found) as LPARAM) };
        found
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_bundled_dock_replaced_since_it_started_is_outdated() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let bundled =
            Path::new("/Applications/notchbase.app/Contents/Helpers/notchbase dock.app/Contents/MacOS/notchbase-dock");
        let later = t + Duration::from_secs(30);
        assert!(outdated(Some(bundled), t, bundled, later), "updated after the dock started: restart it");
        assert!(!outdated(Some(bundled), later, bundled, t), "started after the update: it's the new one");
        assert!(!outdated(Some(bundled), t, bundled, t + Duration::from_millis(400)), "the same moment is no update");
        let tried = Path::new("/Users/me/notchbase/target/release/notchbase dock.app/Contents/MacOS/notchbase-dock");
        assert!(!outdated(Some(tried), t, bundled, later), "a dock run from elsewhere is left alone");
    }

    #[test]
    fn a_dock_left_running_from_the_app_an_update_set_aside_is_outdated() {
        // What 0.23.16's update left: the old dock running from notchbase.app.old, which was deleted later.
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let later = t + Duration::from_secs(7_000);
        let bundled =
            Path::new("/Applications/notchbase.app/Contents/Helpers/notchbase dock.app/Contents/MacOS/notchbase-dock");
        let aside = Path::new(
            "/Applications/notchbase.app.old/Contents/Helpers/notchbase dock.app/Contents/MacOS/notchbase-dock",
        );
        assert_eq!(set_aside(bundled).as_deref(), Some(aside));
        assert!(outdated(Some(aside), t, bundled, later), "running from the set-aside copy");
        assert!(outdated(None, t, bundled, later), "its file is gone now");
        assert!(!outdated(None, later, bundled, t), "a dock started after the update is never stopped");
    }

    #[test]
    fn the_pid_file_reads_with_or_without_the_handover_line() {
        assert_eq!(read_pid("4242\nhandover\n"), Some(4242));
        assert_eq!(read_pid("4242\n"), Some(4242), "a dock from before 0.23.11");
        assert_eq!(read_pid(""), None);
        assert!(can_hand_over("4242\nhandover\n"));
        assert!(!can_hand_over("4242\n"), "an older dock is stopped outright instead");
        assert!(!can_hand_over("handover"), "the pid line itself never counts");
    }

    #[test]
    fn the_docks_areas_file_reads_as_screens_and_heights() {
        let text = r#"[{"display": 1, "reserved": 70.0}, {"display": 4, "reserved": 74.5, "side": "left"}]"#;
        assert_eq!(parse_areas(text), vec![(1, 70.0, DockSide::Bottom), (4, 74.5, DockSide::Left)]);
        assert!(parse_areas("").is_empty() && parse_areas("{}").is_empty());
        assert_eq!(
            parse_areas(r#"[{"display": 1, "reserved": 0}, {"display": 2}]"#),
            vec![],
            "nothing kept, or broken"
        );
    }

    #[test]
    fn a_change_writes_only_its_own_keys() {
        let mut o: Map<String, Value> =
            serde_json::from_str(r#"{"pinned": ["/Applications/Mail.app"], "tile_size": 48, "apps": {}}"#).unwrap();
        write_change(&mut o, &DockChange::MinimizeLiquid(false));
        write_change(&mut o, &DockChange::TileSize(56.4));
        write_change(&mut o, &DockChange::MissionControl(DockMissionControl::Fade));
        assert_eq!(o["pinned"], serde_json::json!(["/Applications/Mail.app"]), "the pinned apps stay");
        assert_eq!(o["minimize_effect"], "system");
        assert_eq!(o["tile_size"], 56.0);
        assert_eq!(o["large_size"], 84.0);
        let view = view_of(Some(&o));
        assert!(view.found && !view.minimize_liquid);
        assert_eq!(view.tile_size, 56.0);
        assert_eq!(view.mission_control, DockMissionControl::Fade);
        assert_eq!((view.stack_view, view.stack_sort), (DockStackView::Automatic, DockStackSort::Added));
        write_change(&mut o, &DockChange::StackView(DockStackView::Grid));
        write_change(&mut o, &DockChange::StackSort(DockStackSort::Name));
        assert_eq!(
            (o["stack_view"].as_str(), o["stack_sort"].as_str()),
            (Some("grid"), Some("name")),
            "the names the dock reads"
        );
        let view = view_of(Some(&o));
        assert_eq!((view.stack_view, view.stack_sort), (DockStackView::Grid, DockStackSort::Name));
        assert!(view.show_downloads, "shown unless turned off");
        write_change(&mut o, &DockChange::ShowDownloads(false));
        assert_eq!(o["show_downloads"], Value::Bool(false));
        assert!(!view_of(Some(&o)).show_downloads);
        assert!(view.music_pulse, "the music pulse is on unless turned off");
        write_change(&mut o, &DockChange::MusicPulse(false));
        assert_eq!(o["music_pulse"], false);
        assert!(!view_of(Some(&o)).music_pulse);
        write_change(&mut o, &DockChange::MusicPulse(true));
        assert!(!view.all_displays, "the main screen only, unless asked");
        write_change(&mut o, &DockChange::AllDisplays(true));
        assert_eq!(o["displays"], "all");
        assert!(view_of(Some(&o)).all_displays);
        write_change(&mut o, &DockChange::AllDisplays(false));
        assert_eq!(o["displays"], "primary");
        assert!(!view_of(Some(&o)).all_displays);
    }

    #[test]
    fn missing_or_older_settings_read_as_the_dock_defaults() {
        assert!(!view_of(None).found);
        let older: Map<String, Value> = serde_json::from_str(r#"{"launch_effect": false}"#).unwrap();
        let view = view_of(Some(&older));
        assert!(!view.launch_pop, "an older file's false means off");
        assert!(view.hide_in_fullscreen && view.replace_switcher && view.minimize_liquid);
        assert_eq!(view.minimize_duration, 0.4);
    }

    #[test]
    fn helper_sits_inside_the_app() {
        let exe = Path::new("/Applications/notchbase.app/Contents/MacOS/notchbase");
        let helper = helper_beside(exe).unwrap();
        assert_eq!(helper, Path::new("/Applications/notchbase.app/Contents/Helpers/notchbase dock.app"));
        assert_eq!(helper_beside(Path::new("/usr/local/bin/notchbase")), None, "a bare binary has no helper");
    }

    #[test]
    fn writes_go_through_and_read_back() {
        let dir = std::env::temp_dir().join(format!("notchbase-dock-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), r#"{"pinned": [], "show_names": false}"#).unwrap();
        let mut link = DockLink::new(dir.clone());
        assert!(link.view().found && !link.view().show_names);
        link.apply(&DockChange::ShowNames(true));
        link.apply(&DockChange::Glass(0.8));
        assert!(link.view().show_names);
        assert!((link.view().glass - 0.8).abs() < 1e-6);
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains("\"pinned\""));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
