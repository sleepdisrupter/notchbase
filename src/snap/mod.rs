//! Window layouts: drag a window to the notch and drop it on a layout. This holds the drag watcher, the zones, the
//! ghost that previews them and the habits learned per app, and the platform code that moves windows.

mod drag;
mod ghost;
mod habits;
mod zones;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as backend;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as backend;

use egui::{Pos2, Rect};
use winit::monitor::MonitorHandle;

pub use drag::{Drag, Probe, Watcher};
pub use ghost::{Ghost, Scene};
pub use habits::{Class, Habits};
pub use zones::{Choice, Zone, assign, free_space, tile};

/// A display in desktop coordinates, which are points on a Mac and pixels on Windows. `work` leaves out the menu
/// bar and taskbar, and `scale` turns points into these coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub key: u64,
    pub frame: Rect,
    pub work: Rect,
    pub scale: f32,
}

impl Screen {
    /// A gap given in points, in this screen's coordinates.
    pub fn gap(&self, points: f32) -> f32 {
        points * self.scale
    }

    /// A desktop rectangle in points from this screen's top left, for drawing on an overlay.
    pub fn local(&self, rect: Rect) -> Rect {
        Rect::from_min_max(
            ((rect.min - self.frame.min) / self.scale).to_pos2(),
            ((rect.max - self.frame.min) / self.scale).to_pos2(),
        )
    }
}

/// A window, by its id and its app's process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub id: u64,
    pub pid: u32,
}

/// A window and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grab {
    pub handle: Handle,
    pub frame: Rect,
}

struct Live;

impl Probe for Live {
    fn window_at(&self, point: Pos2) -> Option<Grab> {
        window_at(point)
    }

    fn frame_of(&self, handle: Handle) -> Option<Rect> {
        frame_of(handle)
    }

    fn fills_a_screen(&self, frame: Rect) -> bool {
        screens().iter().any(|screen| {
            (frame.width() - screen.work.width()).abs() < 12.0 && (frame.height() - screen.work.height()).abs() < 12.0
        })
    }
}

impl Watcher {
    /// Follows the real mouse. The pointer is only read while the button is down.
    pub fn watch(&mut self, down: bool) -> Drag {
        let cursor = if down { cursor() } else { None };
        self.poll(down, cursor, &Live)
    }
}

/// The app's process name, which its habits are kept under.
pub fn app_name(pid: u32) -> Option<String> {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.name().to_string_lossy().into_owned())
}

pub fn class_of(screen: &Screen) -> Class {
    if zones::is_ultrawide(screen.work) { Class::Wide } else { Class::Standard }
}

/// Whether notchbase may move other apps' windows. A Mac needs the Accessibility permission for that.
pub fn allowed() -> bool {
    backend::allowed()
}

pub fn ask_for_access() {
    backend::ask_for_access();
}

pub fn cursor() -> Option<Pos2> {
    backend::cursor()
}

pub fn screens() -> Vec<Screen> {
    clear_of_dock(backend::screens(), &crate::system::dock::reserved_bottoms())
}

/// Takes notchbase dock's strip off the bottom of each screen it's up on (display number, height in points), so
/// layouts end above it rather than under it. macOS leaves it in the visible frame: the system Dock it replaces is
/// hidden.
fn clear_of_dock(mut screens: Vec<Screen>, bottoms: &[(u64, f32)]) -> Vec<Screen> {
    for screen in &mut screens {
        if let Some((_, reserved)) = bottoms.iter().find(|(key, _)| *key == screen.key) {
            let limit = screen.frame.max.y - reserved * screen.scale;
            if limit > screen.work.min.y {
                screen.work.max.y = screen.work.max.y.min(limit);
            }
        }
    }
    screens
}

pub fn screen_of(monitor: &MonitorHandle) -> Option<Screen> {
    let key = backend::monitor_key(monitor);
    screens().into_iter().find(|screen| screen.key == key)
}

/// The next display from left to right, wrapping around, for holding Shift over a layout.
pub fn next_screen(screen: &Screen) -> Option<Screen> {
    let mut all = screens();
    all.sort_by(|a, b| a.frame.min.x.total_cmp(&b.frame.min.x).then(a.frame.min.y.total_cmp(&b.frame.min.y)));
    let here = all.iter().position(|other| other.key == screen.key)?;
    all.get((here + 1) % all.len()).copied().filter(|next| next.key != screen.key)
}

pub fn window_at(point: Pos2) -> Option<Grab> {
    backend::window_at(point)
}

pub fn frame_of(handle: Handle) -> Option<Rect> {
    backend::frame_of(handle)
}

pub fn windows_on(screen: &Screen) -> Vec<Grab> {
    backend::windows_on(screen)
}

pub fn place(handle: Handle, frame: Rect) -> Result<(), String> {
    backend::place(handle, frame)
}

/// Tiles every window on the screen, giving the dragged one the first slot and the rest the slots nearest them.
pub fn lay_out(dragged: Handle, screen: &Screen, gap: f32) -> Result<usize, String> {
    let mut others: Vec<Grab> = windows_on(screen).into_iter().filter(|grab| grab.handle != dragged).collect();
    others.sort_by(|a, b| {
        let (a, b) = (a.frame.center(), b.frame.center());
        a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
    });
    let slots = tile(others.len() + 1, screen.work, screen.gap(gap));
    let mut placed = 0;
    let mut first_problem = None;
    let mut record = |handle: Handle, rect: Rect| match place(handle, rect) {
        Ok(()) => placed += 1,
        Err(problem) => {
            first_problem.get_or_insert(problem);
        }
    };
    record(dragged, slots[0]);
    let current: Vec<Rect> = others.iter().map(|grab| grab.frame).collect();
    let wanted = assign(&slots[1..], &current);
    for (grab, slot) in others.iter().zip(wanted) {
        record(grab.handle, slots[1 + slot]);
    }
    match (placed, first_problem) {
        (0, Some(problem)) => Err(problem),
        _ => Ok(placed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn screen(key: u64, x: f32, menu: f32) -> Screen {
        let frame = Rect::from_min_size(pos2(x, 0.0), vec2(1470.0, 956.0));
        Screen { key, frame, work: Rect::from_min_max(pos2(x, menu), frame.max), scale: 1.0 }
    }

    #[test]
    fn layouts_end_above_the_dock_only_on_screens_it_is_up_on() {
        let screens = vec![screen(1, 0.0, 33.0), screen(2, 1470.0, 25.0)];
        let clear = clear_of_dock(screens.clone(), &[(1, 70.0)]);
        assert_eq!(clear[0].work.max.y, 886.0, "above the dock on its screen");
        assert_eq!(clear[0].work.min.y, 33.0, "below the menu bar as before");
        assert_eq!(clear[1].work, screens[1].work, "the other screen keeps all of it");
        let both = clear_of_dock(screens.clone(), &[(1, 70.0), (2, 70.0)]);
        assert!(both.iter().all(|s| s.work.max.y == 886.0), "a dock on each: both keep clear");
        assert_eq!(clear_of_dock(screens.clone(), &[]), screens, "no dock running: macOS's own areas");
        // A macOS Dock still showing already took its room: never more than the larger of the two.
        let mut shown = screens.clone();
        shown[0].work.max.y = 870.0;
        assert_eq!(clear_of_dock(shown, &[(1, 70.0)])[0].work.max.y, 870.0);
        // A filled window ends exactly at the dock's top: no gap, no overlap.
        let full = Zone::Full.rect(clear[0].work, 0.0);
        assert_eq!(full.max.y, 886.0);
    }
}
