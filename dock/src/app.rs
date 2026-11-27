//! The event loop: two windows (the dock, and a click-through overlay for the launch effect), the pointer poll
//! that decides when the dock takes the mouse, app changes from the workspace, and frames at the display's rate
//! only while something moves.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui::{Pos2, Rect, pos2, vec2};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS as _, WindowAttributesExtMacOS as _};
use winit::window::{WindowAttributes, WindowId, WindowLevel};

use crate::config::{Config, Displays, MissionControl};
use crate::dock::{Dock, Effect, WINDOW_HEIGHT};
use crate::flight::{Flight, Way};
use crate::gl::GlWindow;
use crate::liquid::Liquid;
use crate::minimized::Minimized;
use crate::pop::{Pop, Reach};
use crate::side::Side;
use crate::windows::{self, Win};
use crate::{lifecycle, mac, real_dock};

const IDLE_POLL: Duration = Duration::from_millis(120);
const NEAR_POLL: Duration = Duration::from_millis(16);
/// Frames while only a download bar or a beat moves.
const AMBIENT_FRAME: Duration = Duration::from_millis(33);
const APPS_POLL: Duration = Duration::from_secs(3);
const SCREEN_POLL: Duration = Duration::from_secs(2);
const WINDOW_LOOKUP: Duration = Duration::from_millis(25);
/// How often the watcher thread checks for Mission Control and the system Dock: fast enough that the crossfade starts
/// with Mission Control's own animation.
const SYSTEM_DOCK_POLL: Duration = Duration::from_millis(20);
/// How often minimized windows are looked for: soon enough that a tile arrives as the window lands in the dock.
const MINIMIZED_POLL: Duration = Duration::from_millis(400);
/// How often the apps' badges are read off the system Dock.
const BADGES_POLL: Duration = Duration::from_secs(1);
/// How often to look for a zoomed window under the dock.
/// Where the hidden system Dock's Trash is, the middle of it in global top-left points (f32 bits; both 0 until
/// known). macOS keeps the Dock on whichever screen the pointer last pushed against the bottom of, not always the
/// main one, and plays its own minimize animation between a window and there.
static SYSTEM_TRASH_X: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static SYSTEM_TRASH_Y: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Where the dock tells notchbase which screens it's on and how tall it stands there.
const AREAS_FILE: &str = "dock-areas.json";
const CONFIG_POLL: Duration = Duration::from_millis(500);
const FULLSCREEN_POLL: Duration = Duration::from_millis(250);
/// Over a full-screen app: how long the pointer rests on the bottom edge before the dock comes up, and how long it's
/// away from the dock before it goes back down.
const EDGE_DWELL: Duration = Duration::from_millis(110);
const TUCK_AFTER: Duration = Duration::from_millis(500);
/// How near the bottom edge counts as on it. The pointer stops at an edge with nothing past it, so a few points do;
/// where another screen sits right below (a laptop under a monitor) it never stops there and passes on down,
/// so the band is taller and the pointer has to linger in it a moment longer, so passing through calls nothing up.
const EDGE_BAND: f32 = 3.0;
const SHARED_EDGE_BAND: f32 = 12.0;
const SHARED_EDGE_DWELL: Duration = Duration::from_millis(170);
/// How often the pointer is looked at while the dock is tucked away, waiting for the edge.
const EDGE_POLL: Duration = Duration::from_millis(30);
const AVOID_POLL: Duration = Duration::from_millis(300);
/// Frames never come closer than this, in case a swap doesn't wait for the display.
const SHORTEST_FRAME: Duration = Duration::from_micros(7_800);

struct DockWindow {
    gl: GlWindow,
    liquid: Liquid,
    backdrop: crate::glass::Backdrop,
    /// The blurred desktop picture, on the GPU, for the glass in Mission Control.
    fake: Option<glow::Texture>,
}

/// A dock on another screen, with "All displays": a copy of the dock that only shows. The pointer coming down to it
/// makes it the live one, traded with the dock that was (see `App::activate`).
struct Seat {
    /// Its screen, in AppKit's bottom-left points, and that screen's display number.
    screen: CGRect,
    display: u32,
    dock: Dock,
    win: Option<DockWindow>,
    /// Tucked away below a full-screen app on its screen.
    tucked: bool,
}

/// Windows with a minimize or restore at work, so each runs once at a time: a second ask for the same window (a
/// held Cmd+M, a click on a tile mid-flight) waits for the first to finish rather than playing over it.
#[derive(Default)]
struct Busy {
    windows: HashMap<u32, Arc<AtomicBool>>,
}

/// Held by a window's minimize or restore while it runs; the window is free again once it's dropped.
struct Done(Arc<AtomicBool>);

impl Drop for Done {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl Busy {
    /// Whether any window's minimize or restore is still at work.
    fn any(&self) -> bool {
        self.windows.values().any(|done| !done.load(Ordering::Relaxed))
    }

    /// Whether window `id`'s minimize or restore is still at work.
    fn held(&self, id: u32) -> bool {
        self.windows.get(&id).is_some_and(|done| !done.load(Ordering::Relaxed))
    }

    /// Starts work on window `id`, or None while earlier work on it hasn't finished.
    fn begin(&mut self, id: u32) -> Option<Done> {
        self.windows.retain(|_, done| !done.load(Ordering::Relaxed));
        if self.windows.contains_key(&id) {
            return None;
        }
        let done = Arc::new(AtomicBool::new(false));
        self.windows.insert(id, Arc::clone(&done));
        Some(Done(done))
    }
}

/// Work on a real window, waiting for the overlay to show what covers it (the window's picture, the screen's
/// bottom), then run on a thread of its own so the flight never waits on the app.
struct Job {
    /// Overlay frames shown since it was queued.
    shown: u8,
    queued: Instant,
    run: Box<dyn FnOnce() + Send>,
}

/// Overlay frames a job waits for: by the second, the first is surely on screen.
const JOB_FRAMES: u8 = 2;
/// The longest a job waits for them, should the overlay stall.
const JOB_WAIT: Duration = Duration::from_millis(250);
/// How long a picture that came up waits over the window's place for the window, before it fades anyway.
const REVEAL_WAIT: f32 = 2.0;
/// How long an ask that came while its window was busy is kept.
const WANTED_FOR: Duration = Duration::from_secs(4);
/// How long a window going down on another screen's dock waits for the live dock to be free to move there, before
/// it's minimized plainly.
const SEAT_WAIT: Duration = Duration::from_millis(1500);
/// How much of a screen's bottom the cover hides while macOS animates a window under it.
const COVER_HEIGHT: f32 = 56.0;

/// Covers the bottom of the screen windows park under, when that isn't the overlay's screen: a window going down
/// on one screen waits at the bottom of the one the hidden system Dock is on, and without this its sliver and
/// macOS's own animation showed there.
struct Shade {
    gl: GlWindow,
    shown: bool,
    /// Where it is, in AppKit's bottom-left points.
    frame: CGRect,
}

struct Overlay {
    gl: GlWindow,
    liquid: Liquid,
    shown: bool,
    /// The screen it covers: the live dock's, or the switcher's while that's up on another one.
    screen: CGRect,
}

struct Run {
    pop: Pop,
    path: PathBuf,
    pid: Option<i32>,
    looked: Instant,
}

pub struct App {
    dir: PathBuf,
    dock: Dock,
    win: Option<DockWindow>,
    overlay: Option<Overlay>,
    runs: Vec<Run>,
    /// Each app's window frame from last time, so a launch flies straight to where its window will be.
    windows: HashMap<PathBuf, Rect>,
    windows_dirty: bool,
    observers: Vec<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2::runtime::NSObjectProtocol>>>,
    presses: Option<mac::PressesElsewhere>,
    pressed_elsewhere: Arc<AtomicBool>,
    /// When `dock.json` was last looked at, and how it was then, so a change made elsewhere (notchbase's settings)
    /// takes at once.
    config_checked: Instant,
    config_stamp: Option<std::time::SystemTime>,
    /// The displays with a full-screen app (watched on a thread), and the ones it looks at.
    fullscreen: Arc<std::sync::Mutex<Vec<u32>>>,
    watched_displays: Arc<std::sync::Mutex<Vec<u32>>>,
    /// What the dock last made of it, and, over a full-screen app, whether the pointer has called it up.
    in_fullscreen: bool,
    revealed: bool,
    edge_since: Option<Instant>,
    left_since: Option<Instant>,
    apps_changed: Arc<AtomicBool>,
    /// Minimized windows, from their own watch thread.
    minimized: Arc<std::sync::Mutex<Option<Vec<crate::minimized::Minimized>>>>,
    /// New badges from the system Dock, for the tick to take.
    badges: Arc<std::sync::Mutex<Option<Vec<crate::badges::Badge>>>>,
    /// Windows flying down into their tiles or back up out of them.
    flights: Vec<Flight>,
    /// Windows the dock minimized, parked out of sight in the screen's corner while they're down: their app, the
    /// frame they came from, and when.
    parked: HashMap<u32, (i32, Rect, Instant)>,
    /// Work on real windows, waiting for the overlay to cover them.
    jobs: Vec<Job>,
    /// Windows whose app never minimized them: their tile goes again.
    refused: Arc<std::sync::Mutex<Vec<u32>>>,
    /// Asks that came while their window was still at work (a click mid-flight, a quick Cmd+M after a restore):
    /// the latest for each window is carried out once it's free, never dropped and never played over the first.
    wanted: Vec<(u32, i32, Way, Instant)>,
    /// Windows going down on another screen's dock while the live one was still busy: each waits until the live dock
    /// can move there, so it flies into its own screen's dock rather than going down plainly across the screens.
    seat_waits: Vec<(u32, i32, Instant)>,
    /// The cover for the screen windows park under, when that's another screen.
    shade: Option<Shade>,
    /// A stress run of minimizing and restoring a TextEdit window, while one goes.
    stress: Option<crate::stress::Run>,
    busy: Busy,
    /// Cmd+Tab's panel, while it's up.
    switcher: Option<crate::switcher::Switcher>,
    /// The screen it's on: the one the pointer was on when it opened.
    switcher_screen: CGRect,
    /// Apps by when they were last in front, newest first.
    recent: Vec<i32>,
    /// Downloads coming in, for a bar on the browser fetching them.
    downloads: Option<crate::downloads::Watch>,
    /// The music player playing, for its icon's beat.
    pulse: crate::pulse::Pulse,
    /// App icons for the switcher, by pid.
    switch_icons: HashMap<i32, egui::ColorImage>,
    /// Covers the bottom of the screen with what's behind a window while macOS animates it there.
    cover: Option<Cover>,
    /// The picture each window had as it went down, to fly it back up.
    window_pictures: HashMap<u32, egui::ColorImage>,
    /// The same pictures already on the graphics card, kept from the way down, so the way up needn't wait on them.
    window_textures: HashMap<u32, egui::TextureHandle>,
    proxy: EventLoopProxy<()>,
    /// Folders shown as stacks, each watched on a thread of its own.
    watches: HashMap<PathBuf, crate::stack::Watch>,
    /// Makes the pictures of the files in stacks, off the main thread.
    file_pictures: Option<crate::stack::Pictures>,
    /// Quick Look, opened from a stack with Space.
    quick_look: Option<std::process::Child>,
    last_frame: Instant,
    /// Whether the last tick drew nothing: the next frame is the first of a new movement.
    idle: bool,
    last_apps: Instant,
    last_screen: Instant,
    last_trash: Instant,
    /// Mission Control, App Exposé, Launchpad or the system Dock itself on screen: the dock fades out in place meanwhile.
    system_dock: Arc<std::sync::atomic::AtomicBool>,
    /// The system Dock's icons while it shows, in global top-left points (from the watch thread).
    real_dock: Arc<std::sync::Mutex<Option<[f64; 4]>>>,
    /// The desktop picture the glass's Mission Control stand-in was last asked for, and the one being built.
    wallpaper: Option<crate::wallpaper::Source>,
    wallpaper_built: Arc<std::sync::Mutex<Option<crate::wallpaper::Picture>>>,
    away: bool,
    avoider: crate::avoid::Avoider,
    last_avoid: Instant,
    hittest: bool,
    hid_dock: bool,
    keep_system_dock: bool,
    needs_frame: bool,
    /// The resize cursor is showing, over the separator.
    resize_cursor: bool,
    last_pointer: Option<Pos2>,
    modifiers: ModifiersState,
    /// The live dock's screen (AppKit's bottom-left points) and display number, and the main screen's top.
    screen: CGRect,
    display: u32,
    primary_top: f64,
    /// The main screen (AppKit's points).
    main_screen: CGRect,
    /// Every screen in global top-left points, the main one first.
    screens: Vec<Rect>,
    /// Their display numbers, in the same order.
    screen_ids: Vec<u32>,
    /// The docks on the other screens, with "All displays".
    seats: Vec<Seat>,
    /// What `dock-areas.json` last said: each screen with the dock up and how much of its bottom the dock keeps.
    areas_written: Option<Vec<(u32, f32, Side)>>,
    /// The Displays setting changed: the docks are laid out again on the next tick.
    reposition_soon: bool,
    /// Each folder's latest reading and the last minimized windows, for a dock that comes up on a new screen.
    listings: HashMap<PathBuf, crate::stack::Listing>,
    last_minimized: Option<Vec<Minimized>>,
    failed: Option<String>,
}

pub fn run(dir: PathBuf) -> Result<(), String> {
    let mut builder = EventLoop::<()>::with_user_event();
    builder
        .with_activation_policy(ActivationPolicy::Accessory)
        .with_default_menu(false)
        .with_activate_ignoring_other_apps(false);
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    let config = Config::load_or_seed(&dir, seed_from_system_dock);
    let mut app = App {
        dir: dir.clone(),
        dock: Dock::new(config, dir.clone(), vec2(1440.0, WINDOW_HEIGHT)).live(),
        win: None,
        overlay: None,
        runs: Vec::new(),
        windows: load_windows(&dir),
        windows_dirty: false,
        observers: Vec::new(),
        presses: None,
        pressed_elsewhere: Arc::new(AtomicBool::new(false)),
        config_checked: Instant::now(),
        config_stamp: None,
        fullscreen: Arc::default(),
        watched_displays: Arc::default(),
        in_fullscreen: false,
        revealed: false,
        edge_since: None,
        left_since: None,
        apps_changed: Arc::new(AtomicBool::new(true)),
        minimized: Arc::default(),
        badges: Arc::default(),
        flights: Vec::new(),
        parked: HashMap::new(),
        jobs: Vec::new(),
        refused: Arc::default(),
        wanted: Vec::new(),
        seat_waits: Vec::new(),
        shade: None,
        stress: None,
        busy: Busy::default(),
        switcher: None,
        switcher_screen: CGRect::ZERO,
        recent: Vec::new(),
        downloads: crate::downloads::Watch::start(),
        pulse: crate::pulse::Pulse::start(),
        switch_icons: HashMap::new(),
        cover: None,
        window_pictures: HashMap::new(),
        window_textures: HashMap::new(),
        proxy: event_loop.create_proxy(),
        watches: HashMap::new(),
        file_pictures: None,
        quick_look: None,
        last_frame: Instant::now(),
        idle: true,
        last_apps: Instant::now(),
        last_screen: Instant::now(),
        last_trash: Instant::now() - Duration::from_secs(60),
        system_dock: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        real_dock: Arc::new(std::sync::Mutex::new(None)),
        wallpaper: None,
        wallpaper_built: Arc::new(std::sync::Mutex::new(None)),
        away: false,
        avoider: crate::avoid::Avoider::new(),
        last_avoid: Instant::now(),
        hittest: true,
        hid_dock: false,
        keep_system_dock: std::env::var_os("NOTCHBASE_DOCK_KEEP_SYSTEM_DOCK").is_some(),
        needs_frame: true,
        resize_cursor: false,
        last_pointer: None,
        modifiers: ModifiersState::empty(),
        screen: CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1440.0, 900.0)),
        display: 0,
        primary_top: 900.0,
        main_screen: CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1440.0, 900.0)),
        screens: Vec::new(),
        screen_ids: Vec::new(),
        seats: Vec::new(),
        reposition_soon: false,
        areas_written: None,
        listings: HashMap::new(),
        last_minimized: None,
        failed: None,
    };
    let result = event_loop.run_app(&mut app).map_err(|e| e.to_string());
    app.shutdown();
    match app.failed.take() {
        Some(err) => Err(err),
        None => result,
    }
}

pub fn seed_from_system_dock() -> Option<Config> {
    let prefs = mac::dock_prefs()?;
    let mut config = Config {
        pinned: prefs.apps,
        others: prefs.others.into_iter().filter(|p| p.is_dir()).collect(),
        ..Config::default()
    };
    if let Some(t) = prefs.tile_size {
        config.tile_size = t;
    }
    if let Some(l) = prefs.large_size {
        config.large_size = l;
    }
    if let Some(m) = prefs.magnification {
        config.magnification = m;
    }
    Some(config)
}

impl App {
    /// Where the live dock's window goes: along the edge its settings ask for.
    fn dock_frame(&self) -> CGRect {
        dock_frame_on(self.screen, self.dock.config.side)
    }

    /// The live dock's screen in global top-left points.
    fn screen_rect(&self) -> Rect {
        global_rect(self.screen, self.primary_top)
    }

    /// The overlay's screen in global top-left points.
    fn overlay_rect(&self) -> Rect {
        self.overlay.as_ref().map_or_else(|| self.screen_rect(), |o| global_rect(o.screen, self.primary_top))
    }

    /// Puts the overlay over `frame`'s screen, cleared first so nothing it showed flashes up in its new place.
    fn move_overlay(&mut self, frame: CGRect) {
        let Some(o) = &mut self.overlay else { return };
        if o.screen == frame {
            return;
        }
        if o.shown {
            o.gl.frame(|_, _, _| {}, |_| {}, None, None);
            o.gl.present();
            o.shown = false;
        }
        mac::place_window(&o.gl.window, mac::EFFECT_LEVEL, frame);
        o.gl.resized();
        o.screen = frame;
        self.needs_frame = true;
    }

    /// The dock window's top left corner in global top-left points, along the edge it's laid out on now.
    fn dock_origin(&self) -> Pos2 {
        self.dock.side().window_on(self.screen_rect(), WINDOW_HEIGHT).min
    }

    fn create(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if let Some(main) = mac::screens().first() {
            self.screen = main.frame;
            self.display = main.id;
            self.primary_top = main.frame.origin.y + main.frame.size.height;
            self.main_screen = main.frame;
        }
        let w = open_dock_window(event_loop, self.dock_frame())?;
        self.dock.set_window(self.dock.config.side, w.gl.size());
        self.win = Some(w);

        let s = self.screen;
        let mut og =
            GlWindow::new(event_loop, window_attributes("notchbase dock launch", s.size.width, s.size.height), false)?;
        mac::place_window(&og.window, mac::EFFECT_LEVEL, s);
        prevent_activation(&og.window);
        let _ = og.window.set_cursor_hittest(false);
        let liquid = Liquid::new(&og.gl, og.shader_version);
        // Clear it once, then leave it ordered in: launches only redraw it.
        og.frame(|_, _, _| {}, |_| {}, None, None);
        og.present();
        mac::show_window(&og.window, true);
        self.overlay = Some(Overlay { gl: og, liquid, shown: false, screen: s });
        self.shade =
            open_shade(event_loop, self.main_screen).map_err(|e| eprintln!("notchbase-dock: no cover: {e}")).ok();
        self.reposition(event_loop);
        let proxy = self.proxy.clone();
        crate::titlebar::on_minimize(move || {
            let _ = proxy.send_event(());
        });
        let proxy = self.proxy.clone();
        crate::stack::on_keys_wake(move || {
            let _ = proxy.send_event(());
        });
        let proxy = self.proxy.clone();
        crate::switcher::on_wake(move || {
            let _ = proxy.send_event(());
        });
        // Whether this copy can take window pictures, for checking by hand: `capture.log` in the dock's folder.
        let _ = std::fs::write(
            self.dir.join("capture.log"),
            format!("screen recording allowed: {}\n", windows::can_capture(false)),
        );

        let changed = Arc::clone(&self.apps_changed);
        let proxy = self.proxy.clone();
        self.observers = mac::watch_apps(move || {
            changed.store(true, Ordering::Relaxed);
            let _ = proxy.send_event(());
        });
        let pressed = Arc::clone(&self.pressed_elsewhere);
        let proxy = self.proxy.clone();
        self.presses = mac::PressesElsewhere::new(move || {
            pressed.store(true, Ordering::Relaxed);
            let _ = proxy.send_event(());
        });

        // A full-screen app on a screen with a dock: a cheap look at the window list a few times a second.
        let full = Arc::clone(&self.fullscreen);
        let watched = Arc::clone(&self.watched_displays);
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("full screen watch".into()).spawn(move || {
            loop {
                let displays = watched.lock().map(|d| d.clone()).unwrap_or_default();
                let now: Vec<u32> = displays.into_iter().filter(|&d| windows::fullscreen_on(d)).collect();
                let changed = full.lock().is_ok_and(|mut f| std::mem::replace(&mut *f, now.clone()) != now);
                if changed && proxy.send_event(()).is_err() {
                    return;
                }
                std::thread::sleep(FULLSCREEN_POLL);
            }
        });

        // Mission Control and the system Dock are watched on a thread of their own, which wakes the loop the moment
        // either shows, so the dock fades with them rather than a poll later.
        let showing = Arc::clone(&self.system_dock);
        let real_dock = Arc::clone(&self.real_dock);
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("system dock watch".into()).spawn(move || {
            let mut watch = mac::SystemDockWatch::default();
            loop {
                let now = objc2::rc::autoreleasepool(|_| watch.showing());
                // Where its icons are, as Mission Control moves them, so the cover follows them every check.
                let moved = real_dock.lock().map(|mut seen| std::mem::replace(&mut *seen, watch.frame) != watch.frame);
                let changed = showing.swap(now, Ordering::Relaxed) != now || (now && moved.unwrap_or(false));
                if changed && proxy.send_event(()).is_err() {
                    return;
                }
                std::thread::sleep(SYSTEM_DOCK_POLL);
            }
        });

        // The apps' badges are read off the system Dock, which keeps them even while it's hidden; a change wakes the loop.
        let found = Arc::clone(&self.badges);
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("dock badges".into()).spawn(move || {
            let mut last = Vec::new();
            loop {
                let now = if crate::titlebar::trusted() {
                    let dock = mac::running_pid("com.apple.dock");
                    objc2::rc::autoreleasepool(|_| dock.map(crate::titlebar::dock_badges).unwrap_or_default())
                } else {
                    Vec::new()
                };
                if now != last {
                    last.clone_from(&now);
                    if let Ok(mut slot) = found.lock() {
                        *slot = Some(now);
                    }
                    if proxy.send_event(()).is_err() {
                        return;
                    }
                }
                std::thread::sleep(BADGES_POLL);
            }
        });

        // Minimized windows are watched on a thread of their own too; a change wakes the loop to refresh the tiles.
        let found = Arc::clone(&self.minimized);
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("minimized windows".into()).spawn(move || {
            let mut watch = crate::minimized::Watch::default();
            let mut last = Vec::new();
            loop {
                let now = objc2::rc::autoreleasepool(|_| watch.poll());
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

        if !self.keep_system_dock {
            real_dock::hide(&self.dir, &real_dock::System, self.dock.config.mission_control == MissionControl::Cover)
                .map_err(|e| format!("could not hide the Dock: {e}"))?;
            self.hid_dock = true;
        }
        Ok(())
    }

    /// Tells notchbase's window layouts how much of each screen's bottom the dock keeps, so a window snapped there
    /// ends above it: `dock-areas.json`, rewritten only when it changes. A dock tucked under a full-screen app keeps
    /// none; Mission Control showing changes nothing.
    fn write_areas(&mut self) {
        if self.away {
            return;
        }
        let reserved = self.dock.reserved_height();
        let side = self.dock.side();
        let mut areas = Vec::new();
        if !self.in_fullscreen {
            areas.push((self.display, reserved, side));
        }
        areas.extend(self.seats.iter().filter(|s| !s.tucked).map(|s| (s.display, reserved, s.dock.side())));
        areas.sort_by_key(|a| a.0);
        if self.areas_written.as_ref() == Some(&areas) {
            return;
        }
        let text = areas_json(&areas);
        let path = self.dir.join(AREAS_FILE);
        let temp = self.dir.join(format!("{AREAS_FILE}.tmp"));
        if std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, &path)).is_ok() {
            self.areas_written = Some(areas);
        }
    }

    fn shutdown(&mut self) {
        let _ = std::fs::remove_file(self.dir.join(AREAS_FILE));
        // Making way for a newer copy after an update: it takes over the hidden Dock and Cmd+Tab as they are.
        let handing_over = lifecycle::take_handover(&self.dir);
        if !handing_over {
            crate::switcher::set_catching(false);
            crate::switcher::system_switcher(true);
        }
        mac::unwatch(&self.observers);
        self.observers.clear();
        self.presses = None;
        if self.hid_dock && !handing_over {
            let _ = real_dock::restore(&self.dir, &real_dock::System);
            self.hid_dock = false;
        }
        lifecycle::release(&self.dir);
        self.overlay = None;
        self.seats.clear();
        self.win = None;
    }

    /// Keeps a dock on each screen that should have one ("All displays", or just the main screen), as screens come,
    /// go and move.
    fn reposition(&mut self, event_loop: &ActiveEventLoop) {
        let screens = mac::screens();
        let Some(main) = screens.first() else { return };
        self.primary_top = main.frame.origin.y + main.frame.size.height;
        self.main_screen = main.frame;
        self.screens = screens.iter().map(|s| global_rect(s.frame, self.primary_top)).collect();
        self.screen_ids = screens.iter().map(|s| s.id).collect();
        let wanted: Vec<mac::Screen> = match self.dock.config.displays {
            Displays::All => screens.clone(),
            Displays::Primary => vec![*main],
        };
        let display = self.display;
        self.seats.retain(|s| s.display != display && wanted.iter().any(|w| w.id == s.display));
        for w in &wanted {
            if w.id != self.display && !self.seats.iter().any(|s| s.display == w.id) {
                match self.open_seat(event_loop, w) {
                    Ok(seat) => self.seats.push(seat),
                    Err(err) => eprintln!("notchbase-dock: no dock on display {}: {err}", w.id),
                }
            }
        }
        // The live dock's screen went (or isn't wanted any more): the dock on the first wanted one takes over.
        if !wanted.iter().any(|w| w.id == self.display) {
            match self.seats.iter().position(|s| s.display == wanted[0].id) {
                Some(i) => {
                    self.activate(i);
                    self.seats.remove(i);
                }
                None => self.display = wanted[0].id,
            }
        }
        let frame_of = |id: u32| wanted.iter().find(|w| w.id == id).map(|w| w.frame);
        // A new place for the screen, or a new edge for the dock (its settings changed).
        if let Some(frame) = frame_of(self.display)
            && (frame != self.screen || self.dock.side() != self.dock.config.side)
        {
            self.screen = frame;
            let dock_frame = self.dock_frame();
            if let Some(w) = &self.win {
                mac::place_window(&w.gl.window, mac::DOCK_LEVEL, dock_frame);
                w.gl.resized();
                self.dock.set_window(self.dock.config.side, w.gl.size());
            }
            self.last_pointer = None;
            // The switcher is laid out for the screens as they were.
            if let Some(s) = &mut self.switcher {
                s.close();
            }
            self.move_overlay(frame);
            self.needs_frame = true;
        }
        for seat in &mut self.seats {
            if let Some(frame) = frame_of(seat.display)
                && (frame != seat.screen || seat.dock.side() != seat.dock.config.side)
            {
                seat.screen = frame;
                let side = seat.dock.config.side;
                if let Some(w) = &seat.win {
                    mac::place_window(&w.gl.window, mac::DOCK_LEVEL, dock_frame_on(frame, side));
                    w.gl.resized();
                    seat.dock.set_window(side, w.gl.size());
                }
                self.needs_frame = true;
            }
        }
        // Every screen, docked or not: a window never parks or waits on one showing a full-screen app.
        let mut displays: Vec<u32> =
            std::iter::once(self.display).chain(self.seats.iter().map(|s| s.display)).collect();
        for id in &self.screen_ids {
            if !displays.contains(id) {
                displays.push(*id);
            }
        }
        if let Ok(mut watched) = self.watched_displays.lock() {
            *watched = displays;
        }
    }

    /// A dock for another screen, caught up with the live one.
    fn open_seat(&mut self, event_loop: &ActiveEventLoop, screen: &mac::Screen) -> Result<Seat, String> {
        let win = open_dock_window(event_loop, dock_frame_on(screen.frame, self.dock.config.side))?;
        let _ = win.gl.window.set_cursor_hittest(false);
        let mut dock = Dock::new(self.dock.config.clone(), self.dir.clone(), win.gl.size()).live();
        dock.dark = self.dock.dark;
        dock.trash_full = self.dock.trash_full;
        dock.badges.clone_from(&self.dock.badges);
        if let Some(m) = &self.last_minimized {
            dock.set_minimized(m.clone());
        }
        dock.refresh(&mac::running_apps());
        for (folder, listing) in &self.listings {
            dock.set_listing(folder.clone(), listing.clone());
        }
        if self.away {
            dock.set_away(true);
        }
        self.needs_frame = true;
        Ok(Seat { screen: screen.frame, display: screen.id, dock, win: Some(win), tucked: false })
    }

    /// Whether the live dock is at rest with nothing of its own going on, so another screen's dock can take over.
    fn calm(&self) -> bool {
        !self.dock.popup_open()
            && !self.dock.interactive()
            && !self.dock.stack_wants_keys()
            && self.flights.is_empty()
            && self.runs.is_empty()
            && self.cover.is_none()
            && self.jobs.is_empty()
            && self.switcher.is_none()
    }

    /// The pointer came down to the dock on another screen: that one becomes the live dock (taking the pointer,
    /// the overlay, minimizing and the switcher), and this one only shows.
    fn activate(&mut self, i: usize) {
        let seat = &mut self.seats[i];
        std::mem::swap(&mut self.dock, &mut seat.dock);
        std::mem::swap(&mut self.win, &mut seat.win);
        std::mem::swap(&mut self.screen, &mut seat.screen);
        std::mem::swap(&mut self.display, &mut seat.display);
        seat.dock.set_pointer(None);
        seat.dock.fake_ready = false;
        if let Some(w) = &seat.win {
            let _ = w.gl.window.set_cursor_hittest(false);
            w.gl.window.set_cursor(winit::window::CursorIcon::Default);
        }
        if seat.dock.config != self.dock.config {
            self.dock.take_config(seat.dock.config.clone());
            self.dock.refresh(&mac::running_apps());
        }
        let tucked = std::mem::replace(&mut seat.tucked, self.in_fullscreen && !self.revealed);
        self.in_fullscreen = tucked;
        self.revealed = false;
        self.edge_since = None;
        self.left_since = None;
        self.hittest = false;
        self.resize_cursor = false;
        self.last_pointer = None;
        self.move_overlay(self.screen);
        self.wallpaper = None;
        self.needs_frame = true;
    }

    /// Hands the other screens' docks what the live one was just told, and moves the live dock to whichever screen's
    /// dock the pointer comes down to.
    fn step_seats(&mut self) {
        if self.seats.is_empty() {
            return;
        }
        let global = mac::pointer();
        if self.calm()
            && !mac::mouse_buttons_held()
            && let Some(i) = self.seats.iter().position(|s| {
                // Turned so the dock's edge is the bottom, whichever edge it is.
                let side = s.dock.side();
                let r = side.rect_to_bottom(global_rect(s.screen, self.primary_top));
                let at = side.to_bottom(global);
                let reach = s.dock.reserved_height().max(4.0);
                (r.left()..r.right()).contains(&at.x) && at.y >= r.bottom() - reach && at.y <= r.bottom()
            })
        {
            self.activate(i);
        }
        let full = self.fullscreen.lock().map(|f| f.clone()).unwrap_or_default();
        let hide = self.dock.config.hide_in_fullscreen;
        for seat in &mut self.seats {
            let tucked = hide && full.contains(&seat.display);
            if tucked != seat.tucked {
                seat.tucked = tucked;
                seat.dock.set_tucked(tucked);
                self.needs_frame = true;
            }
            if seat.dock.trash_full != self.dock.trash_full || seat.dock.dark != self.dock.dark {
                seat.dock.trash_full = self.dock.trash_full;
                seat.dock.dark = self.dock.dark;
                self.needs_frame = true;
            }
        }
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        if lifecycle::quit_requested() {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        let mut slow = Slow::new();
        let minimized = self.minimized.lock().ok().and_then(|mut slot| slot.take());
        if let Some(badges) = self.badges.lock().ok().and_then(|mut slot| slot.take()) {
            for seat in &mut self.seats {
                seat.dock.badges.clone_from(&badges);
            }
            self.dock.badges = badges;
            self.needs_frame = true;
        }
        if let Some(down) = &minimized {
            self.unpark_restored(down);
        }
        // Minimizing is the dock's own while it's on screen.
        let catching = !self.away && self.overlay.is_some();
        self.follow_config(now);
        self.follow_stacks();
        self.step_stack_keys();
        let liquid = self.dock.config.minimize_effect == crate::config::MinimizeEffect::Liquid;
        crate::titlebar::catch_minimize(catching && liquid);
        crate::switcher::set_catching(catching && self.dock.config.replace_switcher && crate::titlebar::start());
        self.step_switcher_keys();
        if catching {
            crate::titlebar::start();
            for (pid, id) in crate::titlebar::take_minimizes() {
                self.start_minimize(pid, id);
            }
        }
        let try_minimize = lifecycle::take_try_minimize();
        let try_restore = lifecycle::take_try_restore();
        let other_way =
            (try_minimize || try_restore) && std::fs::remove_file(lifecycle::data_dir().join("try-other-way")).is_ok();
        // Only ever the test window, never whatever else happens to be in front.
        let test_window = mac::frontmost_pid().is_some_and(|p| Some(p) == mac::running_pid("com.apple.TextEdit"));
        let stress = lifecycle::data_dir().join("try-stress");
        if try_minimize
            && test_window
            && self.stress.is_none()
            && let Ok(cycles) = std::fs::read_to_string(&stress)
            && let Some((pid, id)) = crate::titlebar::focused_minimizable()
        {
            let _ = std::fs::remove_file(&stress);
            let cycles = cycles.trim().parse().unwrap_or(10);
            crate::stress::log(&format!("stress: {cycles} cycles on window {id} of TextEdit ({pid})"));
            crate::flow::mirror(true);
            self.stress = Some(crate::stress::Run::new(pid, id, cycles));
        } else if try_minimize
            && test_window
            && let Some((pid, id)) = crate::titlebar::focused_minimizable()
        {
            if other_way {
                windows::set_minimized(pid, id, true);
            } else {
                self.start_minimize(pid, id);
            }
        }
        let switch = std::fs::read_to_string(lifecycle::data_dir().join("try-switch")).ok();
        let click = std::fs::read_to_string(lifecycle::data_dir().join("try-click")).ok();
        if try_restore && let Some(script) = switch {
            let _ = std::fs::remove_file(lifecycle::data_dir().join("try-switch"));
            std::thread::spawn(move || crate::titlebar::post_keys(&script));
        } else if try_restore && let Some(what) = click {
            let _ = std::fs::remove_file(lifecycle::data_dir().join("try-click"));
            if what.trim() == "yellow" {
                // Only ever the test window: never whatever else happens to be in front.
                let front = mac::frontmost_pid();
                let textedit = mac::running_pid("com.apple.TextEdit");
                if front.is_some()
                    && front == textedit
                    && let Some(at) = crate::titlebar::focused_minimize_button()
                {
                    std::thread::spawn(move || crate::titlebar::post_click(at));
                }
            } else if let Some(at) = self.dock.newest_window_target(what.trim() == "app") {
                let at = at.center() + self.dock_origin().to_vec2();
                std::thread::spawn(move || crate::titlebar::post_click(at));
            }
        } else if try_restore && std::fs::remove_file(lifecycle::data_dir().join("try-burst")).is_ok() {
            crate::recorder::start("remove", self.screen_size());
            self.dock.test_burst();
            self.needs_frame = true;
        } else if try_restore {
            self.dock.restore_newest(other_way);
        }
        if let Some(m) = &minimized {
            self.last_minimized = Some(m.clone());
            for seat in &mut self.seats {
                if seat.dock.set_minimized(m.clone()) {
                    seat.dock.refresh(&mac::running_apps());
                    self.needs_frame = true;
                }
            }
        }
        let windows_changed = minimized.is_some_and(|m| self.dock.set_minimized(m));
        if windows_changed || self.apps_changed.swap(false, Ordering::Relaxed) || now - self.last_apps > APPS_POLL {
            self.last_apps = now;
            let running = mac::running_apps();
            self.dock.refresh(&running);
            for seat in &mut self.seats {
                seat.dock.refresh(&running);
            }
            self.needs_frame = true;
            slow.mark("apps");
        }
        self.step_ambient();
        self.step_wanted();
        self.step_seat_waits();
        self.step_stress();
        if now - self.last_trash > Duration::from_secs(3) {
            self.last_trash = now;
            std::thread::spawn(|| {
                let x = mac::running_pid("com.apple.dock").and_then(|dock| {
                    crate::titlebar::dock_items(dock).into_iter().find(|i| i.0 == "AXTrashDockItem").map(|i| i.2)
                });
                if let Some(f) = x {
                    SYSTEM_TRASH_X.store(((f[0] + f[2] * 0.5) as f32).to_bits(), Ordering::Relaxed);
                    SYSTEM_TRASH_Y.store(((f[1] + f[3] * 0.5) as f32).to_bits(), Ordering::Relaxed);
                }
            });
            let full = mac::trash_is_full();
            if full != self.dock.trash_full {
                self.dock.trash_full = full;
                self.needs_frame = true;
            }
            slow.mark("trash");
        }
        if now - self.last_screen > SCREEN_POLL || std::mem::take(&mut self.reposition_soon) {
            self.last_screen = now;
            self.reposition(event_loop);
        }
        self.write_areas();
        {
            // Covering, the dock only settles a touch and darkens; fading, it makes way for the system Dock.
            let away = self.system_dock.load(Ordering::Relaxed);
            if away != self.away {
                if std::env::var_os("NOTCHBASE_DOCK_LOG_AWAY").is_some() {
                    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                    eprintln!("{} away {away}", wall.as_millis() % 100_000);
                }
                self.away = away;
                self.dock.set_away(away);
                for seat in &mut self.seats {
                    seat.dock.set_away(away);
                }
                self.needs_frame = true;
            }
            let origin = self.dock_origin();
            let real = self.real_dock.lock().ok().and_then(|f| *f).filter(|_| away).map(|[x, y, w, h]| {
                Rect::from_min_size(pos2(x as f32, y as f32) - origin.to_vec2(), vec2(w as f32, h as f32))
            });
            if real != self.dock.real_dock {
                self.dock.real_dock = real;
                self.needs_frame = true;
            }
        }
        // While a window flies, its app is busy animating: asking it anything here would stall the flight.
        // Until the app has the window down (or back in place), too: keep-clear and window fill wait for it.
        let flying = !self.flights.is_empty() || self.cover.is_some() || !self.jobs.is_empty() || self.busy.any();
        crate::titlebar::hands_off(flying);
        if self.dock.config.avoid_windows && !flying && now - self.last_avoid > AVOID_POLL {
            self.last_avoid = now;
            // Title-bar double-clicks fill to the area above the dock in one step, and the front app's resizes are
            // corrected as they happen; the check below is the fallback.
            let reserved = f64::from(self.dock.reserved_height());
            let docked: Vec<CGRect> = std::iter::once(self.screen).chain(self.seats.iter().map(|s| s.screen)).collect();
            let side = self.dock.side();
            let areas = if self.away { Vec::new() } else { fill_areas(&mac::screens(), &docked, reserved, side) };
            // Every screen with a dock up (not tucked under a full-screen app).
            let tucked: Vec<CGRect> = self.seats.iter().filter(|s| s.tucked).map(|s| s.screen).collect();
            let docks: Vec<crate::titlebar::Area> = areas
                .iter()
                .filter(|a| a.reserved && !tucked.contains(&a.frame))
                .filter(|a| a.frame != self.screen || !self.in_fullscreen)
                .map(|a| a.area)
                .collect();
            crate::titlebar::set_areas(areas.iter().map(|a| a.area).collect());
            crate::titlebar::start();
            let front = mac::frontmost_pid().filter(|&pid| pid != std::process::id() as i32);
            crate::titlebar::watch(front);
            // Never while a button is down: that's someone dragging or resizing a window.
            if let Some(pid) = front
                && !docks.is_empty()
                && !mac::mouse_buttons_held()
            {
                self.avoider.check(pid, &docks);
            }
            slow.mark("avoid");
        }
        let pressed_elsewhere = self.pressed_elsewhere.swap(false, Ordering::Relaxed);
        if pressed_elsewhere && self.dock.popup_open() {
            self.dock.close_popup();
            self.needs_frame = true;
        }

        self.step_seats();
        let global = mac::pointer();
        let local = global - self.dock_origin().to_vec2();
        let inside = Rect::from_min_size(Pos2::ZERO, self.dock.window_size()).contains(local);
        let pointer = inside.then_some(local);
        if pointer != self.last_pointer {
            self.last_pointer = pointer;
            self.dock.set_pointer(pointer);
            self.needs_frame = true;
        }
        // A release the window never heard (the button came up elsewhere).
        if !mac::mouse_buttons_held() && self.dock.interactive() && pointer.is_none() {
            self.dock.release();
        }
        self.step_fullscreen(now, global, pointer, pressed_elsewhere);
        if crate::files::take_ended() {
            self.dock.file_drag_ended();
            self.needs_frame = true;
        }
        // The window keeps the mouse while files are dragged out of it, so the drag isn't cut off.
        let hittest = self.dock.interactive() || crate::files::dragging();
        if hittest != self.hittest
            && let Some(w) = &self.win
        {
            let _ = w.gl.window.set_cursor_hittest(hittest);
            self.hittest = hittest;
        }

        slow.mark("pointer");
        let moving = self.needs_frame
            || self.switcher.as_ref().is_some_and(|s| s.animating())
            || self.dock.animating()
            || self.seats.iter().any(|s| s.dock.animating())
            || !self.runs.is_empty()
            || !self.flights.is_empty()
            || self.cover.is_some();
        if moving {
            self.render();
            slow.mark("render");
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if self.dock.ambient() || self.seats.iter().any(|s| s.dock.ambient()) {
            // A download bar or a beat moves on its own: fewer frames do for it.
            self.render();
            slow.mark("render");
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + AMBIENT_FRAME));
        } else {
            // Waiting on the edge over a full-screen app counts too, so the dock comes up the moment the wait is over.
            let near = pointer.is_some_and(|p| self.dock.edge_distance(p) < 220.0) || self.edge_since.is_some();
            self.idle = true;
            let wait = if near {
                NEAR_POLL
            } else if self.in_fullscreen && !self.revealed {
                EDGE_POLL
            } else {
                IDLE_POLL
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + wait));
        }
    }

    fn render(&mut self) {
        let now = Instant::now();
        let since = now - self.last_frame;
        if since < SHORTEST_FRAME {
            std::thread::sleep(SHORTEST_FRAME - since);
        }
        let now = Instant::now();
        // After a rest the time since the last frame is idle time, not motion: counting it made the first step of
        // every hover (in and out) jump ahead. A new movement starts with one ordinary frame.
        let dt = if std::mem::take(&mut self.idle) {
            SHORTEST_FRAME.as_secs_f32()
        } else {
            (now - self.last_frame).as_secs_f32().min(1.0 / 20.0)
        };
        self.last_frame = now;
        let all = std::mem::take(&mut self.needs_frame);

        self.dock.option_held = mac::option_held();
        self.dock.step(dt);
        if self.dock.cover_moving() && std::env::var_os("NOTCHBASE_DOCK_LOG_AWAY").is_some() {
            let (p, base, slab, tint) = self.dock.cover_state();
            let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            eprintln!("{} p {p:.3} baseline {base:.2} slab {slab:.1} tint {tint:.3}", wall.as_millis() % 100_000);
        }
        for effect in std::mem::take(&mut self.dock.effects) {
            match effect {
                Effect::Launched { path, pid, icon } => {
                    if self.dock.config.launch_effect_for(&path) == crate::config::LaunchEffect::Window {
                        self.start_pop(path, pid, icon);
                    }
                }
                Effect::Restore { pid, id, icon } => self.start_restore(pid, id, icon),
                // Only when no mouse event started it first (see `window_event`).
                Effect::DragFiles(paths) => self.drag_files(&paths),
                Effect::QuickLook(paths) => self.quick_look(paths),
            }
        }
        self.step_runs(dt);
        self.step_flights(dt);
        if let Some(s) = &mut self.switcher {
            s.step(dt);
            if s.gone() {
                self.switcher = None;
                // Shown on another screen, the overlay goes back to the live dock's.
                self.move_overlay(self.screen);
            }
        }
        // The panel showing on another screen takes the overlay there. A quick Cmd+Tab never shows it, so it never
        // moves the overlay either.
        if self.switcher.as_ref().is_some_and(|s| s.visible() && !s.closing) {
            self.move_overlay(self.switcher_screen);
        }

        // The overlay covers the live dock's screen (or the switcher's): flights, launches and the cover are in
        // global points, moved by its corner; the switcher is laid out in the overlay's own points.
        let shift = -self.overlay_rect().min.to_vec2();
        let flights = &mut self.flights;
        // Whether the overlay put a frame on screen this time (or there's no overlay to wait for).
        let mut presented = self.overlay.is_none();
        if let Some(o) = &mut self.overlay {
            // The overlay stays ordered in and only its contents change, so it never flashes an old frame.
            if self.cover.as_ref().is_some_and(|c| Instant::now() > *c.until.lock().unwrap_or_else(|e| e.into_inner()))
            {
                crate::recorder::event(|| "cover off".into());
                self.cover = None;
            }
            let cover = &mut self.cover;
            let switcher = &mut self.switcher;
            let showing = !self.runs.is_empty()
                || !flights.is_empty()
                || cover.is_some()
                || switcher.as_ref().is_some_and(|s| s.visible());
            if showing || o.shown {
                let moved = |mut blobs: Vec<crate::liquid::Blob>| {
                    for b in &mut blobs {
                        b.center += shift;
                    }
                    blobs
                };
                let mut pops: Vec<_> = self.runs.iter().map(|r| (moved(r.pop.blobs()), r.pop.alpha())).collect();
                // A flying window's glass, under its picture.
                for f in flights.iter() {
                    pops.push((moved(f.blobs()), f.glass_alpha()));
                }
                if let Some(s) = switcher.as_ref() {
                    pops.push(s.glass());
                }
                let liquid = &o.liquid;
                let draw = |gl: &glow::Context, size, ppp| {
                    for (blobs, alpha) in &pops {
                        liquid.draw(gl, size, ppp, blobs, *alpha, None, None);
                    }
                };
                let pictures = |ui: &mut egui::Ui| {
                    let global = egui::LayerId::new(egui::Order::Background, egui::Id::new("global"));
                    ui.ctx().set_transform_layer(global, egui::emath::TSTransform::from_translation(shift));
                    let painter = &ui.painter().clone().with_layer_id(global).with_clip_rect(Rect::EVERYTHING);
                    let on_top = ui
                        .painter()
                        .clone()
                        .with_layer_id(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("switcher")));
                    if let Some(c) = cover {
                        let texture = c.texture.get_or_insert_with(|| {
                            painter.ctx().load_texture("cover", c.image.clone(), egui::TextureOptions::LINEAR)
                        });
                        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                        painter.image(texture.id(), c.rect, uv, egui::Color32::WHITE);
                    }
                    for f in flights.iter_mut() {
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
                            f.paint(painter, texture.id(), alpha);
                        }
                    }
                    if let Some(s) = switcher.as_mut() {
                        s.paint(&on_top);
                    }
                };
                o.gl.frame(draw, pictures, None, None);
                o.gl.present();
                o.shown = showing;
                presented = true;
            }
        }
        // The bottom of the screen the window waits under, when that's another screen.
        if let Some(shade) = &mut self.shade {
            let patch = self.cover.as_mut().and_then(|c| c.park.as_mut());
            match patch {
                Some(p) => {
                    let frame = appkit_rect(p.rect, self.primary_top);
                    if shade.frame != frame {
                        if shade.shown {
                            shade.gl.frame(|_, _, _| {}, |_| {}, None, None);
                            shade.gl.present();
                        }
                        mac::place_window(&shade.gl.window, mac::EFFECT_LEVEL, frame);
                        shade.gl.resized();
                        shade.frame = frame;
                    }
                    let local = Rect::from_min_size(Pos2::ZERO, p.rect.size());
                    shade.gl.frame(
                        |_, _, _| {},
                        |ui| {
                            let texture = p.texture.get_or_insert_with(|| {
                                ui.ctx().load_texture("shade", p.image.clone(), egui::TextureOptions::LINEAR)
                            });
                            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                            ui.painter().clone().with_clip_rect(Rect::EVERYTHING).image(
                                texture.id(),
                                local,
                                uv,
                                egui::Color32::WHITE,
                            );
                        },
                        None,
                        None,
                    );
                    shade.gl.present();
                    shade.shown = true;
                }
                None if shade.shown => {
                    shade.gl.frame(|_, _, _| {}, |_| {}, None, None);
                    shade.gl.present();
                    shade.shown = false;
                }
                None => {}
            }
        }
        // Once the overlay has shown what covers a window, the window itself can move under it. Apps answer slowly
        // while they animate, so each job runs on a thread of its own and the flight never waits on it.
        let mut waiting = Vec::new();
        for mut job in std::mem::take(&mut self.jobs) {
            job.shown = job.shown.saturating_add(u8::from(presented));
            if job.shown >= JOB_FRAMES || job.queued.elapsed() > JOB_WAIT {
                let _ = std::thread::Builder::new().name("window".into()).spawn(job.run);
            } else {
                waiting.push(job);
            }
        }
        self.jobs = waiting;

        let picture = self.wallpaper_built.lock().ok().and_then(|mut built| built.take());
        if let Some(w) = &mut self.win {
            draw_dock(w, &mut self.dock, picture);
            let wants = self.dock.over_separator;
            if wants != self.resize_cursor {
                self.resize_cursor = wants;
                let icon = crate::side::resize_cursor(wants, self.dock.side());
                w.gl.window.set_cursor(icon);
            }
        }
        // The other screens' docks only show: they move with what they're told, never with the pointer.
        for seat in &mut self.seats {
            let moving = seat.dock.animating();
            seat.dock.step(dt);
            seat.dock.effects.clear();
            if (all || moving)
                && let Some(w) = &mut seat.win
            {
                draw_dock(w, &mut seat.dock, None);
            }
        }
    }

    fn start_pop(&mut self, path: PathBuf, pid: Option<i32>, icon: Rect) {
        let icon = icon.translate(self.dock_origin().to_vec2());
        let found = pid.and_then(mac::front_window_frame).filter(|f| self.on_screen(*f));
        let reach = if pid.is_some() { Reach::Switch } else { Reach::Launch };
        let (target, known) = match found {
            Some(frame) => (frame, true),
            None => (
                self.windows.get(&path).copied().filter(|f| self.on_screen(*f)).unwrap_or_else(|| self.likely_window()),
                false,
            ),
        };
        log_pop("start", &path, Some(target));
        // One effect per app at a time.
        self.runs.retain(|r| r.path != path);
        self.runs.push(Run { pop: Pop::new(reach, icon, target, known), path, pid, looked: Instant::now() });
    }

    /// Downloads and the music playing, onto every dock's tiles.
    fn step_ambient(&mut self) {
        let fetching = self.downloads.as_ref().map(|w| w.now()).unwrap_or_default();
        let beat =
            self.pulse.step(self.dock.config.music_pulse && !self.away, self.dock.config.music_beat, self.dock.time);
        self.dock.set_downloads(&fetching, &self.recent);
        self.dock.set_beat(beat);
        for seat in &mut self.seats {
            seat.dock.set_downloads(&fetching, &self.recent);
            seat.dock.set_beat(beat);
        }
    }

    /// Keeps the order apps were last in front, for the switcher.
    fn note_front(&mut self) {
        let me = std::process::id() as i32;
        if let Some(front) = mac::frontmost_pid().filter(|p| *p != me)
            && self.recent.first() != Some(&front)
        {
            self.recent.retain(|p| *p != front);
            self.recent.insert(0, front);
        }
    }

    /// The apps the switcher shows on screen `on` of `screens`: only ones with a window open on that screen right
    /// now. Hidden apps, minimized windows, windows on other Spaces and windows on other screens are left out. Most
    /// recently used first.
    fn switcher_entries(&mut self, screens: &[Rect], on: usize) -> Vec<crate::switcher::Entry> {
        let running = mac::running_apps();
        let apps: Vec<(i32, bool)> = running.iter().map(|a| (a.pid, a.hidden)).collect();
        let shown = crate::switcher::shown_apps(&apps, &windows::switcher_windows(), screens, on);
        let mut out = Vec::new();
        for (pid, window) in shown {
            let Some(app) = running.iter().find(|a| a.pid == pid) else { continue };
            let mut e = crate::switcher::Entry::new(app.pid, app.name.clone());
            e.window = Some(window);
            let icon = self.switch_icons.entry(app.pid).or_insert_with(|| {
                mac::icon_for_pid(app.pid, 128)
                    .unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]))
            });
            e.icon = Some(icon.clone());
            out.push(e);
        }
        let rank = |pid: i32| self.recent.iter().position(|p| *p == pid).unwrap_or(usize::MAX);
        out.sort_by_key(|e| rank(e.pid));
        out
    }

    /// Handles what the switcher's keys asked for since the last look.
    fn step_switcher_keys(&mut self) {
        use crate::switcher::{Key, Switcher};
        self.note_front();
        for key in crate::switcher::take_keys() {
            self.needs_frame = true;
            match key {
                Key::Next | Key::Prev => {
                    let by = if key == Key::Next { 1 } else { -1 };
                    match &mut self.switcher {
                        Some(s) if !s.closing => s.step_by(by),
                        _ => {
                            // It opens on the screen the pointer is on, with only that screen's windows.
                            let screens = mac::screens();
                            let rects: Vec<Rect> =
                                screens.iter().map(|s| global_rect(s.frame, self.primary_top)).collect();
                            let on = crate::switcher::screen_at(mac::pointer(), &rects);
                            let Some((on, screen)) = on.map(|i| (i, screens[i].frame)) else {
                                crate::switcher::finished();
                                continue;
                            };
                            let entries = self.switcher_entries(&rects, on);
                            if entries.is_empty() {
                                crate::switcher::finished();
                                continue;
                            }
                            self.switcher_screen = screen;
                            let front = mac::frontmost_pid();
                            let size = vec2(screen.size.width as f32, screen.size.height as f32);
                            self.switcher = Some(Switcher::new(entries, key == Key::Next, front, size));
                        }
                    }
                }
                Key::Release | Key::Choose => self.switch_now(),
                Key::Esc => match &mut self.switcher {
                    Some(s) => s.close(),
                    None => crate::switcher::finished(),
                },
                Key::Quit => {
                    if let Some(s) = self.switcher.as_mut().filter(|s| !s.closing)
                        && let Some(e) = s.entries.get(s.selected)
                    {
                        mac::quit(e.pid);
                        let i = s.selected;
                        s.remove(i);
                    }
                }
                Key::Hide => {
                    if let Some(s) = self.switcher.as_mut().filter(|s| !s.closing)
                        && let Some(e) = s.entries.get_mut(s.selected)
                    {
                        mac::hide(e.pid);
                        e.hidden = true;
                    }
                }
                Key::Click(at) => {
                    let corner = global_rect(self.switcher_screen, self.primary_top).min.to_vec2();
                    if let Some(s) = self.switcher.as_mut().filter(|s| !s.closing) {
                        match s.at(at - corner) {
                            Some(i) => {
                                s.selected = i;
                                self.switch_now();
                            }
                            None => s.close(),
                        }
                    } else {
                        crate::switcher::finished();
                    }
                }
            }
        }
        let corner = global_rect(self.switcher_screen, self.primary_top).min.to_vec2();
        if let Some(s) = self.switcher.as_mut().filter(|s| s.visible() && !s.closing) {
            s.hover(mac::pointer() - corner);
        }
    }

    /// Starts the system drag of files out of a stack.
    fn drag_files(&mut self, paths: &[PathBuf]) {
        if let Some(w) = &self.win
            && let Err(err) = crate::files::drag(&w.gl.window, paths)
        {
            eprintln!("notchbase-dock: drag failed: {err}");
            self.dock.file_drag_ended();
        }
    }

    /// Switches to the app picked in the switcher and closes it.
    fn switch_now(&mut self) {
        let Some(s) = self.switcher.as_mut().filter(|s| !s.closing) else {
            crate::switcher::finished();
            return;
        };
        let pick = s.entries.get(s.selected).map(|e| (e.pid, e.window));
        s.close();
        match pick {
            // Its window on this screen comes to the front, not whichever one it last had in front elsewhere. Apps
            // answer Accessibility slowly at times, so it's on a thread of its own.
            Some((pid, Some(id))) => {
                let raised =
                    std::thread::Builder::new().name("switch".into()).spawn(move || windows::raise_id(pid, id));
                if raised.is_err() {
                    mac::activate(pid);
                }
            }
            Some((pid, None)) => mac::activate(pid),
            None => {}
        }
    }

    /// Keeps the glass's Mission Control stand-in current: built again on a thread of its own whenever the desktop
    /// picture, its file or the window changes (another Space, display or wallpaper), so it's ready before Mission
    /// Control opens.
    fn follow_wallpaper(&mut self) {
        if self.away {
            return;
        }
        let origin = self.dock_origin() - self.screen_rect().min.to_vec2();
        let size = self.dock.window_size();
        let window = [f64::from(origin.x), f64::from(origin.y), f64::from(size.x), f64::from(size.y)];
        let source = crate::wallpaper::source(self.display, window);
        if source == self.wallpaper {
            return;
        }
        self.wallpaper = source.clone();
        let Some(source) = source else { return };
        let built = Arc::clone(&self.wallpaper_built);
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("wallpaper".into()).spawn(move || {
            if let Some(picture) = objc2::rc::autoreleasepool(|_| crate::wallpaper::render(&source))
                && let Ok(mut slot) = built.lock()
            {
                *slot = Some(picture);
                let _ = proxy.send_event(());
            }
        });
    }

    /// Keeps a watch on every folder in the dock and hands the dock what changed: new readings of the folders and
    /// the pictures made for their files.
    /// Feeds the keys typed while a stack is open to it. The tap only catches them while one is.
    fn step_stack_keys(&mut self) {
        let wants = self.dock.stack_wants_keys();
        let on = wants && crate::titlebar::start();
        crate::stack::catch_keys(on);
        self.dock.stack_keys = on;
        for key in crate::stack::take_keys() {
            if wants {
                self.dock.stack_key(key);
            }
            self.needs_frame = true;
        }
    }

    /// Opens Quick Look on `paths`, or closes it if it's already up.
    fn quick_look(&mut self, paths: Vec<PathBuf>) {
        if let Some(mut child) = self.quick_look.take()
            && matches!(child.try_wait(), Ok(None))
        {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        if paths.is_empty() {
            return;
        }
        let child = std::process::Command::new("/usr/bin/qlmanage")
            .arg("-p")
            .args(&paths)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match child {
            Ok(child) => self.quick_look = Some(child),
            Err(err) => eprintln!("notchbase-dock: quick look failed: {err}"),
        }
    }

    fn follow_stacks(&mut self) {
        let folders = self.dock.stack_folders();
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
                self.listings.insert(folder.clone(), listing.clone());
                self.dock.set_listing(folder.clone(), listing);
                self.needs_frame = true;
            }
        }
        self.listings.retain(|f, _| self.watches.contains_key(f));
        if let Some(keep) = self.dock.take_picture_keep()
            && let Some(pictures) = &self.file_pictures
        {
            pictures.keep(keep);
        }
        let mut wants = self.dock.take_picture_wants();
        for seat in &mut self.seats {
            let _ = seat.dock.take_picture_keep();
            wants.extend(seat.dock.take_picture_wants());
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
                self.dock.give_pictures(came);
                self.needs_frame = true;
            }
        }
    }

    /// Picks up `dock.json` changed by someone else (notchbase's Dock settings), checked about twice a second.
    fn follow_config(&mut self, now: Instant) {
        if now - self.config_checked < CONFIG_POLL {
            return;
        }
        self.config_checked = now;
        self.follow_wallpaper();
        let dark = mac::dark_mode();
        if dark != self.dock.dark {
            self.dock.dark = dark;
            self.needs_frame = true;
        }
        let path = crate::config::Config::path(&self.dir);
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if stamp == self.config_stamp {
            return;
        }
        let first = self.config_stamp.is_none();
        self.config_stamp = stamp;
        if first {
            return;
        }
        let Some(config) = crate::config::Config::read(&self.dir) else { return };
        let running = mac::running_apps();
        for seat in &mut self.seats {
            if config != seat.dock.config {
                seat.dock.take_config(config.clone());
                seat.dock.refresh(&running);
                self.needs_frame = true;
            }
        }
        if config != self.dock.config {
            let displays = config.displays != self.dock.config.displays || config.side != self.dock.config.side;
            self.dock.take_config(config);
            self.dock.refresh(&running);
            self.needs_frame = true;
            if displays {
                self.reposition_soon = true;
            }
        }
    }

    /// Over a full-screen app the dock stays tucked away below the screen, as the Dock does: resting the pointer on
    /// the bottom edge brings it up, and moving away from it (or clicking in the app) sends it back down.
    fn step_fullscreen(&mut self, now: Instant, global: Pos2, pointer: Option<Pos2>, pressed_elsewhere: bool) {
        let full =
            self.dock.config.hide_in_fullscreen && self.fullscreen.lock().is_ok_and(|f| f.contains(&self.display));
        if full != self.in_fullscreen {
            self.in_fullscreen = full;
            self.revealed = false;
            self.edge_since = None;
            self.left_since = None;
            self.dock.set_tucked(full);
            self.needs_frame = true;
        }
        if !full {
            return;
        }
        let screen = self.screen_rect();
        if !self.revealed {
            let dwell =
                reveal_dwell(screen, &self.screens, global, self.dock.side()).filter(|_| !mac::mouse_buttons_held());
            if let Some(dwell) = dwell {
                if now - *self.edge_since.get_or_insert(now) >= dwell {
                    self.revealed = true;
                    self.left_since = None;
                    self.dock.set_tucked(false);
                    self.needs_frame = true;
                }
            } else {
                self.edge_since = None;
            }
            return;
        }
        let near = pointer.is_some_and(|p| self.dock.edge_distance(p) < self.dock.reserved_height() + 40.0);
        let busy = self.dock.popup_open() || self.dock.interactive() || !self.flights.is_empty();
        if near || busy {
            self.left_since = None;
        }
        let gone = !near && !busy && now - *self.left_since.get_or_insert(now) >= TUCK_AFTER;
        if gone || (pressed_elsewhere && !busy) {
            self.revealed = false;
            self.edge_since = None;
            self.dock.set_tucked(true);
            self.needs_frame = true;
        }
    }

    /// Covers the bottom of the screen, where a parked window's sliver shows, with what's behind window `id`, until
    /// the returned time (to be moved up once macOS is done there).
    /// Covers the bottom of the live dock's screen, and of `park` (the screen the window waits under) when that's
    /// another one, with what's there without window `id`, until the returned time.
    fn cover_bottom(&mut self, id: u32, park: Rect) -> Arc<std::sync::Mutex<Instant>> {
        let until = Arc::new(std::sync::Mutex::new(Instant::now() + Duration::from_secs(3)));
        let bottom = |screen: Rect| Rect::from_min_max(pos2(screen.left(), screen.bottom() - COVER_HEIGHT), screen.max);
        let rect = bottom(self.overlay_rect());
        if let Some(image) = windows::screen_without(rect, id) {
            crate::recorder::event(|| "cover on".into());
            let elsewhere = self.shade.is_some() && overlap(self.overlay_rect(), park) < park.area() * 0.5;
            let park = elsewhere
                .then(|| bottom(park))
                .and_then(|rect| windows::screen_without(rect, id).map(|image| Patch { image, rect, texture: None }));
            self.cover = Some(Cover { image, rect, texture: None, park, until: Arc::clone(&until) });
            self.needs_frame = true;
        }
        until
    }

    fn screen_size(&self) -> egui::Vec2 {
        vec2(self.screen.size.width as f32, self.screen.size.height as f32)
    }

    /// The screens, and which of them show a full-screen app right now. A window moved onto one of those (to wait
    /// under it, or in its corner) takes that screen out of the full-screen app's Space to the window's desktop,
    /// swiping the app out of view, so windows keep off them.
    fn screens_now(&self) -> (Vec<Rect>, Vec<bool>) {
        if self.screens.is_empty() {
            return (vec![self.screen_rect()], vec![false]);
        }
        let full = self.fullscreen.lock().map(|f| f.clone()).unwrap_or_default();
        let busy = self.screen_ids.iter().map(|id| full.contains(id)).collect();
        (self.screens.clone(), busy)
    }

    /// Where a window of `size` (on the screen at `home`, global points) waits while it's down, and the screen it
    /// waits under (see `park_spot_on`).
    fn park(&self, size: egui::Vec2, home: Rect) -> (Pos2, Rect) {
        let (screens, mut full) = self.screens_now();
        let home = screens.iter().position(|s| overlap(*s, home) >= s.area() * 0.5);
        // The window's own screen shows its desktop, whatever the watch last said.
        if let Some(h) = home {
            full[h] = false;
        }
        let trash = pos2(
            f32::from_bits(SYSTEM_TRASH_X.load(Ordering::Relaxed)),
            f32::from_bits(SYSTEM_TRASH_Y.load(Ordering::Relaxed)),
        );
        let trash = (trash != Pos2::ZERO).then_some(trash);
        let fallback = screens[home.unwrap_or(0)];
        match park_spot_on(&screens, trash, size, home, &full) {
            Some((spot, i)) => (spot, screens[i]),
            None => (pos2((fallback.center().x - size.x * 0.5).round(), fallback.bottom() - 1.0), fallback),
        }
    }

    /// Where a window coming back up waits for its picture to land: a corner with only a point of it on screen.
    fn waiting_corner(&self, size: egui::Vec2) -> Pos2 {
        let (screens, mut full) = self.screens_now();
        let live = self.screen_rect();
        for (i, s) in screens.iter().enumerate() {
            if overlap(*s, live) >= s.area() * 0.5 {
                full[i] = false;
            }
        }
        waiting_corner_on(&screens, &full, live, size)
    }

    /// A window handed over to minimize: its picture covers it and flies down into its new tile; once the overlay
    /// shows the picture and the screen's bottom (the cover), the window itself is parked under the cover and
    /// minimized there. An ask while the window is still at work waits its turn.
    fn start_minimize(&mut self, pid: i32, id: u32) {
        if self.flights.iter().any(|f| f.id == id) || self.busy.held(id) {
            self.want(id, pid, Way::Down);
            return;
        }
        // A window on another screen with a dock of its own flies into that one, which becomes the live dock. While
        // the live dock is still busy (another window flying, a menu open) it waits for it, rather than going down
        // plainly and having macOS play its own animation across the screens.
        let found = windows::frame_of(pid, id);
        if let Some(f) = found
            && !self.on_screen(f)
            && let Some(i) =
                self.seats.iter().position(|s| mostly_on(&self.screens, global_rect(s.screen, self.primary_top), f))
        {
            if !self.calm() {
                if !self.seat_waits.iter().any(|w| w.0 == id) {
                    crate::recorder::event(|| format!("down {id}: another screen's dock, waits for this one"));
                    self.seat_waits.push((id, pid, Instant::now()));
                }
                return;
            }
            self.activate(i);
        }
        let Some(done) = self.busy.begin(id) else { return };
        let trace = crate::flow::Trace::new(
            format!("down {id} {} on display {} from {found:?}", self.dock.app_name(pid), self.display),
            Some(crate::flow::log_file()),
        );
        // Only a window on the live dock's screen can fly into it; anything else minimizes the usual way.
        let Some(frame) = found.filter(|f| self.on_screen(*f)) else {
            trace.step(|| "not on a docked screen: minimized plainly".into());
            windows::set_minimized(pid, id, true);
            return;
        };
        crate::recorder::start("minimize", self.screen_size());
        let picture = windows::picture(id, (frame.height() * 2.0).min(1600.0) as u32);
        trace.step(|| format!("picture {:?}", picture.as_ref().map(|p| p.size)));
        if let Some(p) = &picture {
            self.window_pictures.insert(id, p.clone());
        }
        let win = Win { id, title: String::new(), frame, on_screen: false };
        // The tile only needs a small one; a full-size picture would stall the dock's frame that uploads it.
        self.dock.expect_minimized(Minimized { pid, win }, picture.as_ref().map(|p| windows::smaller(p, 256)));
        let tile = self.tile_rect(id);
        let fly = self.dock.config.minimize_duration;
        let mut flight = Flight::new(Way::Down, id, frame, tile, picture).toward(self.dock.side()).lasting(fly);
        flight.trace = Some(trace.clone());
        self.flights.push(flight);
        self.parked.insert(id, (pid, frame, Instant::now()));
        // The screen's bottom without the window, taken now so it's in the very first frame with the picture.
        let (spot, park) = self.park(frame.size(), self.screen_rect());
        let until = self.cover_bottom(id, park);
        trace.step(|| format!("cover {} parked at {spot:?}", self.cover.is_some()));
        let refused = Arc::clone(&self.refused);
        let run = move || {
            let _done = done;
            trace.step(|| "covered".into());
            let outcome = match windows::Handle::find(pid, id) {
                Some(ax) => crate::flow::go_down(&ax, spot, frame.min, crate::flow::LIMITS, &trace),
                None => {
                    trace.step(|| "window not found: minimized plainly".into());
                    windows::set_minimized(pid, id, true);
                    crate::flow::Down::Minimized
                }
            };
            // macOS's own animation may still be finishing under the cover.
            let after = if outcome == crate::flow::Down::Minimized { 450 } else { 0 };
            *until.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now() + Duration::from_millis(after);
            if outcome == crate::flow::Down::Refused {
                refused.lock().unwrap_or_else(|e| e.into_inner()).push(id);
            }
            trace.step(|| format!("done: {outcome:?}"));
        };
        self.jobs.push(Job { shown: 0, queued: Instant::now(), run: Box::new(run) });
        self.needs_frame = true;
    }

    /// Moves a stress run on: the acts due now, and a new cycle when one is over.
    fn step_stress(&mut self) {
        use crate::flow::Ax as _;
        use crate::stress::Act;
        let Some(run) = &mut self.stress else { return };
        if run.acts.is_empty() {
            if run.left == 0 {
                crate::stress::log(&format!("stress done: {} passed, {} failed", run.passed, run.failed));
                crate::flow::mirror(false);
                self.stress = None;
                return;
            }
            let screen = global_rect(self.screen, self.primary_top);
            let other = self.seats.first().map(|s| global_rect(s.screen, self.primary_top));
            let (kind, acts) = crate::stress::cycle(run.n, screen, other);
            (run.kind, run.acts, run.started) = (kind, acts, Instant::now());
            run.left -= 1;
            run.n += 1;
        }
        let (pid, id, due) = (run.pid, run.id, run.due());
        for act in due {
            match act {
                Act::Place(r) => {
                    if let Some(run) = &mut self.stress {
                        run.placed = r;
                    }
                    if let Some(ax) = windows::Handle::find(pid, id) {
                        ax.move_to(r.min);
                        ax.resize(r.size());
                    }
                }
                Act::Minimize => self.start_minimize(pid, id),
                Act::Restore => match self.dock.window_tile(id) {
                    Some(icon) => self.start_restore(pid, id, icon),
                    None => self.want(id, pid, Way::Up),
                },
                Act::Check => self.check_stress(pid, id),
            }
        }
        self.needs_frame = true;
    }

    /// Whether a stress cycle ended cleanly, written to its log.
    fn check_stress(&mut self, pid: i32, id: u32) {
        use crate::flow::Ax as _;
        let Some(run) = &self.stress else { return };
        let (n, kind, placed) = (run.n, run.kind, run.placed);
        let ax = windows::Handle::find(pid, id);
        let frame = ax.as_ref().and_then(|a| a.frame());
        let down = ax.as_ref().and_then(|a| a.minimized());
        let mut wrong = Vec::new();
        if down != Some(false) {
            wrong.push(format!("window not up ({down:?})"));
        }
        if !frame.is_some_and(|f| crate::stress::in_place(f, placed)) {
            wrong.push(format!("window at {frame:?}, placed {placed:?}"));
        }
        if self.parked.contains_key(&id) {
            wrong.push("still parked".into());
        }
        if self.flights.iter().any(|f| f.id == id) || !self.jobs.is_empty() || self.busy.held(id) {
            wrong.push("still at work".into());
        }
        if self.cover.is_some() {
            wrong.push("cover still up".into());
        }
        if self.dock.holds_window(id) {
            wrong.push("tile left in the dock".into());
        }
        if self.wanted.iter().any(|w| w.0 == id) {
            wrong.push("an ask left over".into());
        }
        let ok = wrong.is_empty();
        crate::stress::log(&format!(
            "cycle {n} {kind}: {}",
            if ok { "ok".to_owned() } else { format!("FAIL {}", wrong.join("; ")) }
        ));
        if let Some(run) = &mut self.stress {
            if ok {
                run.passed += 1;
            } else {
                run.failed += 1;
            }
        }
        // Whatever happened, the next cycle starts from a window that's up.
        if down != Some(false) {
            windows::raise(pid, &Win { id, title: String::new(), frame: Rect::NOTHING, on_screen: false });
        }
    }

    /// Windows waiting to go down into another screen's dock: each goes once the live dock is free to move there,
    /// or plainly if it never is in time.
    fn step_seat_waits(&mut self) {
        if self.seat_waits.is_empty() {
            return;
        }
        for (id, pid, at) in std::mem::take(&mut self.seat_waits) {
            if self.calm() {
                self.start_minimize(pid, id);
            } else if at.elapsed() > SEAT_WAIT {
                crate::recorder::event(|| format!("down {id}: the live dock stayed busy, minimized plainly"));
                windows::set_minimized(pid, id, true);
            } else {
                self.seat_waits.push((id, pid, at));
            }
        }
        self.needs_frame = true;
    }

    /// Keeps an ask for a window that's still at work, replacing any earlier one for it.
    fn want(&mut self, id: u32, pid: i32, way: Way) {
        crate::recorder::event(|| format!("{way:?} {id}: busy, waits its turn"));
        self.wanted.retain(|w| w.0 != id);
        self.wanted.push((id, pid, way, Instant::now()));
    }

    /// Carries out asks whose window is free again, if they still make sense (a window already down isn't
    /// minimized again).
    fn step_wanted(&mut self) {
        self.wanted.retain(|w| w.3.elapsed() < WANTED_FOR);
        let free: Vec<(u32, i32, Way)> = self
            .wanted
            .iter()
            .filter(|w| !self.busy.held(w.0) && !self.flights.iter().any(|f| f.id == w.0))
            .map(|w| (w.0, w.1, w.2))
            .collect();
        self.wanted.retain(|w| !free.iter().any(|f| f.0 == w.0));
        for (id, pid, way) in free {
            let down = self.dock.holds_window(id);
            match way {
                Way::Down if !down => self.start_minimize(pid, id),
                Way::Up if down => {
                    if let Some(icon) = self.dock.window_tile(id) {
                        self.start_restore(pid, id, icon);
                    }
                }
                _ => {}
            }
        }
        // Windows whose app never minimized them are up where they were: their tile goes again.
        let refused = std::mem::take(&mut *self.refused.lock().unwrap_or_else(|e| e.into_inner()));
        for id in refused {
            self.dock.forget_expected(id);
            self.parked.remove(&id);
            self.window_pictures.remove(&id);
            self.window_textures.remove(&id);
            self.needs_frame = true;
        }
    }

    /// A minimized window's tile clicked: its picture flies from the tile to where the window was. Once the overlay
    /// shows it and the cover, the window comes up under the cover, waits in the corner (a point of it on screen)
    /// until the picture has landed, then goes back under it, and the picture fades onto it.
    ///
    /// Apps differ: some answer late, some put a window back where they like. Every step is bounded and checked,
    /// and an app that doesn't play along gets a plain restore rather than a window shown twice. A click while the
    /// window is still at work waits its turn.
    fn start_restore(&mut self, pid: i32, id: u32, icon: Rect) {
        if self.flights.iter().any(|f| f.id == id) || self.busy.held(id) {
            self.want(id, pid, Way::Up);
            return;
        }
        let Some(done) = self.busy.begin(id) else { return };
        crate::recorder::start("restore", self.screen_size());
        let tile = icon.translate(self.dock_origin().to_vec2());
        let ours = self.parked.get(&id).map(|p| p.1);
        let window = ours
            .or_else(|| self.dock.minimized_frame(id))
            .filter(|f| self.on_screen(*f))
            .unwrap_or_else(|| self.likely_window());
        let trace = crate::flow::Trace::new(
            format!("up {id} {} on display {} to {window:?}", self.dock.app_name(pid), self.display),
            Some(crate::flow::log_file()),
        );
        // A window minimized some other way waits where it was, so it would show at once: park it first, and if the
        // app won't move a minimized window, bring it back plainly.
        if ours.is_none() {
            let size = windows::frame_of(pid, id).map_or(window.size(), |f| f.size());
            let (spot, _) = self.park(size, self.screen_rect());
            windows::move_to(pid, id, spot);
            // macOS keeps a sliver of a window on screen, so it may end up a little higher than asked.
            let moved =
                windows::frame_of(pid, id).is_some_and(|f| (f.min.x - spot.x).abs() < 2.0 && f.min.y > spot.y - 48.0);
            if !moved {
                trace.step(|| "the app keeps it where it was: restored plainly".into());
                windows::raise(pid, &Win { id, title: String::new(), frame: Rect::NOTHING, on_screen: false });
                return;
            }
        }
        let texture = self.window_pictures.contains_key(&id).then(|| self.window_textures.get(&id).cloned()).flatten();
        let picture = self
            .window_pictures
            .get(&id)
            .cloned()
            .or_else(|| windows::picture(id, (window.height() * 2.0).min(1600.0) as u32))
            // A window minimized some other way: the picture its tile shows.
            .or_else(|| self.dock.window_picture(id));
        trace.step(|| format!("picture {:?}", picture.as_ref().map(|p| p.size)));
        self.parked.insert(id, (pid, window, Instant::now()));
        let mut flight = Flight::new(Way::Up, id, window, tile, picture)
            .toward(self.dock.side())
            .lasting(self.dock.config.minimize_duration);
        flight.texture = texture;
        let ready = Arc::new(AtomicBool::new(false));
        let landed = Arc::new(AtomicBool::new(false));
        flight.ready = Some(Arc::clone(&ready));
        flight.landed = Some(Arc::clone(&landed));
        flight.trace = Some(trace.clone());
        self.flights.push(flight);
        let (spot, park) = self.park(window.size(), self.screen_rect());
        // While it waits for the picture, it sits in a corner, where only a point of it shows.
        let corner = self.waiting_corner(window.size());
        let until = self.cover_bottom(id, park);
        trace.step(|| format!("cover {} parked at {spot:?}, waits at {corner:?}", self.cover.is_some()));
        let run = move || {
            let _done = done;
            trace.step(|| "covered".into());
            let cornered = || {
                *until.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now() + Duration::from_millis(120);
            };
            match windows::Handle::find(pid, id) {
                Some(ax) => {
                    let up = crate::flow::come_up(
                        &ax,
                        spot,
                        corner,
                        window,
                        &landed,
                        &cornered,
                        crate::flow::LIMITS,
                        &trace,
                    );
                    trace.step(|| format!("done: {up:?}"));
                }
                None => {
                    trace.step(|| "window not found: restored plainly".into());
                    windows::raise(pid, &Win { id, title: String::new(), frame: window, on_screen: true });
                    cornered();
                }
            }
            ready.store(true, Ordering::Relaxed);
        };
        self.jobs.push(Job { shown: 0, queued: Instant::now(), run: Box::new(run) });
        self.needs_frame = true;
    }

    /// A window's tile's icon in global points, or where a new one would be.
    fn tile_rect(&self, id: u32) -> Rect {
        let origin = self.dock_origin().to_vec2();
        self.dock.window_tile(id).unwrap_or_else(|| self.dock.spare_tile()).translate(origin)
    }

    fn step_flights(&mut self, dt: f32) {
        let mut flights = std::mem::take(&mut self.flights);
        for f in &mut flights {
            if f.way == Way::Down {
                f.set_tile(self.tile_rect(f.id));
            }
            f.step(dt);
            crate::recorder::event(|| {
                format!(
                    "{:?} {} glass {:.2} picture {:.2} shape {:?}",
                    f.way,
                    f.id,
                    f.glass_alpha(),
                    f.picture_alpha(),
                    f.shape()
                )
            });
            if f.way == Way::Down && f.arrived() {
                if let Some(t) = &f.texture {
                    self.window_textures.insert(f.id, t.clone());
                }
                self.dock.unveil(f.id);
            }
            // Up over where the window was: put the real window back under the picture, then let the picture go.
            if f.way == Way::Up
                && f.arrived()
                && let Some(landed) = &f.landed
                && !landed.swap(true, Ordering::Relaxed)
            {
                crate::recorder::event(|| "landed".into());
            }
            if f.way == Way::Up && f.arrived() && !f.revealing {
                let ready = f.ready.as_ref().is_none_or(|r| r.load(Ordering::Relaxed));
                // A window that's slow to come back never keeps its picture hanging there.
                let late = f.waited() > REVEAL_WAIT;
                if ready || late {
                    f.revealing = true;
                    if let Some(t) = &f.trace {
                        t.step(
                            || if ready { "revealing".into() } else { "revealing before the window was back".into() },
                        );
                    }
                }
            }
        }
        for f in flights.iter().filter(|f| f.finished() && f.way == Way::Up) {
            self.parked.remove(&f.id);
            self.window_pictures.remove(&f.id);
            self.window_textures.remove(&f.id);
        }
        if flights.iter().any(|f| f.finished()) {
            crate::recorder::end_after(Duration::from_millis(350));
        }
        flights.retain(|f| !f.finished());
        self.flights = flights;
    }

    /// A parked window that came back up some other way (its app, the Window menu) goes back where it was.
    fn unpark_restored(&mut self, down: &[Minimized]) {
        let flying: Vec<u32> = self.flights.iter().map(|f| f.id).collect();
        self.parked.retain(|id, (pid, frame, at)| {
            if down.iter().any(|m| m.win.id == *id) || flying.contains(id) || at.elapsed() < Duration::from_millis(1500)
            {
                return true;
            }
            windows::move_to(*pid, *id, frame.min);
            false
        });
        self.window_pictures.retain(|id, _| self.parked.contains_key(id));
        self.window_textures.retain(|id, _| self.parked.contains_key(id));
    }

    /// Where a new app's window most likely opens: centred, a bit above the middle.
    fn likely_window(&self) -> Rect {
        let screen = self.screen_rect();
        let (w, h) = (screen.width(), screen.height());
        Rect::from_center_size(screen.min + vec2(w * 0.5, h * 0.47), vec2(w * 0.56, h * 0.6))
    }

    /// The overlay covers the live dock's screen, so only a window there can be flown into.
    /// Whether `frame` is on the live dock's screen: more of it there than on any other. A window only touching it
    /// (hanging over from the screen above) belongs to the screen it's mostly on, which is where macOS shows it.
    fn on_screen(&self, frame: Rect) -> bool {
        if self.screens.is_empty() {
            return self.screen_rect().intersects(frame);
        }
        mostly_on(&self.screens, self.screen_rect(), frame)
    }

    fn step_runs(&mut self, dt: f32) {
        let now = Instant::now();
        let mut running = None;
        let screen = self.screen_rect();
        let mut learned = false;
        for r in &mut self.runs {
            if now - r.looked >= WINDOW_LOOKUP {
                r.looked = now;
                if r.pid.is_none() {
                    let list = running.get_or_insert_with(mac::running_apps);
                    r.pid = list.iter().find(|a| a.path.as_deref() == Some(r.path.as_path())).map(|a| a.pid);
                }
                if let Some(frame) = r.pid.and_then(mac::front_window_frame)
                    && screen.intersects(frame)
                {
                    if !r.pop.knows_window() || r.pop.target != frame {
                        log_pop("window", &r.path, Some(frame));
                    }
                    r.pop.window_at(frame);
                    if self.windows.insert(r.path.clone(), frame) != Some(frame) {
                        learned = true;
                    }
                }
            }
            r.pop.step(dt);
        }
        self.runs.retain(|r| !r.pop.finished());
        self.windows_dirty |= learned;
        if self.windows_dirty && self.runs.is_empty() {
            self.windows_dirty = false;
            save_windows(&self.dir, &self.windows);
        }
    }
}

const WINDOWS_FILE: &str = "windows.json";

fn load_windows(dir: &std::path::Path) -> HashMap<PathBuf, Rect> {
    let Ok(text) = std::fs::read_to_string(dir.join(WINDOWS_FILE)) else { return HashMap::new() };
    let raw: HashMap<PathBuf, [f32; 4]> = serde_json::from_str(&text).unwrap_or_default();
    raw.into_iter().map(|(p, [x0, y0, x1, y1])| (p, Rect::from_min_max(pos2(x0, y0), pos2(x1, y1)))).collect()
}

fn save_windows(dir: &std::path::Path, windows: &HashMap<PathBuf, Rect>) {
    let raw: HashMap<&PathBuf, [f32; 4]> =
        windows.iter().map(|(p, r)| (p, [r.min.x, r.min.y, r.max.x, r.max.y])).collect();
    if let Ok(text) = serde_json::to_string_pretty(&raw) {
        let tmp = dir.join("windows.json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, dir.join(WINDOWS_FILE));
        }
    }
}

/// With `NOTCHBASE_DOCK_LOG_POP` set, says which window frame each launch effect opened into.
fn log_pop(what: &str, path: &std::path::Path, frame: Option<Rect>) {
    if std::env::var_os("NOTCHBASE_DOCK_LOG_POP").is_some() {
        eprintln!("pop {what} {}: {frame:?}", path.display());
    }
}

/// Draws one frame of `dock` into its window, with a new desktop picture for its Mission Control glass if one came.
fn draw_dock(w: &mut DockWindow, dock: &mut Dock, picture: Option<crate::wallpaper::Picture>) {
    if let Some(picture) = picture {
        w.gl.make_current();
        w.fake = crate::liquid::upload(&w.gl.gl, w.fake.take(), &picture);
    }
    dock.fake_ready = w.fake.is_some();
    let fake = w.fake;
    let blobs = std::cell::RefCell::new(Vec::new());
    let liquid = &w.liquid;
    let opacity = std::cell::Cell::new(1.0);
    let blur = std::cell::Cell::new(1.0);
    let band = std::cell::Cell::new(dock.glass_band());
    let ppp = std::cell::Cell::new(1.0);
    let faked = std::cell::Cell::new(0.0);
    w.gl.frame(
        |gl, size, p| {
            let fake = fake.map(|t| (t, faked.get()));
            liquid.draw(gl, size, p, &blobs.borrow(), opacity.get(), Some(band.get()), fake);
        },
        |ui| {
            *blobs.borrow_mut() = dock.compose(ui.painter());
            faked.set(dock.fake_glass());
            opacity.set(dock.opacity());
            blur.set(dock.blur_opacity());
            band.set(dock.glass_band());
            ppp.set(ui.pixels_per_point());
        },
        None,
        None,
    );
    // The blur fades with the dock, and goes once it's fully faded.
    let shown: &[crate::liquid::Blob] = if blur.get() > 0.004 { &blobs.borrow() } else { &[] };
    w.backdrop.fade(blur.get());
    w.backdrop.shape(shown, ppp.get());
    w.gl.present();
}

fn window_attributes(title: &str, w: f64, h: f64) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(LogicalSize::new(w, h))
        .with_position(LogicalPosition::new(0.0, 0.0))
        .with_has_shadow(false)
        .with_accepts_first_mouse(true)
}

/// A dock's window, placed at `frame` (AppKit's bottom-left points) and shown.
fn open_dock_window(event_loop: &ActiveEventLoop, frame: CGRect) -> Result<DockWindow, String> {
    let gl = GlWindow::new(event_loop, window_attributes("notchbase dock", frame.size.width, frame.size.height), true)?;
    mac::place_window(&gl.window, mac::DOCK_LEVEL, frame);
    prevent_activation(&gl.window);
    let liquid = Liquid::new(&gl.gl, gl.shader_version);
    mac::show_window(&gl.window, true);
    let backdrop = crate::glass::Backdrop::new(&gl.window);
    Ok(DockWindow { gl, liquid, backdrop, fake: None })
}

/// Over a full-screen app on `screen` (global top-left points, as `screens` are): how long the pointer at `at` has to
/// stay where it is to call the dock up, or `None` when it isn't on the screen's bottom edge. Where another screen
/// sits right below, the pointer can't rest on the edge, so a taller band near it counts, with a longer wait.
///
/// A dock on a side comes up from that side's edge the same way: everything is turned so that edge is the bottom.
fn reveal_dwell(screen: Rect, screens: &[Rect], at: Pos2, side: Side) -> Option<Duration> {
    let (screen, at) = (side.rect_to_bottom(screen), side.to_bottom(at));
    if !(screen.left()..screen.right()).contains(&at.x) {
        return None;
    }
    let shared = screens
        .iter()
        .map(|s| side.rect_to_bottom(*s))
        .any(|s| s != screen && (s.top() - screen.bottom()).abs() < 1.0 && (s.left()..s.right()).contains(&at.x));
    let (band, dwell, past) =
        if shared { (SHARED_EDGE_BAND, SHARED_EDGE_DWELL, 0.0) } else { (EDGE_BAND, EDGE_DWELL, 1.0) };
    (at.y > screen.bottom() - band && at.y <= screen.bottom() + past).then_some(dwell)
}

/// The dock window's frame on a screen (AppKit's, from the bottom left): along its bottom, or down one of its sides
/// (see [`Side::window_on`]).
fn dock_frame_on(screen: CGRect, side: Side) -> CGRect {
    let reach = screen.size.width.min(f64::from(crate::side::SIDE_REACH));
    match side {
        Side::Bottom => CGRect::new(screen.origin, CGSize::new(screen.size.width, f64::from(WINDOW_HEIGHT))),
        Side::Left => CGRect::new(screen.origin, CGSize::new(reach, screen.size.height)),
        Side::Right => CGRect::new(
            CGPoint::new(screen.origin.x + screen.size.width - reach, screen.origin.y),
            CGSize::new(reach, screen.size.height),
        ),
    }
}

/// How much of `frame` lies on `screen`, in square points.
fn overlap(screen: Rect, frame: Rect) -> f32 {
    let both = screen.intersect(frame);
    if both.is_positive() { both.area() } else { 0.0 }
}

/// Whether `frame` is mostly on `screen`: at least as much of it there as on any other of `screens`.
fn mostly_on(screens: &[Rect], screen: Rect, frame: Rect) -> bool {
    let here = overlap(screen, frame);
    here > 0.0 && screens.iter().all(|s| overlap(*s, frame) <= here)
}

/// The screen the hidden system Dock is on: the one whose bottom edge its Trash (`trash`, its middle) is by.
fn dock_screen(screens: &[Rect], trash: Pos2) -> Option<usize> {
    screens
        .iter()
        .enumerate()
        .filter(|(_, s)| (s.left()..=s.right()).contains(&trash.x))
        .map(|(i, s)| (i, (s.bottom() - trash.y).abs()))
        .filter(|(_, off)| *off < 150.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Where a window of `size` waits while it's down, and which of `screens` it waits under: just below the bottom of
/// the screen the hidden system Dock is on (found by its Trash), a little toward the Trash, under the cover. macOS
/// plays its own minimize and restore between a window and its Dock, so a window waiting under any other screen
/// (the main one, while the Dock had moved to another) showed that animation flying across the screens.
///
/// The window hangs down from there, so it never waits where it would hang over another screen below or beside:
/// it slides along the bottom to a free stretch, or waits under its own screen `home` (then the main one, then any
/// other) when there's none. It never waits under a screen marked in `full` (one showing a full-screen app): moved
/// there, the window would take that screen out of the app's Space. None when no screen has room, which only an odd
/// layout gives.
fn park_spot_on(
    screens: &[Rect],
    trash: Option<Pos2>,
    size: egui::Vec2,
    home: Option<usize>,
    full: &[bool],
) -> Option<(Pos2, usize)> {
    let dock = trash.and_then(|t| dock_screen(screens, t));
    let mut order: Vec<usize> = Vec::new();
    for i in dock.into_iter().chain(home).chain(0..screens.len()) {
        if !order.contains(&i) && !full.get(i).copied().unwrap_or(false) {
            order.push(i);
        }
    }
    for i in order {
        let s = screens[i];
        let ours = s.center().x;
        let system = trash.map(|t| t.x).filter(|x| (s.left()..s.right()).contains(x)).unwrap_or(ours);
        let middle = ours + (system - ours) * 0.35;
        let wanted = (middle - size.x * 0.5).round();
        let y = s.bottom() - 1.0;
        // Enough of it under this screen that it's this screen's window, even if it reaches past the edge.
        let enough = size.x.clamp(1.0, 40.0);
        let free = |x: f32| {
            let r = Rect::from_min_size(pos2(x, y), size);
            let under = r.right().min(s.right()) - r.left().max(s.left());
            under >= enough && screens.iter().enumerate().all(|(j, o)| j == i || overlap(*o, r) <= 0.0)
        };
        let mut tries = vec![wanted, s.left(), s.right() - size.x];
        for o in screens {
            tries.extend([o.right(), o.left() - size.x]);
        }
        if let Some(x) =
            tries.into_iter().filter(|x| free(*x)).min_by(|a, b| (a - wanted).abs().total_cmp(&(b - wanted).abs()))
        {
            return Some((pos2(x, y), i));
        }
    }
    None
}

/// Where a window of `size` coming back up waits: in the bottom-right corner of `prefer` (the live dock's screen),
/// with only a point of it on screen. Never where the rest of it would hang over another screen (one beside or
/// below, more than a hairline along an edge they share), and never on or over a screen marked in `full` (one
/// showing a full-screen app); another screen's corner then, if one is free.
fn waiting_corner_on(screens: &[Rect], full: &[bool], prefer: Rect, size: egui::Vec2) -> Pos2 {
    let corner = |s: Rect| s.max - vec2(1.0, 1.0);
    let hairline = size.x.max(size.y) + 1.0;
    let busy = |i: usize| full.get(i).copied().unwrap_or(false);
    let is_full = |o: Rect| screens.iter().enumerate().any(|(i, f)| busy(i) && overlap(*f, o) >= f.area() * 0.5);
    let free = |s: Rect| {
        let r = Rect::from_min_size(corner(s), size);
        let same = |o: Rect| overlap(o, s) >= s.area() * 0.5;
        !is_full(s)
            && screens
                .iter()
                .enumerate()
                .all(|(i, o)| same(*o) || overlap(*o, r) <= if busy(i) { 0.0 } else { hairline })
    };
    std::iter::once(prefer).chain(screens.iter().copied()).find(|s| free(*s)).map_or(corner(prefer), corner)
}

/// A screen's AppKit frame (bottom-left points) in global top-left points, given the main screen's top.
fn global_rect(frame: CGRect, primary_top: f64) -> Rect {
    let top = primary_top - (frame.origin.y + frame.size.height);
    Rect::from_min_size(
        pos2(frame.origin.x as f32, top as f32),
        vec2(frame.size.width as f32, frame.size.height as f32),
    )
}

/// Clicking the dock shouldn't take focus from the app in front. `_setPreventsActivation:` is what AppKit's own
/// panels use; when it isn't there the click still works, it just activates this app for a moment.
fn prevent_activation(window: &winit::window::Window) {
    let Some(ns) = mac::ns_window(window) else { return };
    let sel = objc2::sel!(_setPreventsActivation:);
    let responds: bool = unsafe { objc2::msg_send![&*ns, respondsToSelector: sel] };
    if responds {
        let _: () = unsafe { objc2::msg_send![&*ns, _setPreventsActivation: true] };
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_some() {
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
        // Files dragged onto, or a click on, another screen's dock: that one becomes the live dock first.
        let input = matches!(
            event,
            WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::HoveredFile(_)
                | WindowEvent::DroppedFile(_)
        );
        if let Some(i) = self.seats.iter().position(|s| s.win.as_ref().is_some_and(|w| w.gl.window.id() == id)) {
            if input && !self.dock.popup_open() && !self.dock.interactive() {
                self.activate(i);
            } else if let WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } = event {
                let seat = &mut self.seats[i];
                if let Some(w) = &seat.win {
                    w.gl.resized();
                    seat.dock.set_window(seat.dock.side(), w.gl.size());
                }
                self.needs_frame = true;
                return;
            } else {
                return;
            }
        }
        let is_dock = self.win.as_ref().is_some_and(|w| w.gl.window.id() == id);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(w) = &self.win
                    && is_dock
                {
                    w.gl.resized();
                    self.dock.set_window(self.dock.side(), w.gl.size());
                }
                if let Some(o) = &self.overlay
                    && o.gl.window.id() == id
                {
                    o.gl.resized();
                }
                self.needs_frame = true;
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            // A press dragged far enough out of a stack starts the file drag here, inside the mouse event: macOS
            // only lets a drag begin from the real event.
            WindowEvent::CursorMoved { position, .. } if is_dock && mac::mouse_buttons_held() => {
                let scale = self.win.as_ref().map_or(2.0, |w| w.gl.window.scale_factor());
                self.dock.set_pointer(Some(pos2((position.x / scale) as f32, (position.y / scale) as f32)));
                if let Some(paths) = self.dock.take_file_drag() {
                    self.drag_files(&paths);
                }
                self.needs_frame = true;
            }
            WindowEvent::MouseInput { state, button, .. } if is_dock => {
                let pointer = mac::pointer() - self.dock_origin().to_vec2();
                self.dock.set_pointer(Some(pointer));
                let right = button == MouseButton::Right || self.modifiers.control_key();
                self.dock.mods = mods_held();
                match state {
                    ElementState::Pressed if matches!(button, MouseButton::Left | MouseButton::Right) => {
                        self.dock.press(right)
                    }
                    ElementState::Released => self.dock.release(),
                    _ => {}
                }
                self.needs_frame = true;
            }
            WindowEvent::MouseWheel { delta, .. } if is_dock => {
                let (dy, precise) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, lines) => (lines * 36.0, false),
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        let scale = self.win.as_ref().map_or(2.0, |w| w.gl.window.scale_factor());
                        ((p.y / scale) as f32, true)
                    }
                };
                self.dock.scroll(dy, precise);
                self.needs_frame = true;
            }
            WindowEvent::HoveredFile(path) if is_dock => {
                self.dock.hover_file(path);
                self.needs_frame = true;
            }
            WindowEvent::HoveredFileCancelled if is_dock => {
                self.dock.hover_files(false);
                self.needs_frame = true;
            }
            WindowEvent::DroppedFile(path) if is_dock => {
                self.dock.drop_file(path);
                self.needs_frame = true;
            }
            WindowEvent::RedrawRequested => self.needs_frame = true,
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.tick(event_loop);
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.shutdown();
    }
}

/// Logs any part of a tick that holds up the main thread, under `NOTCHBASE_DOCK_LOG_SLOW`.
struct Slow {
    on: bool,
    at: Instant,
}

impl Slow {
    fn new() -> Self {
        Self { on: std::env::var_os("NOTCHBASE_DOCK_LOG_SLOW").is_some(), at: Instant::now() }
    }

    fn mark(&mut self, what: &str) {
        let took = self.at.elapsed();
        if took > Duration::from_millis(12) {
            crate::recorder::event(|| format!("slow: {what} took {:.1} ms", took.as_secs_f64() * 1000.0));
        }
        if !self.on {
            self.at = Instant::now();
            return;
        }
        if took > Duration::from_millis(6) {
            eprintln!("slow: {what} took {:.1} ms", took.as_secs_f64() * 1000.0);
        }
        self.at = Instant::now();
    }
}

/// A stand-in for the bottom of the screen, drawn over it while macOS animates a window there.
struct Cover {
    image: egui::ColorImage,
    rect: Rect,
    texture: Option<egui::TextureHandle>,
    /// The bottom of the screen the window waits under, when that's another screen, for the shade.
    park: Option<Patch>,
    until: Arc<std::sync::Mutex<Instant>>,
}

/// A piece of a screen as it is without a window, in global top-left points.
struct Patch {
    image: egui::ColorImage,
    rect: Rect,
    texture: Option<egui::TextureHandle>,
}

/// The shade's window, cleared and ordered in under the main screen's bottom, out of the way until it's needed.
fn open_shade(event_loop: &ActiveEventLoop, main: CGRect) -> Result<Shade, String> {
    let frame = CGRect::new(main.origin, CGSize::new(main.size.width, f64::from(COVER_HEIGHT)));
    let mut gl = GlWindow::new(
        event_loop,
        window_attributes("notchbase dock cover", frame.size.width, frame.size.height),
        false,
    )?;
    mac::place_window(&gl.window, mac::EFFECT_LEVEL, frame);
    prevent_activation(&gl.window);
    let _ = gl.window.set_cursor_hittest(false);
    gl.resized();
    gl.frame(|_, _, _| {}, |_| {}, None, None);
    gl.present();
    mac::show_window(&gl.window, true);
    Ok(Shade { gl, shown: false, frame })
}

/// A rect in global top-left points as an AppKit frame (bottom-left points), given the main screen's top.
fn appkit_rect(r: Rect, primary_top: f64) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(r.left()), primary_top - f64::from(r.bottom())),
        CGSize::new(f64::from(r.width()), f64::from(r.height())),
    )
}

/// The Command and Shift keys held right now, read from the window server (the dock never has the keyboard).
#[cfg(target_os = "macos")]
fn mods_held() -> crate::dock::Mods {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceFlagsState(state: i32) -> u64;
    }
    // The combined session state.
    let flags = unsafe { CGEventSourceFlagsState(0) };
    crate::dock::Mods { command: flags & 0x10_0000 != 0, shift: flags & 0x2_0000 != 0 }
}

#[cfg(not(target_os = "macos"))]
fn mods_held() -> crate::dock::Mods {
    crate::dock::Mods::default()
}

/// A screen's area for filled windows, and whether the dock takes an edge of it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FillArea {
    area: crate::titlebar::Area,
    reserved: bool,
    /// Its screen's AppKit frame.
    frame: CGRect,
}

/// What a filled window covers on each screen: below its menu bar (and notch), and clear of the dock (above it, or
/// beside it on `side`) on screens with a dock only.
fn fill_areas(screens: &[mac::Screen], docked: &[CGRect], reserved: f64, side: Side) -> Vec<FillArea> {
    screens
        .iter()
        .map(|s| {
            let docked = docked.contains(&s.frame);
            let keep = |on: Side| if docked && side == on { reserved } else { 0.0 };
            FillArea {
                area: crate::titlebar::Area {
                    left: s.left + keep(Side::Left),
                    top: s.top + s.menu,
                    right: s.right - keep(Side::Right),
                    bottom: s.bottom - keep(Side::Bottom),
                    screen_left: s.left,
                    screen_top: s.top,
                    screen_right: s.right,
                    screen_bottom: s.bottom,
                },
                reserved: docked,
                frame: s.frame,
            }
        })
        .collect()
}

/// `[{"display": 1, "reserved": 70, "side": "bottom"}, …]`, as notchbase reads it: how far in from which edge.
fn areas_json(areas: &[(u32, f32, Side)]) -> String {
    let rows: Vec<String> = areas
        .iter()
        .map(|(display, reserved, side)| {
            format!("{{\"display\": {display}, \"reserved\": {reserved:.1}, \"side\": \"{}\"}}", side.key())
        })
        .collect();
    format!("[{}]", rows.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// John's screens: the laptop is main, a 1920 x 1080 monitor above it, a little to the left.
    fn laptop_and_monitor_above() -> [Rect; 2] {
        [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1470.0, 956.0)),
            Rect::from_min_size(pos2(-60.0, -1080.0), vec2(1920.0, 1080.0)),
        ]
    }

    /// Three screens side by side, the main one in the middle; the one on the right sits lower and is taller.
    fn three_in_a_row() -> [Rect; 3] {
        [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1512.0, 982.0)),
            Rect::from_min_size(pos2(-1920.0, -98.0), vec2(1920.0, 1080.0)),
            Rect::from_min_size(pos2(1512.0, -200.0), vec2(2560.0, 1440.0)),
        ]
    }

    fn parked(screens: &[Rect], spot: Pos2, size: egui::Vec2, on: usize) {
        let r = Rect::from_min_size(spot, size);
        assert_eq!(spot.y, screens[on].bottom() - 1.0, "just under the bottom of screen {on}");
        assert!(r.right() > screens[on].left() && r.left() < screens[on].right(), "under screen {on}");
        for (i, s) in screens.iter().enumerate().filter(|(i, _)| *i != on) {
            assert!(overlap(*s, r) <= 0.0, "never hanging over screen {i}: {r:?}");
        }
    }

    #[test]
    fn windows_park_under_the_main_screen_while_the_system_dock_is_there() {
        let screens = laptop_and_monitor_above();
        let size = vec2(600.0, 400.0);
        let (spot, on) = park_spot_on(&screens, Some(pos2(1200.0, 940.0)), size, None, &[]).unwrap();
        assert_eq!(on, 0);
        parked(&screens, spot, size, 0);
        assert!(spot.x + 300.0 > 735.0, "a little toward the Trash");
        // The Trash not known yet: the main screen's middle.
        assert_eq!(park_spot_on(&screens, None, size, None, &[]), Some((pos2(435.0, 955.0), 0)));
        // The Trash somewhere no screen's bottom is: the main screen too.
        assert_eq!(park_spot_on(&screens, Some(pos2(-5000.0, 30.0)), size, None, &[]).map(|p| p.1), Some(0));
    }

    #[test]
    fn windows_park_under_whichever_screen_the_system_dock_moved_to() {
        let screens = three_in_a_row();
        let size = vec2(800.0, 600.0);
        // macOS moved its Dock to the monitor on the left, so its animation plays there: so does the window wait.
        let (spot, on) = park_spot_on(&screens, Some(pos2(-400.0, 975.0)), size, None, &[]).unwrap();
        assert_eq!(on, 1);
        parked(&screens, spot, size, 1);
        // And on the right, taller one.
        let (spot, on) = park_spot_on(&screens, Some(pos2(3500.0, 1230.0)), size, None, &[]).unwrap();
        assert_eq!(on, 2);
        parked(&screens, spot, size, 2);
        // Back on the main screen. The right-hand screen reaches lower than its bottom, so the window never slides
        // over toward there.
        let (spot, on) = park_spot_on(&screens, Some(pos2(1400.0, 970.0)), vec2(1400.0, 900.0), None, &[]).unwrap();
        assert_eq!(on, 0);
        parked(&screens, spot, vec2(1400.0, 900.0), 0);
    }

    #[test]
    fn a_window_never_parks_where_it_would_hang_over_the_screen_below() {
        // The Dock on the monitor above the laptop: under most of the monitor's bottom is the laptop, so the window
        // waits under the stretch of it that has nothing below.
        let screens = laptop_and_monitor_above();
        let size = vec2(600.0, 400.0);
        let (spot, on) = park_spot_on(&screens, Some(pos2(900.0, -12.0)), size, None, &[]).unwrap();
        assert_eq!(on, 1);
        parked(&screens, spot, size, 1);
        // A wide one too, reaching out past the monitor's edge where there's no screen at all.
        let wide = vec2(1600.0, 900.0);
        let (spot, on) = park_spot_on(&screens, Some(pos2(900.0, -12.0)), wide, None, &[]).unwrap();
        assert_eq!(on, 1);
        parked(&screens, spot, wide, 1);
        // A monitor squarely on top of the laptop has no free stretch at all: the window waits under the laptop.
        let stacked = [screens[0], Rect::from_min_size(pos2(0.0, -900.0), vec2(1470.0, 900.0))];
        let (spot, on) = park_spot_on(&stacked, Some(pos2(900.0, -12.0)), size, None, &[]).unwrap();
        assert_eq!(on, 0);
        parked(&stacked, spot, size, 0);
    }

    #[test]
    fn the_system_dock_is_on_the_screen_whose_bottom_its_trash_is_by() {
        let screens = laptop_and_monitor_above();
        // The monitor's bottom is the laptop's top: which one depends on which bottom it's by.
        assert_eq!(dock_screen(&screens, pos2(1300.0, 940.0)), Some(0));
        assert_eq!(dock_screen(&screens, pos2(1300.0, 970.0)), Some(0), "hidden just below the bottom");
        assert_eq!(dock_screen(&screens, pos2(1300.0, -14.0)), Some(1));
        assert_eq!(dock_screen(&screens, pos2(1300.0, 400.0)), None, "nowhere near a bottom");
    }

    #[test]
    fn a_window_belongs_to_the_screen_most_of_it_is_on() {
        let [laptop, monitor] = laptop_and_monitor_above();
        let screens = [laptop, monitor];
        // On the monitor, its bottom just over the laptop's top.
        let window = Rect::from_min_size(pos2(200.0, -700.0), vec2(800.0, 720.0));
        assert!(mostly_on(&screens, monitor, window));
        assert!(!mostly_on(&screens, laptop, window), "touching the laptop doesn't make it the laptop's");
        let window = Rect::from_min_size(pos2(200.0, -20.0), vec2(800.0, 600.0));
        assert!(mostly_on(&screens, laptop, window));
        assert!(!mostly_on(&screens, laptop, Rect::from_min_size(pos2(3000.0, 0.0), vec2(10.0, 10.0))));
    }

    #[test]
    fn a_window_coming_up_waits_in_a_corner_that_shows_only_a_point_of_it() {
        let screens = three_in_a_row();
        let size = vec2(800.0, 600.0);
        // A point of it, or at most a hairline along the bottom edge of a screen beside it.
        let point_only = |corner: Pos2| {
            let r = Rect::from_min_size(corner, size);
            let shown: f32 = screens.iter().map(|s| overlap(*s, r)).sum();
            assert!(shown <= size.x + 2.0, "only a point shows at {corner:?}, not {shown}");
        };
        // The left screen's corner is free.
        let corner = waiting_corner_on(&screens, &[], screens[1], size);
        assert_eq!(corner, pos2(-1.0, 981.0));
        point_only(corner);
        // The main screen's would hang over the taller one on its right, so another free one is used.
        let corner = waiting_corner_on(&screens, &[], screens[0], size);
        assert_ne!(corner, pos2(1511.0, 981.0));
        point_only(corner);
        // John's: the monitor's corner has nothing past it.
        let two = laptop_and_monitor_above();
        assert_eq!(waiting_corner_on(&two, &[], two[1], size), pos2(1859.0, -1.0));
        assert_eq!(waiting_corner_on(&two, &[], two[0], size), pos2(1469.0, 955.0));
        // One screen alone: its own corner.
        let one = [screens[0]];
        assert_eq!(waiting_corner_on(&one, &[], one[0], size), pos2(1511.0, 981.0));
    }

    #[test]
    fn windows_never_park_under_a_screen_showing_a_full_screen_app() {
        // A full-screen app on the laptop, where the system Dock is, and a window going down on the monitor: moved
        // onto the laptop, it would swipe the app out of view. It waits under its own screen instead.
        let screens = three_in_a_row();
        let size = vec2(800.0, 600.0);
        let full = [true, false, false];
        let (spot, on) = park_spot_on(&screens, Some(pos2(1200.0, 970.0)), size, Some(1), &full).unwrap();
        assert_eq!(on, 1, "under the window's own screen");
        parked(&screens, spot, size, 1);
        // Without the full-screen app it goes where the Dock is.
        let (_, on) = park_spot_on(&screens, Some(pos2(1200.0, 970.0)), size, Some(1), &[false; 3]).unwrap();
        assert_eq!(on, 0);
        // The Trash not known: its own screen still comes before the main one.
        let (_, on) = park_spot_on(&screens, None, size, Some(2), &[false; 3]).unwrap();
        assert_eq!(on, 2);
    }

    #[test]
    fn a_window_coming_up_never_waits_on_a_screen_showing_a_full_screen_app() {
        let screens = three_in_a_row();
        let size = vec2(800.0, 600.0);
        // From the main screen its corner hangs over the right-hand one, and the left one shows a full-screen app,
        // so it waits in the right-hand screen's corner.
        let corner = waiting_corner_on(&screens, &[false, true, false], screens[0], size);
        assert_eq!(corner, pos2(4071.0, 1239.0));
        // Its own corner is fine as long as nothing full-screen is past it.
        let two = laptop_and_monitor_above();
        assert_eq!(waiting_corner_on(&two, &[true, false], two[1], size), pos2(1859.0, -1.0));
    }

    #[test]
    fn the_dock_comes_up_from_the_bottom_edge_over_a_full_screen_app() {
        let two = laptop_and_monitor_above();
        let (laptop, monitor) = (two[0], two[1]);
        // The laptop's bottom has nothing past it: the pointer stops right on it.
        assert_eq!(reveal_dwell(laptop, &two, pos2(700.0, 956.0), Side::Bottom), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(laptop, &two, pos2(700.0, 954.5), Side::Bottom), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(laptop, &two, pos2(700.0, 950.0), Side::Bottom), None, "not near enough");
        assert_eq!(reveal_dwell(laptop, &two, pos2(1500.0, 956.0), Side::Bottom), None, "off the screen");
        // The monitor's bottom runs into the laptop for most of its width: a taller band and a longer wait there.
        assert_eq!(reveal_dwell(monitor, &two, pos2(700.0, -0.5), Side::Bottom), Some(SHARED_EDGE_DWELL));
        assert_eq!(reveal_dwell(monitor, &two, pos2(700.0, -10.0), Side::Bottom), Some(SHARED_EDGE_DWELL));
        assert_eq!(reveal_dwell(monitor, &two, pos2(700.0, 0.5), Side::Bottom), None, "already on the laptop");
        assert_eq!(reveal_dwell(monitor, &two, pos2(700.0, -20.0), Side::Bottom), None);
        // Past the laptop's sides the monitor's edge is a real one again.
        assert_eq!(reveal_dwell(monitor, &two, pos2(1700.0, 0.0), Side::Bottom), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(monitor, &two, pos2(-30.0, -2.0), Side::Bottom), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(monitor, &two, pos2(1700.0, -10.0), Side::Bottom), None);
    }

    #[test]
    fn appkit_frames_round_trip() {
        let frame = CGRect::new(CGPoint::new(-60.0, 956.0), CGSize::new(1920.0, 1080.0));
        let r = global_rect(frame, 956.0);
        assert_eq!(r, Rect::from_min_size(pos2(-60.0, -1080.0), vec2(1920.0, 1080.0)));
        let back = appkit_rect(r, 956.0);
        assert_eq!((back.origin.x, back.origin.y, back.size.width, back.size.height), (-60.0, 956.0, 1920.0, 1080.0));
    }

    #[test]
    fn each_window_minimizes_or_restores_once_at_a_time() {
        let mut busy = Busy::default();
        let first = busy.begin(7).expect("free to start");
        assert!(busy.begin(7).is_none(), "a second ask while the first is at work is skipped");
        let other = busy.begin(8);
        assert!(other.is_some(), "other windows aren't held up");
        drop(first);
        let again = busy.begin(7);
        assert!(again.is_some(), "free again once the first is done");
        assert!(busy.begin(7).is_none());
        drop((again, other));
        for _ in 0..5 {
            assert!(busy.begin(7).is_some() && busy.begin(8).is_some());
        }
        assert!(busy.windows.len() <= 2, "finished work doesn't pile up");
    }

    #[test]
    fn the_areas_file_lists_each_screen_and_how_tall_the_dock_stands_there() {
        assert_eq!(areas_json(&[]), "[]");
        assert_eq!(
            areas_json(&[(1, 70.0, Side::Bottom), (4, 70.0, Side::Left)]),
            r#"[{"display": 1, "reserved": 70.0, "side": "bottom"}, {"display": 4, "reserved": 70.0, "side": "left"}]"#
        );
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn screens_turn_into_top_left_points_with_their_own_menu_bars_and_the_dock_only_on_its_own() {
        // AppKit's frames: the laptop is main; a 1x monitor to its left, its bottom 111 points lower.
        let laptop = rect(0.0, 0.0, 1470.0, 956.0);
        let monitor = rect(-1920.0, -111.0, 1920.0, 1200.0);
        let main_top = 956.0;
        let screens = [
            mac::screen_from(laptop, rect(0.0, 0.0, 1470.0, 923.0), main_top),
            mac::screen_from(monitor, rect(-1920.0, -111.0, 1920.0, 1175.0), main_top),
        ];
        assert_eq!((screens[0].top, screens[0].bottom, screens[0].menu), (0.0, 956.0, 33.0));
        assert_eq!((screens[1].left, screens[1].top, screens[1].bottom), (-1920.0, -133.0, 1067.0));
        assert_eq!(screens[1].menu, 25.0);

        let areas = fill_areas(&screens, &[laptop], 70.0, Side::Bottom);
        assert!(areas[0].reserved && !areas[1].reserved);
        assert_eq!((areas[0].area.top, areas[0].area.bottom), (33.0, 886.0));
        assert_eq!((areas[1].area.left, areas[1].area.top, areas[1].area.bottom), (-1920.0, -108.0, 1067.0));
        // With the dock on the monitor, the laptop fills to its bottom.
        let areas = fill_areas(&screens, &[monitor], 70.0, Side::Bottom);
        assert_eq!(areas[0].area.bottom, 956.0);
        assert_eq!(areas[1].area.bottom, 997.0);
        // With a dock on each, both keep clear of theirs.
        let areas = fill_areas(&screens, &[laptop, monitor], 70.0, Side::Bottom);
        assert_eq!((areas[0].area.bottom, areas[1].area.bottom), (886.0, 997.0));
        assert!(areas.iter().all(|a| a.reserved));
        // A dock on the left keeps the laptop's left edge, and the bottom is the screen's again.
        let areas = fill_areas(&screens, &[laptop], 70.0, Side::Left);
        assert_eq!((areas[0].area.left, areas[0].area.right, areas[0].area.bottom), (70.0, 1470.0, 956.0));
        let areas = fill_areas(&screens, &[laptop], 70.0, Side::Right);
        assert_eq!((areas[0].area.left, areas[0].area.right), (0.0, 1400.0));
        assert_eq!(areas[1].area.left, -1920.0, "a screen without the dock keeps all of itself");
    }

    #[test]
    fn a_side_dock_comes_up_from_its_own_edge_over_a_full_screen_app() {
        let two = laptop_and_monitor_above();
        let laptop = two[0];
        assert_eq!(reveal_dwell(laptop, &two, pos2(0.0, 500.0), Side::Left), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(laptop, &two, pos2(1.0, 500.0), Side::Left), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(laptop, &two, pos2(10.0, 500.0), Side::Left), None, "not near enough");
        assert_eq!(reveal_dwell(laptop, &two, pos2(700.0, 956.0), Side::Left), None, "the bottom does nothing");
        assert_eq!(reveal_dwell(laptop, &two, pos2(1470.0, 500.0), Side::Right), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(laptop, &two, pos2(1469.0, 500.0), Side::Right), Some(EDGE_DWELL));
        assert_eq!(reveal_dwell(laptop, &two, pos2(0.0, 500.0), Side::Right), None);
        // Three in a row: the main screen's left edge runs into the one on its left, so it waits longer there.
        let row = three_in_a_row();
        assert_eq!(reveal_dwell(row[0], &row, pos2(0.5, 500.0), Side::Left), Some(SHARED_EDGE_DWELL));
        assert_eq!(reveal_dwell(row[0], &row, pos2(10.0, 500.0), Side::Left), Some(SHARED_EDGE_DWELL));
        assert_eq!(reveal_dwell(row[0], &row, pos2(-1.0, 500.0), Side::Left), None, "already on the other screen");
    }

    #[test]
    fn a_side_docks_window_runs_down_that_side_of_the_screen() {
        let laptop = rect(0.0, 0.0, 1470.0, 956.0);
        let f = dock_frame_on(laptop, Side::Left);
        assert_eq!((f.origin.x, f.origin.y, f.size.width, f.size.height), (0.0, 0.0, 960.0, 956.0));
        let f = dock_frame_on(laptop, Side::Right);
        assert_eq!((f.origin.x, f.origin.y, f.size.width, f.size.height), (510.0, 0.0, 960.0, 956.0));
        let f = dock_frame_on(laptop, Side::Bottom);
        assert_eq!((f.size.width, f.size.height), (1470.0, f64::from(WINDOW_HEIGHT)));
        // AppKit's frame and the dock's own top-left one agree.
        let top_left = Side::Right.window_on(global_rect(laptop, 956.0), WINDOW_HEIGHT);
        assert_eq!(appkit_rect(top_left, 956.0), dock_frame_on(laptop, Side::Right));
    }
}
