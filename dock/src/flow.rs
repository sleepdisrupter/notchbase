//! The real window's side of minimizing and restoring, on a thread of its own while the picture flies: parking it
//! out of sight and minimizing it, or bringing it back up and putting it in its place. Every step is bounded and
//! checked: an app that answers late or not at all gets a clean outcome (the window plainly where it should be)
//! rather than a half state. Each flight leaves a line in `flights.log` (the last 20), with the time of every step.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Pos2, Rect, Vec2};

/// What the flows ask of one window. The real one talks to the app over Accessibility; tests use a stand-in that
/// answers slowly, refuses, or moves the window on its own.
pub trait Ax {
    fn frame(&self) -> Option<Rect>;
    fn move_to(&self, to: Pos2);
    fn resize(&self, size: Vec2);
    fn set_minimized(&self, minimized: bool);
    fn minimized(&self) -> Option<bool>;
    fn raise(&self);
}

/// How long each step may take.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// For the app to say the window is down (or up), and again after asking a second time.
    pub answer: Duration,
    pub again: Duration,
    /// For the picture to land over the window's place, coming up.
    pub landing: Duration,
    /// For the window to stay in its place and size, coming up.
    pub placing: Duration,
    /// Between looks at the window.
    pub poll: Duration,
}

pub const LIMITS: Limits = Limits {
    answer: Duration::from_millis(1200),
    again: Duration::from_millis(800),
    landing: Duration::from_millis(2500),
    placing: Duration::from_millis(1000),
    poll: Duration::from_millis(25),
};

/// How going down ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Down {
    Minimized,
    /// The app never said the window went down: it's put back where it was, still up.
    Refused,
}

/// How coming up ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Up {
    /// The app said the window is up.
    pub up: bool,
    /// The window stayed in its place and size.
    pub placed: bool,
}

fn wait_for(limit: Duration, poll: Duration, done: &mut dyn FnMut() -> bool) -> bool {
    let until = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(poll);
    }
}

/// Parks the window at `spot` (just under a screen's bottom, under the cover) and minimizes it, asking twice if the
/// app is slow. A window too tall for that screen is first made `fit`: macOS pulls a window taller than the screen
/// it's moved to right up under that screen's menu bar, in plain sight, and its own minimize then plays from there.
/// An app that never minimizes it gets it back at `home`, as it was.
pub fn go_down(ax: &dyn Ax, spot: Pos2, fit: Option<Vec2>, home: Rect, limits: Limits, trace: &Trace) -> Down {
    if let Some(fit) = fit {
        ax.resize(fit);
        trace.step(|| format!("shrunk to {fit:?} to park"));
    }
    ax.move_to(spot);
    trace.step(|| format!("parked {:?}", ax.frame().map(|f| f.min)));
    ax.set_minimized(true);
    let mut down = || ax.minimized() == Some(true);
    if wait_for(limits.answer, limits.poll, &mut down) {
        trace.step(|| "minimized".into());
        return Down::Minimized;
    }
    trace.step(|| "slow to minimize: asked again".into());
    ax.set_minimized(true);
    if wait_for(limits.again, limits.poll, &mut down) {
        trace.step(|| "minimized".into());
        return Down::Minimized;
    }
    if fit.is_some() {
        ax.resize(home.size());
    }
    ax.move_to(home.min);
    trace.step(|| "never minimized: put back".into());
    Down::Refused
}

/// Brings the window up at `spot` (under the cover; made `fit` first when it's too tall to stay there, as in
/// `go_down`), waits in `corner` (a point of it on screen) until the picture has landed, then puts it in `place`, at
/// its own size, and raises it. Bounded throughout: a window that won't come up, or won't stay put, is raised as it
/// is.
#[allow(clippy::too_many_arguments)]
pub fn come_up(
    ax: &dyn Ax,
    spot: Pos2,
    fit: Option<Vec2>,
    corner: Pos2,
    place: Rect,
    landed: &AtomicBool,
    cornered: &dyn Fn(),
    limits: Limits,
    trace: &Trace,
) -> Up {
    if let Some(fit) = fit {
        ax.resize(fit);
        trace.step(|| format!("shrunk to {fit:?} to park"));
    }
    ax.move_to(spot);
    ax.set_minimized(false);
    let mut is_up = || ax.minimized() == Some(false);
    let mut up = wait_for(limits.answer, limits.poll, &mut is_up);
    if !up {
        trace.step(|| "slow to come up: asked again".into());
        ax.set_minimized(false);
        up = wait_for(limits.again, limits.poll, &mut is_up);
    }
    trace.step(|| format!("up {up}"));
    ax.move_to(corner);
    cornered();
    let on_time = wait_for(limits.landing, Duration::from_millis(4), &mut || landed.load(Ordering::Relaxed));
    trace.step(|| if on_time { "picture landed".into() } else { "picture late".into() });
    // Some apps move or resize a window again as it comes up: keep putting it back until it stays.
    let placed = wait_for(limits.placing, limits.poll, &mut || {
        let f = ax.frame();
        if f.is_some_and(|f| (f.min - place.min).length() < 2.0 && (f.size() - place.size()).length() < 2.0) {
            return true;
        }
        ax.move_to(place.min);
        if f.is_some_and(|f| (f.size() - place.size()).length() >= 2.0) {
            ax.resize(place.size());
        }
        false
    });
    trace.step(|| format!("placed {placed}"));
    ax.raise();
    trace.step(|| "raised".into());
    Up { up, placed }
}

/// The steps of one flight, with how long after its start each came. Written to the log once everyone holding it is
/// done (the flight on screen and the thread working on the window).
#[derive(Clone)]
pub struct Trace(Arc<Mutex<Steps>>);

struct Steps {
    start: Instant,
    wall: String,
    head: String,
    steps: Vec<String>,
    log: Option<PathBuf>,
}

/// Flights kept in the log.
const KEPT: usize = 20;

/// While a stress run goes, every flight's steps also go to its log.
static MIRROR: AtomicBool = AtomicBool::new(false);

pub fn mirror(on: bool) {
    MIRROR.store(on, Ordering::Relaxed);
}

impl Trace {
    pub fn new(head: String, log: Option<PathBuf>) -> Self {
        Self(Arc::new(Mutex::new(Steps { start: Instant::now(), wall: wall_clock(), head, steps: Vec::new(), log })))
    }

    /// One that writes nowhere.
    #[cfg(test)]
    pub fn quiet() -> Self {
        Self::new(String::new(), None)
    }

    pub fn step(&self, what: impl FnOnce() -> String) {
        if let Ok(mut s) = self.0.lock() {
            let ms = s.start.elapsed().as_millis();
            let line = format!("{ms} {}", what());
            crate::recorder::event(|| line.clone());
            s.steps.push(line);
        }
    }

    #[cfg(test)]
    pub fn steps(&self) -> Vec<String> {
        self.0.lock().map(|s| s.steps.clone()).unwrap_or_default()
    }
}

impl Drop for Steps {
    fn drop(&mut self) {
        let Some(log) = &self.log else { return };
        let line = format!("{} {} | {}", self.wall, self.head, self.steps.join(" | "));
        if MIRROR.load(Ordering::Relaxed) {
            crate::stress::log(&format!("  {line}"));
        }
        let old = std::fs::read_to_string(log).unwrap_or_default();
        let _ = std::fs::write(log, keep_last(&old, &line, KEPT));
    }
}

/// The log with `line` added, keeping only the newest `kept` lines.
fn keep_last(old: &str, line: &str, kept: usize) -> String {
    let mut lines: Vec<&str> = old.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.push(line);
    let from = lines.len().saturating_sub(kept);
    let mut out = lines[from..].join("\n");
    out.push('\n');
    out
}

/// The local date and time, to the second.
fn wall_clock() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let Some(tm) = crate::clock::local(now as i64) else { return String::new() };
    format!(
        "{}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// The log of the last flights.
pub fn log_file() -> PathBuf {
    crate::lifecycle::data_dir().join("flights.log")
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;

    /// A window in an app that answers after `lag` and may be difficult.
    struct Fake {
        state: Mutex<(Rect, bool)>,
        lag: Duration,
        /// Asks to minimize (or restore) are ignored this many times.
        ignore: Mutex<u32>,
        /// After coming up, the app puts the window here instead, this many times.
        wanders: Mutex<u32>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl Fake {
        fn new(minimized: bool) -> Self {
            Self {
                state: Mutex::new((Rect::from_min_size(pos2(200.0, 150.0), vec2(800.0, 500.0)), minimized)),
                lag: Duration::ZERO,
                ignore: Mutex::new(0),
                wanders: Mutex::new(0),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn wait(&self, call: &'static str) {
            self.calls.lock().unwrap().push(call);
            std::thread::sleep(self.lag);
        }
    }

    impl Ax for Fake {
        fn frame(&self) -> Option<Rect> {
            self.wait("frame");
            Some(self.state.lock().unwrap().0)
        }
        fn move_to(&self, to: Pos2) {
            self.wait("move");
            let mut s = self.state.lock().unwrap();
            s.0 = Rect::from_min_size(to, s.0.size());
        }
        fn resize(&self, size: Vec2) {
            self.wait("resize");
            let mut s = self.state.lock().unwrap();
            s.0 = Rect::from_min_size(s.0.min, size);
        }
        fn set_minimized(&self, minimized: bool) {
            self.wait("minimize");
            let mut ignore = self.ignore.lock().unwrap();
            if *ignore > 0 {
                *ignore -= 1;
                return;
            }
            let mut s = self.state.lock().unwrap();
            s.1 = minimized;
            let mut wanders = self.wanders.lock().unwrap();
            if !minimized && *wanders > 0 {
                *wanders -= 1;
                s.0 = Rect::from_min_size(pos2(40.0, 40.0), vec2(640.0, 400.0));
            }
        }
        fn minimized(&self) -> Option<bool> {
            self.wait("ask");
            Some(self.state.lock().unwrap().1)
        }
        fn raise(&self) {
            self.wait("raise");
        }
    }

    const FAST: Limits = Limits {
        answer: Duration::from_millis(120),
        again: Duration::from_millis(80),
        landing: Duration::from_millis(200),
        placing: Duration::from_millis(150),
        poll: Duration::from_millis(5),
    };

    const SPOT: Pos2 = Pos2 { x: 600.0, y: 860.0 };
    const HOME: Pos2 = Pos2 { x: 200.0, y: 150.0 };
    const HOME_FRAME: Rect = Rect { min: HOME, max: Pos2 { x: 1000.0, y: 650.0 } };

    #[test]
    fn a_window_too_tall_for_the_screen_it_parks_under_is_made_to_fit_first_and_comes_back_whole() {
        let ax = Fake::new(false);
        let fit = vec2(800.0, 300.0);
        let trace = Trace::quiet();
        assert_eq!(go_down(&ax, SPOT, Some(fit), HOME_FRAME, FAST, &trace), Down::Minimized);
        let calls = ax.calls.lock().unwrap().clone();
        let (resize, park) = (calls.iter().position(|c| *c == "resize"), calls.iter().position(|c| *c == "move"));
        assert!(resize.is_some() && resize < park, "made to fit before it moves: {calls:?}");
        assert_eq!(ax.state.lock().unwrap().0, Rect::from_min_size(SPOT, fit));
        // Coming back up: shrunk again under the cover (a window minimized some other way is still tall), then
        // placed at its own size.
        let landed = AtomicBool::new(true);
        let up = come_up(&ax, SPOT, Some(fit), pos2(1469.0, 955.0), HOME_FRAME, &landed, &|| {}, FAST, &trace);
        assert_eq!(up, Up { up: true, placed: true });
        assert_eq!(ax.state.lock().unwrap().0, HOME_FRAME, "back where it was, as big as it was");
    }

    #[test]
    fn a_tall_window_an_app_never_minimizes_gets_its_size_back_too() {
        let ax = Fake::new(false);
        *ax.ignore.lock().unwrap() = 99;
        assert_eq!(go_down(&ax, SPOT, Some(vec2(800.0, 300.0)), HOME_FRAME, FAST, &Trace::quiet()), Down::Refused);
        assert_eq!(ax.state.lock().unwrap().0, HOME_FRAME);
    }

    #[test]
    fn a_window_goes_down_parked_out_of_sight() {
        let ax = Fake::new(false);
        assert_eq!(go_down(&ax, SPOT, None, HOME_FRAME, FAST, &Trace::quiet()), Down::Minimized);
        let (frame, down) = *ax.state.lock().unwrap();
        assert!(down);
        assert_eq!(frame.min, SPOT);
    }

    #[test]
    fn an_app_that_ignores_the_first_ask_is_asked_again() {
        let ax = Fake::new(false);
        *ax.ignore.lock().unwrap() = 1;
        let trace = Trace::quiet();
        assert_eq!(go_down(&ax, SPOT, None, HOME_FRAME, FAST, &trace), Down::Minimized);
        assert!(trace.steps().iter().any(|s| s.contains("asked again")));
    }

    #[test]
    fn an_app_that_never_minimizes_gets_its_window_back_where_it_was_in_bounded_time() {
        let ax = Fake::new(false);
        *ax.ignore.lock().unwrap() = 99;
        let started = Instant::now();
        assert_eq!(go_down(&ax, SPOT, None, HOME_FRAME, FAST, &Trace::quiet()), Down::Refused);
        assert!(started.elapsed() < FAST.answer + FAST.again + Duration::from_millis(150));
        assert_eq!(ax.state.lock().unwrap().0.min, HOME, "never left parked at the bottom");
    }

    #[test]
    fn a_slow_app_still_finishes_within_the_limits() {
        // Every question takes 40 ms, as a busy app's do.
        let mut ax = Fake::new(false);
        ax.lag = Duration::from_millis(40);
        let started = Instant::now();
        assert_eq!(go_down(&ax, SPOT, None, HOME_FRAME, FAST, &Trace::quiet()), Down::Minimized);
        assert!(started.elapsed() < Duration::from_millis(400));
        let landed = AtomicBool::new(true);
        let place = Rect::from_min_size(HOME, vec2(800.0, 500.0));
        let started = Instant::now();
        let up = come_up(&ax, SPOT, None, pos2(1469.0, 955.0), place, &landed, &|| {}, FAST, &Trace::quiet());
        assert_eq!(up, Up { up: true, placed: true });
        assert!(started.elapsed() < Duration::from_millis(600));
    }

    #[test]
    fn a_window_comes_up_in_its_place_once_the_picture_has_landed() {
        let ax = Fake::new(true);
        let place = Rect::from_min_size(HOME, vec2(800.0, 500.0));
        let landed = Arc::new(AtomicBool::new(false));
        let later = Arc::clone(&landed);
        let lands = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            later.store(true, Ordering::Relaxed);
        });
        let trace = Trace::quiet();
        let up = come_up(&ax, SPOT, None, pos2(1469.0, 955.0), place, &landed, &|| {}, FAST, &trace);
        lands.join().unwrap();
        assert_eq!(up, Up { up: true, placed: true });
        assert_eq!(ax.state.lock().unwrap().0, place);
        // It waited in the corner until the picture was over its place, and only then went there.
        let steps = trace.steps();
        let landed_at = steps.iter().position(|s| s.contains("picture landed")).unwrap();
        let placed_at = steps.iter().position(|s| s.contains("placed true")).unwrap();
        assert!(landed_at < placed_at);
    }

    #[test]
    fn a_window_its_app_moves_and_resizes_on_the_way_up_is_put_back() {
        let ax = Fake::new(true);
        *ax.wanders.lock().unwrap() = 1;
        let place = Rect::from_min_size(HOME, vec2(800.0, 500.0));
        let landed = AtomicBool::new(true);
        let up = come_up(&ax, SPOT, None, pos2(1469.0, 955.0), place, &landed, &|| {}, FAST, &Trace::quiet());
        assert!(up.placed);
        assert_eq!(ax.state.lock().unwrap().0, place);
    }

    #[test]
    fn a_window_that_wont_come_up_is_still_raised_in_bounded_time() {
        let ax = Fake::new(true);
        *ax.ignore.lock().unwrap() = 99;
        let place = Rect::from_min_size(HOME, vec2(800.0, 500.0));
        let landed = AtomicBool::new(true);
        let started = Instant::now();
        let up = come_up(&ax, SPOT, None, pos2(1469.0, 955.0), place, &landed, &|| {}, FAST, &Trace::quiet());
        assert!(!up.up);
        assert!(ax.calls.lock().unwrap().contains(&"raise"));
        assert!(started.elapsed() < FAST.answer + FAST.again + FAST.placing + Duration::from_millis(200));
    }

    #[test]
    fn a_picture_that_never_lands_doesnt_hold_the_window_forever() {
        let ax = Fake::new(true);
        let place = Rect::from_min_size(HOME, vec2(800.0, 500.0));
        let landed = AtomicBool::new(false);
        let trace = Trace::quiet();
        let up = come_up(&ax, SPOT, None, pos2(1469.0, 955.0), place, &landed, &|| {}, FAST, &trace);
        assert!(up.placed);
        assert!(trace.steps().iter().any(|s| s.contains("picture late")));
    }

    #[test]
    fn the_log_keeps_the_last_twenty_flights() {
        let mut log = String::new();
        for i in 0..30 {
            log = keep_last(&log, &format!("flight {i}"), KEPT);
        }
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 20);
        assert_eq!(lines[0], "flight 10");
        assert_eq!(lines[19], "flight 29");
    }

    #[test]
    fn a_trace_is_written_once_everyone_is_done_with_it() {
        let dir = std::env::temp_dir().join(format!("notchbase-flow-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("flights.log");
        let _ = std::fs::remove_file(&log);
        let trace = Trace::new("down 7 TextEdit".into(), Some(log.clone()));
        let thread = trace.clone();
        std::thread::spawn(move || thread.step(|| "minimized".into())).join().unwrap();
        assert!(!log.exists(), "the flight still holds it");
        drop(trace);
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("down 7 TextEdit |") && text.contains("minimized"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
