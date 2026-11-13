//! The event loop on Windows: a dock window along the bottom of each screen (all of them with "All displays"), a
//! click-through overlay for windows flying down into the dock and back and for the launch effect, the pointer poll
//! that decides when a dock takes the mouse, and frames only while something moves.
//!
//! Unlike macOS, every screen's dock is live: the one under the pointer gets it, and each draws on its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Pos2, Rect, pos2, vec2};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, LWA_ALPHA, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows as _};
use winit::window::{WindowAttributes, WindowId, WindowLevel};

use super::os::{self, Screen};
use super::taskbar::{self, AppBar};
use super::{hooks, lifecycle, sys};
use crate::config::{Config, Displays, LaunchEffect, MinimizeEffect};
use crate::dock::{Dock, Effect, WINDOW_HEIGHT};
use crate::flight::{Flight, Way};
use crate::gl::GlWindow;
use crate::liquid::Liquid;
use crate::minimized::Minimized;
use crate::pop::{Pop, Reach};
use crate::windows::Win;

const IDLE_POLL: Duration = Duration::from_millis(120);
const NEAR_POLL: Duration = Duration::from_millis(16);
const AMBIENT_FRAME: Duration = Duration::from_millis(33);
const APPS_POLL: Duration = Duration::from_millis(1000);
const SCREEN_POLL: Duration = Duration::from_secs(2);
const TASKBAR_POLL: Duration = Duration::from_secs(2);
const TRASH_POLL: Duration = Duration::from_secs(3);
const CONFIG_POLL: Duration = Duration::from_millis(500);
const FULLSCREEN_POLL: Duration = Duration::from_millis(250);
const MINIMIZED_POLL: Duration = Duration::from_millis(400);
const WINDOW_LOOKUP: Duration = Duration::from_millis(25);
const EDGE_DWELL: Duration = Duration::from_millis(200);
const TUCK_AFTER: Duration = Duration::from_millis(500);
const SHORTEST_FRAME: Duration = Duration::from_micros(7_800);
/// How long a restored window gets to draw itself under its picture before the picture fades.
const PAINT_WAIT: Duration = Duration::from_millis(70);
/// How long a picture over a window's place waits for the window before it fades anyway.
const REVEAL_WAIT: f32 = 2.0;

struct DockWindow {
    gl: GlWindow,
    liquid: Liquid,
    hwnd: HWND,
    appbar: AppBar,
    /// Clicks go through to whatever is under it.
    click_through: bool,
}

/// The dock on one screen.
struct Seat {
    screen: Screen,
    dock: Dock,
    win: DockWindow,
    /// Slid down out of sight under a full-screen app.
    full: bool,
    revealed: bool,
    edge_since: Option<Instant>,
    left_since: Option<Instant>,
    pointer: Option<Pos2>,
}

impl Seat {
    /// The dock window's top left, in physical pixels.
    fn origin(&self) -> Pos2 {
        let s = &self.screen;
        pos2(s.frame.left as f32, s.frame.bottom as f32 - WINDOW_HEIGHT * s.scale)
    }

    /// A point in physical pixels as the dock's own points.
    fn local(&self, global: Pos2) -> Pos2 {
        ((global - self.origin()) / self.screen.scale).to_pos2()
    }

    /// A rect in the dock's points as the overlay's points (the overlay covers the whole screen).
    fn to_overlay(&self, r: Rect) -> Rect {
        let top = self.screen.frame.height() as f32 / self.screen.scale - WINDOW_HEIGHT;
        r.translate(vec2(0.0, top))
    }
}

struct Overlay {
    gl: GlWindow,
    liquid: Liquid,
    hwnd: HWND,
    /// The screen it covers now.
    screen: Option<Screen>,
    shown: bool,
}

/// A launch effect flowing from an icon into the app's window.
struct Run {
    pop: Pop,
    path: PathBuf,
    pid: Option<i32>,
    looked: Instant,
    screen: u32,
}

/// A window flying down into its tile or back up, on screen `screen`.
struct Flying {
    flight: Flight,
    screen: u32,
    pid: i32,
    /// Going up: the real window was asked back.
    asked: bool,
}

pub struct App {
    dir: PathBuf,
    seats: Vec<Seat>,
    overlay: Option<Overlay>,
    proxy: EventLoopProxy<()>,
    runs: Vec<Run>,
    flights: Vec<Flying>,
    /// Each window's picture as it went down, to fly it back up.
    pictures: HashMap<u32, egui::ColorImage>,
    /// Each app's window frame from last time, so a launch flies straight to where its window will be.
    windows: HashMap<PathBuf, Rect>,
    minimized: Arc<Mutex<Option<Vec<Minimized>>>>,
    last_minimized: Option<Vec<Minimized>>,
    downloads: Option<crate::downloads::Watch>,
    pulse: crate::pulse::Pulse,
    recent: Vec<i32>,
    watches: HashMap<PathBuf, crate::stack::Watch>,
    listings: HashMap<PathBuf, crate::stack::Listing>,
    file_pictures: Option<crate::stack::Pictures>,
    config_checked: Instant,
    config_stamp: Option<std::time::SystemTime>,
    last_apps: Instant,
    last_screen: Instant,
    last_taskbar: Instant,
    last_trash: Instant,
    last_fullscreen: Instant,
    apps_changed: Arc<AtomicBool>,
    hid_taskbar: bool,
    keep_taskbar: bool,
    /// Windows' own minimize animation, as the dock last set it.
    animating: Option<bool>,
    needs_frame: bool,
    idle: bool,
    last_frame: Instant,
    config: Config,
    failed: Option<String>,
}

pub fn run(dir: PathBuf) -> Result<(), String> {
    let event_loop = EventLoop::<()>::with_user_event().build().map_err(|e| e.to_string())?;
    let config = Config::load_or_seed(&dir, seed_from_system_dock);
    let mut app = App {
        dir: dir.clone(),
        seats: Vec::new(),
        overlay: None,
        proxy: event_loop.create_proxy(),
        runs: Vec::new(),
        flights: Vec::new(),
        pictures: HashMap::new(),
        windows: HashMap::new(),
        minimized: Arc::default(),
        last_minimized: None,
        downloads: crate::downloads::Watch::start(),
        pulse: crate::pulse::Pulse::start(),
        recent: Vec::new(),
        watches: HashMap::new(),
        listings: HashMap::new(),
        file_pictures: None,
        config_checked: Instant::now(),
        config_stamp: None,
        last_apps: Instant::now(),
        last_screen: Instant::now(),
        last_taskbar: Instant::now(),
        last_trash: Instant::now() - Duration::from_secs(60),
        last_fullscreen: Instant::now(),
        apps_changed: Arc::new(AtomicBool::new(true)),
        hid_taskbar: false,
        keep_taskbar: std::env::var_os("NOTCHBASE_DOCK_KEEP_TASKBAR").is_some(),
        animating: None,
        needs_frame: true,
        idle: true,
        last_frame: Instant::now(),
        config,
        failed: None,
    };
    let result = event_loop.run_app(&mut app).map_err(|e| e.to_string());
    app.shutdown();
    match app.failed.take() {
        Some(err) => Err(err),
        None => result,
    }
}

/// The first run's dock: the apps pinned to the taskbar, in its order.
pub fn seed_from_system_dock() -> Option<Config> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from)?;
    let folder = appdata.join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar");
    let mut links: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")))
        .collect();
    links.sort();
    let mut pinned: Vec<PathBuf> = Vec::new();
    for link in links {
        if let Some(target) = super::shell::shortcut_target(&link).filter(|t| os::is_app(t) && t.exists())
            && !pinned.contains(&target)
            && !is_explorer(&target)
        {
            pinned.push(target);
        }
    }
    Some(Config { pinned, ..Config::default() })
}

fn is_explorer(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("explorer.exe"))
}

/// A dropped or dragged-in shortcut stands for the program it opens.
fn resolved(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        && let Some(target) = super::shell::shortcut_target(&path).filter(|t| os::is_app(t))
    {
        return target;
    }
    path
}

fn hwnd_of(window: &winit::window::Window) -> HWND {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as HWND,
        _ => std::ptr::null_mut(),
    }
}

fn set_ex_style(hwnd: HWND, add: u32, remove: u32) {
    let old = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let new = (old | add) & !remove;
    if new != old {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new as isize);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
    // A newly layered window is set opaque, so GL's own alpha still applies.
    if new & WS_EX_LAYERED != 0 && old & WS_EX_LAYERED == 0 {
        unsafe { SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) };
    }
}

fn on_top(hwnd: HWND) {
    unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
}

fn attributes(title: &str, position: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_skip_taskbar(true)
        .with_undecorated_shadow(false)
        .with_corner_preference(CornerPreference::DoNotRound)
        .with_inner_size(size)
        .with_position(position)
}

/// The dock window's place on a screen: along its bottom, the screen's width.
fn dock_place(screen: &Screen) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let height = (WINDOW_HEIGHT * screen.scale).round() as i32;
    (
        PhysicalPosition::new(screen.frame.left, screen.frame.bottom - height),
        PhysicalSize::new(screen.frame.width().max(1) as u32, height.max(1) as u32),
    )
}

fn open_dock_window(event_loop: &ActiveEventLoop, screen: &Screen) -> Result<DockWindow, String> {
    let (position, size) = dock_place(screen);
    let gl = GlWindow::new(event_loop, attributes("notchbase dock", position, size), false)?;
    let hwnd = hwnd_of(&gl.window);
    set_ex_style(hwnd, WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT, WS_EX_APPWINDOW);
    gl.window.set_outer_position(position);
    let _ = gl.window.request_inner_size(size);
    let liquid = Liquid::new(&gl.gl, gl.shader_version);
    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    on_top(hwnd);
    Ok(DockWindow { gl, liquid, hwnd, appbar: AppBar::new(hwnd), click_through: true })
}

fn mods_held() -> crate::dock::Mods {
    let held = |key: u16| unsafe { GetAsyncKeyState(i32::from(key)) } < 0;
    crate::dock::Mods { command: held(VK_CONTROL), shift: held(VK_SHIFT) }
}

fn rect_px(r: os::RectPx) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

impl App {
    fn create(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if !self.keep_taskbar {
            // A dock that was killed left its save behind: put that back first, so the saved values stay the
            // person's own.
            let _ = taskbar::restore(&self.dir);
            taskbar::hide(&self.dir).map_err(|e| format!("could not hide the taskbar: {e}"))?;
            self.hid_taskbar = true;
        }
        self.reposition(event_loop);
        if self.seats.is_empty() {
            return Err("no screen to show the dock on".into());
        }
        let first = self.seats[0].screen.clone();
        let (position, size) = (
            PhysicalPosition::new(first.frame.left, first.frame.top),
            PhysicalSize::new(first.frame.width().max(1) as u32, first.frame.height().max(1) as u32),
        );
        let mut gl = GlWindow::new(event_loop, attributes("notchbase dock flights", position, size), false)?;
        let hwnd = hwnd_of(&gl.window);
        set_ex_style(hwnd, WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT, WS_EX_APPWINDOW);
        let liquid = Liquid::new(&gl.gl, gl.shader_version);
        gl.frame(|_, _, _| {}, |_| {}, None, None);
        gl.present();
        self.overlay = Some(Overlay { gl, liquid, hwnd, screen: Some(first), shown: false });

        let proxy = self.proxy.clone();
        let changed = Arc::clone(&self.apps_changed);
        hooks::on_wake(move || {
            changed.store(true, Ordering::Relaxed);
            let _ = proxy.send_event(());
        });
        let proxy = self.proxy.clone();
        crate::stack::on_keys_wake(move || {
            let _ = proxy.send_event(());
        });
        hooks::start();

        let found = Arc::clone(&self.minimized);
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("minimized windows".into()).spawn(move || {
            let mut watch = crate::minimized::Watch::default();
            let mut last = Vec::new();
            loop {
                let now = watch.poll();
                if now != last {
                    last.clone_from(&now);
                    if let Ok(mut slot) = found.lock() {
                        *slot = Some(now);
                    }
                    if proxy.send_event(()).is_err() {
                        return;
                    }
                }
                std::thread::sleep(MINIMIZED_POLL);
            }
        });
        Ok(())
    }

    fn shutdown(&mut self) {
        hooks::catch_minimize(false);
        for seat in &mut self.seats {
            seat.win.appbar.release();
        }
        if self.hid_taskbar {
            let _ = taskbar::restore(&self.dir);
            self.hid_taskbar = false;
        }
        lifecycle::release(&self.dir);
        self.overlay = None;
        self.seats.clear();
    }

    /// Keeps a dock on each screen that should have one, as screens come, go, move and change scale.
    fn reposition(&mut self, event_loop: &ActiveEventLoop) {
        let screens = os::screens();
        let wanted: Vec<Screen> = match self.config.displays {
            Displays::All => screens,
            Displays::Primary => screens.into_iter().take(1).collect(),
        };
        self.seats.retain(|s| wanted.iter().any(|w| w.id == s.screen.id));
        for w in &wanted {
            if let Some(seat) = self.seats.iter_mut().find(|s| s.screen.id == w.id) {
                if seat.screen != *w {
                    seat.screen = w.clone();
                    let (position, size) = dock_place(w);
                    seat.win.gl.window.set_outer_position(position);
                    let _ = seat.win.gl.window.request_inner_size(size);
                    seat.win.gl.resized();
                    seat.dock.size = seat.win.gl.size();
                    seat.win.appbar.refresh();
                    self.needs_frame = true;
                }
                continue;
            }
            match self.open_seat(event_loop, w) {
                Ok(seat) => self.seats.push(seat),
                Err(err) => eprintln!("notchbase-dock: no dock on screen {}: {err}", w.id),
            }
        }
        // The primary screen's dock first.
        self.seats.sort_by_key(|s| wanted.iter().position(|w| w.id == s.screen.id));
    }

    fn open_seat(&mut self, event_loop: &ActiveEventLoop, screen: &Screen) -> Result<Seat, String> {
        let win = open_dock_window(event_loop, screen)?;
        let mut dock = Dock::new(self.config.clone(), self.dir.clone(), win.gl.size()).live();
        dock.dark = os::dark_mode();
        dock.trash_full = os::trash_is_full();
        if let Some(m) = &self.last_minimized {
            dock.set_minimized(m.clone());
        }
        dock.refresh(&os::running_apps());
        for (folder, listing) in &self.listings {
            dock.set_listing(folder.clone(), listing.clone());
        }
        self.needs_frame = true;
        Ok(Seat {
            screen: screen.clone(),
            dock,
            win,
            full: false,
            revealed: false,
            edge_since: None,
            left_since: None,
            pointer: None,
        })
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        if lifecycle::quit_requested() {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        self.follow_config(now);
        self.follow_minimizing();
        for (pid, id) in hooks::take_minimizes() {
            self.start_minimize(pid, id);
        }
        if hooks::take_pressed_elsewhere() {
            for seat in &mut self.seats {
                if seat.dock.popup_open() {
                    seat.dock.close_popup();
                    self.needs_frame = true;
                }
            }
        }
        let minimized = self.minimized.lock().ok().and_then(|mut slot| slot.take());
        let mut windows_changed = false;
        if let Some(m) = &minimized {
            self.last_minimized = Some(m.clone());
            for seat in &mut self.seats {
                windows_changed |= seat.dock.set_minimized(m.clone());
            }
        }
        if windows_changed || self.apps_changed.swap(false, Ordering::Relaxed) || now - self.last_apps > APPS_POLL {
            self.last_apps = now;
            let running = os::running_apps();
            for seat in &mut self.seats {
                seat.dock.refresh(&running);
            }
            self.note_front();
            self.needs_frame = true;
        }
        if now - self.last_trash > TRASH_POLL {
            self.last_trash = now;
            let full = os::trash_is_full();
            let dark = os::dark_mode();
            for seat in &mut self.seats {
                if seat.dock.trash_full != full || seat.dock.dark != dark {
                    seat.dock.trash_full = full;
                    seat.dock.dark = dark;
                    self.needs_frame = true;
                }
            }
        }
        if now - self.last_screen > SCREEN_POLL {
            self.last_screen = now;
            self.reposition(event_loop);
        }
        if self.hid_taskbar && now - self.last_taskbar > TASKBAR_POLL {
            self.last_taskbar = now;
            taskbar::keep_hidden();
        }
        self.reserve_space();
        self.step_ambient();
        self.follow_stacks();
        self.step_stack_keys();
        if now - self.last_fullscreen > FULLSCREEN_POLL {
            self.last_fullscreen = now;
            self.step_fullscreen(now);
        }
        self.step_pointer(now);

        let moving = self.needs_frame
            || self.seats.iter().any(|s| s.dock.animating())
            || !self.runs.is_empty()
            || !self.flights.is_empty();
        if moving {
            self.render();
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if self.seats.iter().any(|s| s.dock.ambient()) {
            self.render();
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + AMBIENT_FRAME));
        } else {
            self.idle = true;
            let near = self.seats.iter().any(|s| s.pointer.is_some_and(|p| p.y > s.dock.size.y - 220.0));
            let edge = self.seats.iter().any(|s| s.full && !s.revealed);
            let wait = if near || edge { NEAR_POLL } else { IDLE_POLL };
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + wait));
        }
    }

    /// The minimize effect follows the setting: drawn by the dock (Windows' own animation off), or Windows' own.
    fn follow_minimizing(&mut self) {
        let liquid = self.config.minimize_effect == MinimizeEffect::Liquid && self.hid_taskbar;
        hooks::catch_minimize(liquid);
        let animate = if liquid { false } else { taskbar::saved_min_animate(&self.dir) };
        if self.hid_taskbar && self.animating != Some(animate) {
            self.animating = Some(animate);
            std::thread::spawn(move || taskbar::set_min_animate(animate));
        }
    }

    /// Keeps the strip each dock stands on from maximized windows, as the taskbar would.
    fn reserve_space(&mut self) {
        let avoid = self.config.avoid_windows;
        for seat in &mut self.seats {
            let height =
                if avoid && !seat.full { (seat.dock.reserved_height() * seat.screen.scale).round() as i32 } else { 0 };
            seat.win.appbar.reserve(rect_px(seat.screen.frame), height);
        }
    }

    /// Who the pointer is over, which dock takes clicks, and releases the window never heard.
    fn step_pointer(&mut self, now: Instant) {
        let global = os::pointer();
        let held = os::mouse_buttons_held();
        for seat in &mut self.seats {
            let local = seat.local(global);
            let inside = Rect::from_min_size(Pos2::ZERO, seat.dock.size).contains(local);
            let pointer = inside.then_some(local);
            if pointer != seat.pointer {
                seat.pointer = pointer;
                seat.dock.set_pointer(pointer);
                self.needs_frame = true;
            }
            if !held && seat.dock.interactive() && pointer.is_none() {
                seat.dock.release();
            }
            // Over a full-screen app: resting on the bottom edge brings the dock up, leaving sends it down.
            if seat.full {
                let screen = seat.screen.frame;
                if !seat.revealed {
                    let on_edge = screen.contains(global) && global.y >= screen.bottom as f32 - 2.0 && !held;
                    if !on_edge {
                        seat.edge_since = None;
                    } else if now - *seat.edge_since.get_or_insert(now) >= EDGE_DWELL {
                        seat.revealed = true;
                        seat.left_since = None;
                        seat.dock.set_tucked(false);
                        self.needs_frame = true;
                    }
                } else {
                    let near = pointer.is_some_and(|p| p.y > seat.dock.size.y - seat.dock.reserved_height() - 40.0);
                    let busy = seat.dock.popup_open() || seat.dock.interactive();
                    if near || busy {
                        seat.left_since = None;
                    } else if now - *seat.left_since.get_or_insert(now) >= TUCK_AFTER {
                        seat.revealed = false;
                        seat.edge_since = None;
                        seat.dock.set_tucked(true);
                        self.needs_frame = true;
                    }
                }
            }
            // The window takes the mouse only over the dock itself, and keeps it while files are dragged out.
            let takes = seat.dock.interactive() || super::shell::dragging();
            if takes == seat.win.click_through {
                seat.win.click_through = !takes;
                let (add, remove) = if takes { (0, WS_EX_TRANSPARENT) } else { (WS_EX_TRANSPARENT, 0) };
                set_ex_style(seat.win.hwnd, add, remove);
            }
        }
        if super::shell::take_ended() {
            for seat in &mut self.seats {
                seat.dock.file_drag_ended();
            }
            self.needs_frame = true;
        }
    }

    fn step_fullscreen(&mut self, now: Instant) {
        let hide = self.config.hide_in_fullscreen;
        for seat in &mut self.seats {
            let full = hide && crate::windows::fullscreen_on(seat.screen.frame);
            if full != seat.full {
                seat.full = full;
                seat.revealed = false;
                seat.edge_since = None;
                seat.left_since = Some(now);
                seat.dock.set_tucked(full);
                self.needs_frame = true;
            }
        }
    }

    /// Keeps the order apps were last in front, for the download bar's browser.
    fn note_front(&mut self) {
        if let Some(front) = os::frontmost_pid().filter(|p| *p != std::process::id() as i32)
            && self.recent.first() != Some(&front)
        {
            self.recent.retain(|p| *p != front);
            self.recent.insert(0, front);
            self.recent.truncate(32);
        }
    }

    fn step_ambient(&mut self) {
        let fetching = self.downloads.as_ref().map(|w| w.now()).unwrap_or_default();
        let time = self.seats.first().map_or(0.0, |s| s.dock.time);
        let beat = self.pulse.step(self.config.music_pulse, time);
        for seat in &mut self.seats {
            seat.dock.set_downloads(&fetching, &self.recent);
            seat.dock.set_beat(beat);
        }
    }

    fn step_stack_keys(&mut self) {
        let wants = self.seats.iter().position(|s| s.dock.stack_wants_keys());
        crate::stack::catch_keys(wants.is_some());
        for seat in &mut self.seats {
            seat.dock.stack_keys = wants.is_some();
        }
        for key in crate::stack::take_keys() {
            if let Some(i) = wants {
                self.seats[i].dock.stack_key(key);
            }
            self.needs_frame = true;
        }
    }

    /// Keeps a watch on every folder in the docks and hands them what changed, and the pictures of their files.
    fn follow_stacks(&mut self) {
        let mut folders: Vec<PathBuf> = Vec::new();
        for seat in &self.seats {
            for f in seat.dock.stack_folders() {
                if !folders.contains(&f) {
                    folders.push(f);
                }
            }
        }
        if folders.len() != self.watches.len() || folders.iter().any(|f| !self.watches.contains_key(f)) {
            self.watches.retain(|f, _| folders.contains(f));
            for folder in folders {
                let proxy = self.proxy.clone();
                self.watches.entry(folder.clone()).or_insert_with(|| {
                    crate::stack::Watch::start(folder, move || {
                        let _ = proxy.send_event(());
                    })
                });
            }
        }
        for (folder, watch) in &self.watches {
            if let Some(listing) = watch.take() {
                for seat in &mut self.seats {
                    seat.dock.set_listing(folder.clone(), listing.clone());
                }
                self.listings.insert(folder.clone(), listing);
                self.needs_frame = true;
            }
        }
        self.listings.retain(|f, _| self.watches.contains_key(f));
        let mut wants = Vec::new();
        let mut keep: Option<std::collections::HashSet<PathBuf>> = None;
        for seat in &mut self.seats {
            if let Some(k) = seat.dock.take_picture_keep() {
                keep.get_or_insert_with(Default::default).extend(k);
            }
            wants.extend(seat.dock.take_picture_wants());
        }
        if let Some(keep) = keep
            && let Some(pictures) = &self.file_pictures
        {
            pictures.keep(keep);
        }
        if !wants.is_empty() {
            let proxy = self.proxy.clone();
            let pictures = self.file_pictures.get_or_insert_with(|| {
                crate::stack::Pictures::start(move || {
                    let _ = proxy.send_event(());
                })
            });
            for path in wants {
                pictures.ask(path);
            }
        }
        if let Some(pictures) = &self.file_pictures {
            let came = pictures.take();
            if !came.is_empty() {
                for seat in &mut self.seats {
                    let copies = came
                        .iter()
                        .map(|p| crate::stack::Picture { path: p.path.clone(), image: p.image.clone(), thumb: p.thumb })
                        .collect();
                    seat.dock.give_pictures(copies);
                }
                self.needs_frame = true;
            }
        }
    }

    /// Picks up `dock.json` changed by someone else (notchbase's Dock settings).
    fn follow_config(&mut self, now: Instant) {
        if now - self.config_checked < CONFIG_POLL {
            return;
        }
        self.config_checked = now;
        let stamp = std::fs::metadata(Config::path(&self.dir)).and_then(|m| m.modified()).ok();
        // A dock changes the file itself (pinning, resizing): take that on into the others.
        let mine = self.seats.iter().map(|s| &s.dock.config).find(|c| **c != self.config).cloned();
        if stamp == self.config_stamp && mine.is_none() {
            return;
        }
        let first = self.config_stamp.is_none();
        self.config_stamp = stamp;
        let config = match mine {
            Some(c) => c,
            None if first => return,
            None => match Config::read(&self.dir) {
                Some(c) => c,
                None => return,
            },
        };
        if config == self.config {
            return;
        }
        let displays = config.displays != self.config.displays;
        self.config = config.clone();
        let running = os::running_apps();
        for seat in &mut self.seats {
            if seat.dock.config != config {
                seat.dock.take_config(config.clone());
                seat.dock.refresh(&running);
            }
        }
        if displays {
            self.last_screen = now - SCREEN_POLL * 2;
        }
        self.needs_frame = true;
    }

    /// The seat on the screen holding `at` (physical pixels), else the primary screen's.
    fn seat_at(&self, at: Pos2) -> Option<usize> {
        self.seats.iter().position(|s| s.screen.frame.contains(at)).or((!self.seats.is_empty()).then_some(0))
    }

    /// A window on its way down: Windows' own animation is off, so it's already gone. Its last picture flies from
    /// where it was into its new tile.
    fn start_minimize(&mut self, pid: i32, id: u32) {
        if self.flights.iter().any(|f| f.flight.id == id) {
            return;
        }
        let hwnd = sys::hwnd_of(id);
        let kept = hooks::last_picture(id);
        let placed = sys::placement(hwnd);
        let frame = match (&kept, placed) {
            (Some((frame, _)), _) => *frame,
            (None, Some((normal, _))) => sys::rect_of(normal),
            (None, None) => return,
        };
        let Some(i) = self.seat_at(frame.center()) else { return };
        let picture = kept.map(|(_, image)| image);
        if let Some(p) = &picture {
            self.pictures.insert(id, p.clone());
        }
        let win = Win { id, title: sys::title_of(hwnd), frame, on_screen: false };
        let small = picture.as_ref().map(|p| crate::windows::smaller(p, 256));
        for (n, seat) in self.seats.iter_mut().enumerate() {
            // Only the dock it flies into veils the tile until it lands.
            seat.dock.expect_minimized(Minimized { pid, win: win.clone() }, small.clone());
            if n != i {
                seat.dock.unveil(id);
            }
        }
        let seat = &self.seats[i];
        let window = self.to_points(i, frame);
        let tile = self.tile_rect(i, id);
        let flight = Flight::new(Way::Down, id, window, tile, picture).lasting(self.config.minimize_duration);
        self.flights.push(Flying { flight, screen: seat.screen.id, pid, asked: false });
        self.needs_frame = true;
    }

    /// A minimized window's tile clicked: its picture flies up from the tile to where the window goes back to, and
    /// the window comes back under it.
    fn start_restore(&mut self, seat: usize, pid: i32, id: u32, icon: Rect) {
        if self.flights.iter().any(|f| f.flight.id == id) {
            return;
        }
        let hwnd = sys::hwnd_of(id);
        if self.config.minimize_effect != MinimizeEffect::Liquid {
            os::bring_forward(hwnd);
            return;
        }
        let Some((normal, maximized)) = sys::placement(hwnd) else {
            os::bring_forward(hwnd);
            return;
        };
        let mut target = sys::rect_of(normal);
        // Placements are in work-area coordinates, which start where the primary screen's work area does.
        if let Some(primary) = self.seats.first() {
            let shift = vec2(primary.screen.work.left as f32, primary.screen.work.top as f32)
                - vec2(primary.screen.frame.left as f32, primary.screen.frame.top as f32);
            target = target.translate(shift);
        }
        if maximized && let Some(s) = self.seats.iter().find(|s| s.screen.frame.contains(target.center())) {
            target = s.screen.work.rect();
        }
        let window_seat = self.seats.iter().position(|s| s.screen.frame.contains(target.center())).unwrap_or(seat);
        let picture = self
            .pictures
            .get(&id)
            .cloned()
            .or_else(|| hooks::last_picture(id).map(|(_, image)| image))
            .or_else(|| self.seats[seat].dock.window_picture(id));
        let Some(picture) = picture else {
            os::bring_forward(hwnd);
            return;
        };
        // The overlay covers one screen: a window coming back on another screen flies from its own dock there.
        let from =
            if window_seat == seat { self.seats[seat].to_overlay(icon) } else { self.tile_rect(window_seat, id) };
        let window = self.to_points(window_seat, target);
        let mut flight = Flight::new(Way::Up, id, window, from, Some(picture)).lasting(self.config.minimize_duration);
        flight.ready = Some(Arc::new(AtomicBool::new(false)));
        self.flights.push(Flying { flight, screen: self.seats[window_seat].screen.id, pid, asked: false });
        self.needs_frame = true;
    }

    /// A rect in physical pixels as overlay points on seat `i`'s screen.
    fn to_points(&self, i: usize, r: Rect) -> Rect {
        let s = &self.seats[i].screen;
        let origin = vec2(s.frame.left as f32, s.frame.top as f32);
        Rect::from_min_max((r.min - origin) / s.scale, (r.max - origin) / s.scale)
    }

    /// A window's tile on seat `i`, in overlay points, or where a new one would be.
    fn tile_rect(&self, i: usize, id: u32) -> Rect {
        let seat = &self.seats[i];
        match seat.dock.window_tile(id) {
            Some(r) => seat.to_overlay(r),
            None => {
                let size = self.config.tile_size;
                let s = &seat.screen;
                let (w, h) = (s.frame.width() as f32 / s.scale, s.frame.height() as f32 / s.scale);
                Rect::from_center_size(pos2(w * 0.5, h - size), vec2(size, size))
            }
        }
    }

    fn step_flights(&mut self, dt: f32) {
        let mut flights = std::mem::take(&mut self.flights);
        for f in &mut flights {
            let seat = self.seats.iter().position(|s| s.screen.id == f.screen);
            if f.flight.way == Way::Down
                && let Some(i) = seat
            {
                f.flight.tile = self.tile_rect(i, f.flight.id);
            }
            f.flight.step(dt);
            if f.flight.way == Way::Down && f.flight.arrived() {
                for seat in &mut self.seats {
                    seat.dock.unveil(f.flight.id);
                }
            }
            if f.flight.way == Way::Up && f.flight.arrived() && !f.asked {
                // Over the window's place: the window comes back under the picture, then the picture fades.
                f.asked = true;
                let id = f.flight.id;
                let ready = f.flight.ready.clone();
                std::thread::spawn(move || {
                    os::bring_forward(sys::hwnd_of(id));
                    std::thread::sleep(PAINT_WAIT);
                    if let Some(r) = ready {
                        r.store(true, Ordering::Relaxed);
                    }
                });
            }
            if f.flight.way == Way::Up && f.flight.arrived() && !f.flight.revealing {
                let ready = f.flight.ready.as_ref().is_none_or(|r| r.load(Ordering::Relaxed));
                if ready || f.flight.waited() > REVEAL_WAIT {
                    f.flight.revealing = true;
                }
            }
        }
        for f in flights.iter().filter(|f| f.flight.finished() && f.flight.way == Way::Up) {
            self.pictures.remove(&f.flight.id);
            hooks::forget_picture(f.flight.id);
            let _ = f.pid;
        }
        flights.retain(|f| !f.flight.finished());
        self.flights = flights;
    }

    fn start_pop(&mut self, seat: usize, path: PathBuf, pid: Option<i32>, icon: Rect) {
        let icon = self.seats[seat].to_overlay(icon);
        let screen = self.seats[seat].screen.frame.rect();
        let found = pid.and_then(os::front_window_frame).filter(|f| screen.intersects(*f));
        let reach = if pid.is_some() { Reach::Switch } else { Reach::Launch };
        let (target, known) = match found {
            Some(frame) => (self.to_points(seat, frame), true),
            None => (
                self.windows.get(&path).copied().unwrap_or_else(|| {
                    let s = &self.seats[seat].screen;
                    let (w, h) = (s.frame.width() as f32 / s.scale, s.frame.height() as f32 / s.scale);
                    Rect::from_center_size(pos2(w * 0.5, h * 0.47), vec2(w * 0.56, h * 0.6))
                }),
                false,
            ),
        };
        self.runs.retain(|r| r.path != path);
        let screen = self.seats[seat].screen.id;
        self.runs.push(Run { pop: Pop::new(reach, icon, target, known), path, pid, looked: Instant::now(), screen });
    }

    fn step_runs(&mut self, dt: f32) {
        let now = Instant::now();
        let mut running = None;
        let mut runs = std::mem::take(&mut self.runs);
        for r in &mut runs {
            if now - r.looked >= WINDOW_LOOKUP {
                r.looked = now;
                if r.pid.is_none() {
                    let list = running.get_or_insert_with(os::running_apps);
                    r.pid = list.iter().find(|a| a.path.as_deref() == Some(r.path.as_path())).map(|a| a.pid);
                }
                if let Some(i) = self.seats.iter().position(|s| s.screen.id == r.screen)
                    && let Some(frame) = r.pid.and_then(os::front_window_frame)
                    && self.seats[i].screen.frame.rect().intersects(frame)
                {
                    let frame = self.to_points(i, frame);
                    r.pop.window_at(frame);
                    self.windows.insert(r.path.clone(), frame);
                }
            }
            r.pop.step(dt);
        }
        runs.retain(|r| !r.pop.finished());
        self.runs = runs;
    }

    fn handle_effects(&mut self) {
        for i in 0..self.seats.len() {
            for effect in std::mem::take(&mut self.seats[i].dock.effects) {
                match effect {
                    Effect::Launched { path, pid, icon } => {
                        if self.config.launch_effect_for(&path) == LaunchEffect::Window {
                            self.start_pop(i, path, pid, icon);
                        }
                    }
                    Effect::Restore { pid, id, icon } => self.start_restore(i, pid, id, icon),
                    Effect::DragFiles(paths) => {
                        if let Err(err) = super::files::drag(&self.seats[i].win.gl.window, &paths) {
                            eprintln!("notchbase-dock: drag failed: {err}");
                            self.seats[i].dock.file_drag_ended();
                        }
                    }
                    // Windows has no Quick Look: Space in a stack does nothing.
                    Effect::QuickLook(_) => {}
                }
            }
        }
    }

    fn render(&mut self) {
        let since = self.last_frame.elapsed();
        if since < SHORTEST_FRAME {
            std::thread::sleep(SHORTEST_FRAME - since);
        }
        let now = Instant::now();
        let dt = if std::mem::take(&mut self.idle) {
            SHORTEST_FRAME.as_secs_f32()
        } else {
            (now - self.last_frame).as_secs_f32().min(1.0 / 20.0)
        };
        self.last_frame = now;
        let all = std::mem::take(&mut self.needs_frame);
        let option = os::option_held();
        for seat in &mut self.seats {
            seat.dock.option_held = option;
            seat.dock.step(dt);
        }
        self.handle_effects();
        self.step_runs(dt);
        self.step_flights(dt);
        self.draw_overlay();
        for seat in &mut self.seats {
            draw_dock(&mut seat.win, &mut seat.dock);
        }
        let _ = all;
        unsafe { windows_sys::Win32::Graphics::Dwm::DwmFlush() };
    }

    /// Flights and launch effects, on the overlay over their screen. It's shown only while something's on it.
    fn draw_overlay(&mut self) {
        let Some(o) = &mut self.overlay else { return };
        let busy = self.flights.first().map(|f| f.screen).or_else(|| self.runs.first().map(|r| r.screen));
        if busy.is_none() && !o.shown {
            return;
        }
        if let Some(id) = busy
            && o.screen.as_ref().is_none_or(|s| s.id != id)
            && let Some(seat) = self.seats.iter().find(|s| s.screen.id == id)
        {
            let s = &seat.screen;
            o.gl.window.set_outer_position(PhysicalPosition::new(s.frame.left, s.frame.top));
            let _ = o.gl.window.request_inner_size(PhysicalSize::new(s.frame.width() as u32, s.frame.height() as u32));
            o.gl.resized();
            o.screen = Some(s.clone());
        }
        let here = o.screen.as_ref().map(|s| s.id);
        let flights: Vec<&mut Flying> = self.flights.iter_mut().filter(|f| Some(f.screen) == here).collect();
        let mut pops: Vec<(Vec<crate::liquid::Blob>, f32)> =
            self.runs.iter().filter(|r| Some(r.screen) == here).map(|r| (r.pop.blobs(), r.pop.alpha())).collect();
        for f in &flights {
            pops.push((f.flight.blobs(), f.flight.glass_alpha()));
        }
        let liquid = &o.liquid;
        let draw = |gl: &glow::Context, size, ppp| {
            for (blobs, alpha) in &pops {
                liquid.draw(gl, size, ppp, blobs, *alpha, None, None);
            }
        };
        let mut flights = flights;
        let pictures = |ui: &mut egui::Ui| {
            let painter = ui.painter().clone().with_clip_rect(Rect::EVERYTHING);
            for f in flights.iter_mut() {
                let f = &mut f.flight;
                if f.texture.is_none()
                    && let Some(picture) = f.picture.clone()
                {
                    let options = egui::TextureOptions {
                        magnification: egui::TextureFilter::Linear,
                        minification: egui::TextureFilter::Linear,
                        wrap_mode: egui::TextureWrapMode::ClampToEdge,
                        mipmap_mode: Some(egui::TextureFilter::Linear),
                    };
                    f.texture = Some(painter.ctx().load_texture(format!("flight {}", f.id), picture, options));
                }
                let alpha = f.picture_alpha();
                if let Some(texture) = &f.texture
                    && alpha > 0.0
                {
                    f.paint(&painter, texture.id(), alpha);
                }
            }
        };
        o.gl.frame(draw, pictures, None, None);
        o.gl.present();
        let showing = busy.is_some();
        if showing != o.shown {
            o.shown = showing;
            unsafe { ShowWindow(o.hwnd, if showing { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
            if showing {
                // Under the docks, over everything else.
                on_top(o.hwnd);
                for seat in &self.seats {
                    on_top(seat.win.hwnd);
                }
            }
        }
    }

    fn seat_of(&self, id: WindowId) -> Option<usize> {
        self.seats.iter().position(|s| s.win.gl.window.id() == id)
    }
}

/// Draws one frame of `dock` into its window.
fn draw_dock(w: &mut DockWindow, dock: &mut Dock) {
    let blobs = std::cell::RefCell::new(Vec::new());
    let liquid = &w.liquid;
    let opacity = std::cell::Cell::new(1.0);
    let band = std::cell::Cell::new(dock.glass_band());
    w.gl.frame(
        |gl, size, p| liquid.draw(gl, size, p, &blobs.borrow(), opacity.get(), Some(band.get()), None),
        |ui| {
            *blobs.borrow_mut() = dock.compose(ui.painter());
            opacity.set(dock.opacity());
            band.set(dock.glass_band());
        },
        None,
        None,
    );
    w.gl.present();
    let wants = dock.over_separator;
    let icon = if wants { winit::window::CursorIcon::NsResize } else { winit::window::CursorIcon::Default };
    w.gl.window.set_cursor(icon);
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.seats.is_empty() {
            return;
        }
        if let Err(err) = self.create(event_loop) {
            self.failed = Some(err);
            event_loop.exit();
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, (): ()) {
        self.needs_frame = true;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
            return;
        }
        let Some(i) = self.seat_of(id) else {
            if let (WindowEvent::Resized(_), Some(o)) = (&event, &self.overlay)
                && o.gl.window.id() == id
            {
                o.gl.resized();
            }
            return;
        };
        let seat = &mut self.seats[i];
        match event {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                seat.win.gl.resized();
                seat.dock.size = seat.win.gl.size();
                self.last_screen = Instant::now() - SCREEN_POLL * 2;
            }
            // A press dragged far enough out of a stack starts the file drag here.
            WindowEvent::CursorMoved { position, .. } if os::mouse_buttons_held() => {
                let scale = seat.win.gl.window.scale_factor();
                seat.dock.set_pointer(Some(pos2((position.x / scale) as f32, (position.y / scale) as f32)));
                if let Some(paths) = seat.dock.take_file_drag()
                    && let Err(err) = super::files::drag(&seat.win.gl.window, &paths)
                {
                    eprintln!("notchbase-dock: drag failed: {err}");
                    seat.dock.file_drag_ended();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pointer = seat.local(os::pointer());
                seat.dock.set_pointer(Some(pointer));
                seat.dock.mods = mods_held();
                match state {
                    ElementState::Pressed if matches!(button, MouseButton::Left | MouseButton::Right) => {
                        seat.dock.press(button == MouseButton::Right)
                    }
                    ElementState::Released => seat.dock.release(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dy, precise) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, lines) => (lines * 36.0, false),
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        ((p.y / seat.win.gl.window.scale_factor()) as f32, true)
                    }
                };
                seat.dock.scroll(dy, precise);
            }
            WindowEvent::HoveredFile(path) => seat.dock.hover_file(resolved(path)),
            WindowEvent::HoveredFileCancelled => seat.dock.hover_files(false),
            WindowEvent::DroppedFile(path) => seat.dock.drop_file(resolved(path)),
            WindowEvent::RedrawRequested => {}
            _ => return,
        }
        self.needs_frame = true;
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.tick(event_loop);
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.shutdown();
    }
}
