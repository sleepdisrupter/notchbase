//! The app switcher: Cmd+Tab shows, on the screen the pointer is on, the apps with a window open on that screen
//! right now (never hidden apps, minimized windows, other Spaces or other screens), most recently used first, each
//! as a live
//! picture of its front window with its icon, on a floating panel of the dock's smoked black glass. Tab and
//! Shift+Tab (or ` and the arrow keys, or the pointer) move along; letting go of Cmd switches; Esc cancels; Q quits
//! and H hides the app picked. A quick Cmd+Tab switches straight back to the last app without showing the panel.
//!
//! The keys come from the dock's event tap (`titlebar.rs`), on its own thread; macOS's own switcher is turned off
//! while this one catches them and back on whenever it doesn't (and on the way out).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use notchbase_paint::color::{PEARL, white};
use notchbase_paint::font;
use notchbase_paint::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, lerp};

/// A Cmd+Tab let go of sooner than this switches without the panel ever showing.
pub const SHOW_AFTER: Duration = Duration::from_millis(110);
/// The panel growing in and fading out: the island opening and closing.
const APPEAR: SpringConfig = SpringConfig::new(0.30, 0.86);
const DISAPPEAR: SpringConfig = SpringConfig::new(0.14, 1.0);
/// The highlight gliding from one app to the next.
const GLIDE: SpringConfig = SpringConfig::new(0.22, 0.9);
/// How often the pictures are taken again while the panel shows.
const RECAPTURE: Duration = Duration::from_millis(500);

// The layout, in points.
const PREVIEW: Vec2 = vec2(220.0, 140.0);
const LABEL: f32 = 26.0;
const GAP: f32 = 14.0;
const PAD: f32 = 18.0;
const RADIUS: f32 = 30.0;
const PREVIEW_CORNER: f32 = 10.0;
const ICON: f32 = 46.0;
/// The panel never takes more of the screen's width than this; past it the pictures shrink, then wrap.
const MOST_WIDTH: f32 = 0.9;
const SMALLEST_SCALE: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Next,
    Prev,
    Esc,
    Quit,
    Hide,
    /// Cmd let go.
    Release,
    /// Return: switch now.
    Choose,
    Click(Pos2),
}

// Shared with the event tap's thread.
static CATCHING: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static SWALLOW_UP: AtomicBool = AtomicBool::new(false);
static KEYS: Mutex<Vec<Key>> = Mutex::new(Vec::new());
type Wake = Box<dyn Fn() + Send + Sync>;
static WAKE: OnceLock<Wake> = OnceLock::new();

const COMMAND: u64 = 0x10_0000;
const SHIFT: u64 = 0x2_0000;
const CONTROL: u64 = 0x4_0000;
const OPTION: u64 = 0x8_0000;

pub fn on_wake(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Box::new(wake));
}

fn push(key: Key) {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).push(key);
    if let Some(wake) = WAKE.get() {
        wake();
    }
}

/// Whether Cmd+Tab is caught: on while the dock is here, off while it's away. macOS's own switcher takes over
/// whenever it's off.
pub fn set_catching(on: bool) {
    if CATCHING.swap(on, Ordering::Relaxed) != on {
        system_switcher(!on);
        if !on {
            ACTIVE.store(false, Ordering::Relaxed);
        }
    }
}

/// Turns macOS's own Cmd+Tab (and Cmd+Shift+Tab) on or off. It stays as set until logout, so it's always turned
/// back on when the dock goes (and by `--restore-dock` after a crash).
pub fn system_switcher(enabled: bool) {
    type Set = unsafe extern "C" fn(u32, bool) -> i32;
    let f = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"CGSSetSymbolicHotKeyEnabled".as_ptr()) };
    if f.is_null() {
        return;
    }
    let set = unsafe { std::mem::transmute::<*mut libc::c_void, Set>(f) };
    // 1: Cmd+Tab, 2: Cmd+Shift+Tab.
    for hotkey in [1, 2] {
        unsafe { set(hotkey, enabled) };
    }
}

/// A key went down (on the tap's thread). True when it's the switcher's, to keep it from the app in front.
pub fn on_key(keycode: i64, flags: u64) -> bool {
    if !CATCHING.load(Ordering::Relaxed) {
        return false;
    }
    let shift = flags & SHIFT != 0;
    if !ACTIVE.load(Ordering::Relaxed) {
        if keycode == 48 && flags & COMMAND != 0 && flags & (CONTROL | OPTION) == 0 {
            ACTIVE.store(true, Ordering::Relaxed);
            push(if shift { Key::Prev } else { Key::Next });
            return true;
        }
        return false;
    }
    let key = match keycode {
        48 => {
            if shift {
                Key::Prev
            } else {
                Key::Next
            }
        }
        50 => {
            if shift {
                Key::Next
            } else {
                Key::Prev
            }
        }
        123 | 126 => Key::Prev,
        124 | 125 => Key::Next,
        53 => Key::Esc,
        12 => Key::Quit,
        4 => Key::Hide,
        36 | 76 => Key::Choose,
        _ => return false,
    };
    push(key);
    true
}

/// The modifier keys changed (on the tap's thread): letting go of Cmd switches.
pub fn on_flags(flags: u64) {
    if ACTIVE.load(Ordering::Relaxed) && flags & COMMAND == 0 {
        push(Key::Release);
    }
}

/// A click while the switcher is up is the switcher's.
pub fn on_mouse_down(at: Pos2) -> bool {
    if !ACTIVE.load(Ordering::Relaxed) {
        return false;
    }
    SWALLOW_UP.store(true, Ordering::Relaxed);
    push(Key::Click(at));
    true
}

pub fn on_mouse_up() -> bool {
    SWALLOW_UP.swap(false, Ordering::Relaxed)
}

pub fn take_keys() -> Vec<Key> {
    std::mem::take(&mut *KEYS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The switcher is done: keys go back to the apps.
pub fn finished() {
    ACTIVE.store(false, Ordering::Relaxed);
}

/// One app in the switcher.
pub struct Entry {
    pub pid: i32,
    pub name: String,
    /// The window its picture shows: its front one on screen.
    pub window: Option<u32>,
    /// Hidden with H while the switcher is up: shown dimmed until it closes.
    pub hidden: bool,
    pub icon: Option<egui::ColorImage>,
    pub picture: Option<egui::ColorImage>,
    icon_texture: Option<egui::TextureHandle>,
    picture_texture: Option<egui::TextureHandle>,
    picture_fresh: bool,
}

impl Entry {
    pub fn new(pid: i32, name: String) -> Self {
        Self {
            pid,
            name,
            window: None,
            hidden: false,
            icon: None,
            picture: None,
            icon_texture: None,
            picture_texture: None,
            picture_fresh: false,
        }
    }
}

/// A real window, as the switcher sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listed {
    pub pid: i32,
    pub id: u32,
    /// Minimized windows and windows on other Spaces aren't.
    pub on_screen: bool,
    /// Where it is, in global top-left points.
    pub frame: Rect,
}

/// Which of `screens` (global top-left points) a window is on: the one holding most of it, as macOS decides. None
/// when it's on none of them.
pub fn screen_of(frame: Rect, screens: &[Rect]) -> Option<usize> {
    let shared = |s: &Rect| {
        let both = s.intersect(frame);
        if both.is_positive() { both.area() } else { 0.0 }
    };
    let (i, most) = screens.iter().map(shared).enumerate().max_by(|a, b| a.1.total_cmp(&b.1))?;
    (most > 0.0).then_some(i)
}

/// The screen the pointer is on, or the nearest one when it's between them.
pub fn screen_at(p: Pos2, screens: &[Rect]) -> Option<usize> {
    screens.iter().position(|s| s.contains(p)).or_else(|| {
        let away = |s: &Rect| s.distance_sq_to_pos(p);
        (0..screens.len()).min_by(|&a, &b| away(&screens[a]).total_cmp(&away(&screens[b])))
    })
}

/// Which apps the switcher shows on screen `on` of `screens`, and the window each one's picture shows: only apps
/// with a real window open on that screen right now. `apps` are the Dock's kind of apps, (pid, hidden); `windows`
/// are front to back, already only real windows of a real size. Hidden apps, minimized windows (never on screen),
/// windows on other Spaces (not on screen either) and windows mostly on another screen don't count.
pub fn shown_apps(apps: &[(i32, bool)], windows: &[Listed], screens: &[Rect], on: usize) -> Vec<(i32, u32)> {
    apps.iter()
        .filter(|(_, hidden)| !hidden)
        .filter_map(|&(pid, _)| {
            windows
                .iter()
                .find(|w| w.pid == pid && w.on_screen && screen_of(w.frame, screens) == Some(on))
                .map(|w| (pid, w.id))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub panel: Rect,
    /// Each app's picture.
    pub previews: Vec<Rect>,
    pub scale: f32,
}

/// Where everything goes for `n` apps on a screen this size: one centred row, shrinking to fit, wrapping last.
pub fn layout(n: usize, screen: Vec2) -> Layout {
    let n = n.max(1);
    let most = screen.x * MOST_WIDTH - PAD * 2.0;
    let width_at = |scale: f32, cols: usize| cols as f32 * PREVIEW.x * scale + (cols - 1) as f32 * GAP;
    let scale = ((most - (n - 1) as f32 * GAP) / (n as f32 * PREVIEW.x)).clamp(SMALLEST_SCALE, 1.0);
    let mut cols = n;
    while cols > 1 && width_at(scale, cols) > most {
        cols -= 1;
    }
    let rows = n.div_ceil(cols);
    let cell = vec2(PREVIEW.x * scale, PREVIEW.y * scale + LABEL);
    let size = vec2(width_at(scale, cols) + PAD * 2.0, rows as f32 * cell.y + (rows - 1) as f32 * GAP + PAD * 2.0);
    let panel = Rect::from_center_size(pos2(screen.x * 0.5, screen.y * 0.5), size);
    let previews = (0..n)
        .map(|i| {
            let (row, col) = (i / cols, i % cols);
            // The last row is centred under the others.
            let in_row = if row == rows - 1 { n - row * cols } else { cols };
            let row_width = width_at(scale, in_row);
            let left = panel.center().x - row_width * 0.5;
            let x = left + col as f32 * (cell.x + GAP);
            let y = panel.top() + PAD + row as f32 * (cell.y + GAP);
            Rect::from_min_size(pos2(x, y), PREVIEW * scale)
        })
        .collect();
    Layout { panel, previews, scale }
}

pub struct Switcher {
    pub entries: Vec<Entry>,
    pub selected: usize,
    opened: Instant,
    screen: Vec2,
    pub layout: Layout,
    show: Spring,
    glide: [Spring; 2],
    pub closing: bool,
    /// Pictures arriving from the thread that takes them.
    arriving: Arc<Mutex<Vec<(u32, egui::ColorImage)>>>,
    capturing: Arc<AtomicBool>,
    captured: Option<Instant>,
    pointer: Option<Pos2>,
}

impl Switcher {
    pub fn new(entries: Vec<Entry>, forward: bool, front: Option<i32>, screen: Vec2) -> Self {
        let n = entries.len();
        // Forward from the app in front is the one used before it; backward is the oldest.
        let first_is_front = entries.first().is_some_and(|e| Some(e.pid) == front);
        let selected = match (forward, first_is_front) {
            (true, true) if n > 1 => 1,
            (true, _) => 0,
            (false, _) => n.saturating_sub(1),
        };
        let layout = layout(n, screen);
        let at = layout.previews.get(selected).map_or(Pos2::ZERO, |r| r.center());
        Self {
            entries,
            selected,
            opened: Instant::now(),
            screen,
            layout,
            show: Spring::new(0.0, APPEAR, UNIT_PRECISION),
            glide: [Spring::new(at.x, GLIDE, POINT_PRECISION), Spring::new(at.y, GLIDE, POINT_PRECISION)],
            closing: false,
            arriving: Arc::new(Mutex::new(Vec::new())),
            capturing: Arc::new(AtomicBool::new(false)),
            captured: None,
            pointer: None,
        }
    }

    /// Whether the panel is up, or should be by now.
    pub fn showing(&self) -> bool {
        !self.closing && self.opened.elapsed() >= SHOW_AFTER
    }

    pub fn visible(&self) -> bool {
        self.show.value > 0.003 || self.showing()
    }

    /// Gone: closed and faded out.
    pub fn gone(&self) -> bool {
        self.closing && self.show.value <= 0.003
    }

    pub fn step_by(&mut self, by: isize) {
        let n = self.entries.len() as isize;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as isize + by).rem_euclid(n) as usize;
    }

    pub fn close(&mut self) {
        self.closing = true;
        finished();
    }

    /// An app left (quit): its place closes up.
    pub fn remove(&mut self, i: usize) {
        if i < self.entries.len() {
            self.entries.remove(i);
        }
        if self.entries.is_empty() {
            self.close();
            return;
        }
        self.selected = self.selected.min(self.entries.len() - 1);
        self.layout = layout(self.entries.len(), self.screen);
    }

    pub fn at(&self, p: Pos2) -> Option<usize> {
        let cell = |r: &Rect| Rect::from_min_max(r.min, pos2(r.max.x, r.max.y + LABEL)).expand(GAP * 0.5);
        self.layout.previews.iter().position(|r| cell(r).contains(p))
    }

    /// The pointer moved over an app: it's picked, as the pointer does in macOS's switcher.
    pub fn hover(&mut self, p: Pos2) {
        if self.pointer.is_some_and(|last| (last - p).length() < 0.5) {
            return;
        }
        let first = self.pointer.is_none();
        self.pointer = Some(p);
        // Where the pointer already rests when the panel opens doesn't count.
        if !first && let Some(i) = self.at(p) {
            self.selected = i;
        }
    }

    pub fn step(&mut self, dt: f32) {
        let target = if self.showing() {
            1.0
        } else if self.closing {
            0.0
        } else {
            self.show.target
        };
        if target != self.show.target {
            self.show.animate_to(target, if target > 0.5 { APPEAR } else { DISAPPEAR });
        }
        self.show.step(dt);
        if let Some(r) = self.layout.previews.get(self.selected) {
            let c = r.center();
            for (s, v) in self.glide.iter_mut().zip([c.x, c.y]) {
                if s.target != v {
                    s.animate_to(v, GLIDE);
                }
                s.step(dt);
            }
        }
        for (id, picture) in std::mem::take(&mut *self.arriving.lock().unwrap_or_else(|e| e.into_inner())) {
            for e in self.entries.iter_mut().filter(|e| e.window == Some(id)) {
                e.picture = Some(picture.clone());
                e.picture_fresh = true;
            }
        }
        self.capture();
    }

    pub fn animating(&self) -> bool {
        !self.show.is_settled() || self.glide.iter().any(|s| !s.is_settled()) || self.visible()
    }

    /// Takes every app's picture again on a thread of its own, now and then while the panel shows.
    fn capture(&mut self) {
        if self.closing || self.captured.is_some_and(|at| at.elapsed() < RECAPTURE) {
            return;
        }
        if self.capturing.swap(true, Ordering::Relaxed) {
            return;
        }
        self.captured = Some(Instant::now());
        let ids: Vec<u32> = self.entries.iter().filter_map(|e| e.window).collect();
        let px = (PREVIEW.y * 2.0) as u32;
        let (arriving, capturing) = (Arc::clone(&self.arriving), Arc::clone(&self.capturing));
        let spawned = std::thread::Builder::new().name("switcher pictures".into()).spawn(move || {
            for id in ids {
                if let Some(picture) = crate::windows::picture(id, px) {
                    arriving.lock().unwrap_or_else(|e| e.into_inner()).push((id, picture));
                }
            }
            capturing.store(false, Ordering::Relaxed);
        });
        if spawned.is_err() {
            self.capturing.store(false, Ordering::Relaxed);
        }
    }

    /// The panel's glass, for the liquid: (blobs, alpha).
    pub fn glass(&self) -> (Vec<crate::liquid::Blob>, f32) {
        let k = self.show.value.clamp(0.0, 1.2);
        let grow = lerp(0.94, 1.0, k.min(1.0));
        let panel = Rect::from_center_size(self.layout.panel.center(), self.layout.panel.size() * grow);
        (vec![crate::liquid::Blob::rect(panel, RADIUS * grow, 0.0)], 0.9 * k.min(1.0))
    }

    pub fn paint(&mut self, painter: &egui::Painter) {
        let k = self.show.value.clamp(0.0, 1.0);
        if k <= 0.003 {
            return;
        }
        let grow = lerp(0.94, 1.0, k);
        let centre = self.layout.panel.center();
        let at = |r: Rect| Rect::from_center_size(centre + (r.center() - centre) * grow, r.size() * grow);
        let panel = at(self.layout.panel);
        // The island's keyline: a hairline of light just inside the edge.
        painter.rect_stroke(panel, RADIUS * grow, Stroke::new(1.0, white(0.13 * k)), egui::StrokeKind::Inside);

        // The highlight, gliding from app to app.
        let glide = pos2(self.glide[0].value, self.glide[1].value);
        let scale = self.layout.scale;
        let plate = Rect::from_center_size(
            glide + vec2(0.0, LABEL * 0.5 - 2.0),
            vec2(PREVIEW.x * scale + 12.0, PREVIEW.y * scale + LABEL + 8.0),
        );
        let plate = at(plate);
        painter.rect_filled(plate, 16.0 * grow, white(0.10 * k));
        painter.rect_stroke(plate, 16.0 * grow, Stroke::new(1.0, white(0.16 * k)), egui::StrokeKind::Inside);

        let options = egui::TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification: egui::TextureFilter::Linear,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
            mipmap_mode: Some(egui::TextureFilter::Linear),
        };
        let ctx = painter.ctx().clone();
        let previews = self.layout.previews.clone();
        for (i, (e, r)) in self.entries.iter_mut().zip(previews).enumerate() {
            let r = at(r);
            let dim = if e.hidden { 0.55 } else { 1.0 };
            if e.picture_fresh || (e.picture_texture.is_none() && e.picture.is_some()) {
                e.picture_fresh = false;
                if let Some(p) = e.picture.clone() {
                    match &mut e.picture_texture {
                        Some(t) => t.set(p, options),
                        None => e.picture_texture = Some(ctx.load_texture(format!("switch {}", e.pid), p, options)),
                    }
                }
            }
            if e.icon_texture.is_none()
                && let Some(icon) = e.icon.clone()
            {
                e.icon_texture = Some(ctx.load_texture(format!("switch icon {}", e.pid), icon, options));
            }
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            // The picture, fitted inside its place without stretching.
            if let (Some(t), Some(p)) = (&e.picture_texture, &e.picture) {
                let aspect = p.size[0] as f32 / p.size[1].max(1) as f32;
                let size = if aspect > r.width() / r.height() {
                    vec2(r.width(), r.width() / aspect)
                } else {
                    vec2(r.height() * aspect, r.height())
                };
                let shown = Rect::from_center_size(r.center(), size);
                painter.add(
                    egui::epaint::RectShape::filled(
                        shown,
                        PREVIEW_CORNER * grow,
                        Color32::WHITE.gamma_multiply(k * dim),
                    )
                    .with_texture(t.id(), uv),
                );
            }
            // The app's icon: over the bottom of its picture, or large in its place when there's no picture.
            if let Some(t) = &e.icon_texture {
                let icon = if e.picture_texture.is_some() {
                    Rect::from_center_size(pos2(r.center().x, r.bottom() - ICON * 0.3 * grow), Vec2::splat(ICON * grow))
                } else {
                    Rect::from_center_size(r.center(), Vec2::splat(r.height() * 0.62))
                };
                painter.image(t.id(), icon, uv, Color32::WHITE.gamma_multiply(k));
            }
            // Its name, under it, brighter for the one picked.
            let picked = i == self.selected;
            let colour = if picked { PEARL.gamma_multiply(k) } else { white(0.62 * k) };
            let mut name = e.name.clone();
            let room = r.width() + GAP * 0.6;
            let mut galley = painter.layout_no_wrap(name.clone(), font::semibold(12.0), colour);
            while galley.size().x > room && name.chars().count() > 2 {
                name.pop();
                galley = painter.layout_no_wrap(format!("{}…", name.trim_end()), font::semibold(12.0), colour);
            }
            let top = r.bottom() + (LABEL - galley.size().y) * 0.5 + 3.0 * grow;
            painter.galley(pos2(r.center().x - galley.size().x * 0.5, top), galley, colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Vec2 = vec2(1470.0, 956.0);

    /// A laptop's screen, with a bigger monitor above it and a bit to the left.
    fn two_screens() -> [Rect; 2] {
        [Rect::from_min_size(pos2(0.0, 0.0), SCREEN), Rect::from_min_size(pos2(-60.0, -1080.0), vec2(1920.0, 1080.0))]
    }

    fn listed(pid: i32, id: u32, on_screen: bool, at: Pos2) -> Listed {
        Listed { pid, id, on_screen, frame: Rect::from_min_size(at, vec2(800.0, 600.0)) }
    }

    #[test]
    fn only_apps_with_a_window_on_screen_show() {
        let screens = two_screens();
        let here = pos2(100.0, 100.0);
        let apps = [(1, false), (2, false), (3, true), (4, false), (5, false)];
        let windows = [
            listed(1, 11, true, here),
            listed(1, 12, true, here),
            // 2: only minimized (off screen).
            listed(2, 21, false, here),
            // 3: hidden, though its window says on screen.
            listed(3, 31, true, here),
            // 4: off screen first (another Space), then on screen.
            listed(4, 41, false, here),
            listed(4, 42, true, here),
            // 6: not one of the Dock's apps (a menu bar app, or this dock).
            listed(6, 61, true, here),
        ];
        assert_eq!(
            shown_apps(&apps, &windows, &screens, 0),
            vec![(1, 11), (4, 42)],
            "the front window on screen of each"
        );
        assert!(shown_apps(&apps, &[], &screens, 0).is_empty(), "no windows, nothing to show");
    }

    #[test]
    fn each_screen_shows_only_its_own_windows() {
        let screens = two_screens();
        let (below, above) = (pos2(100.0, 100.0), pos2(200.0, -900.0));
        let apps = [(1, false), (2, false), (3, false)];
        let windows = [
            // 1: its front window is above, another one below.
            listed(1, 11, true, above),
            listed(1, 12, true, below),
            // 2: only below.
            listed(2, 21, true, below),
            // 3: only above.
            listed(3, 31, true, above),
        ];
        assert_eq!(shown_apps(&apps, &windows, &screens, 0), vec![(1, 12), (2, 21)], "its window on this screen");
        assert_eq!(shown_apps(&apps, &windows, &screens, 1), vec![(1, 11), (3, 31)]);
    }

    #[test]
    fn a_window_across_two_screens_is_on_the_one_holding_most_of_it() {
        let screens = two_screens();
        // 600 high: 400 of it above the built-in screen's top, 200 below.
        let mostly_above = Rect::from_min_size(pos2(100.0, -400.0), vec2(800.0, 600.0));
        assert_eq!(screen_of(mostly_above, &screens), Some(1));
        let mostly_below = Rect::from_min_size(pos2(100.0, -200.0), vec2(800.0, 600.0));
        assert_eq!(screen_of(mostly_below, &screens), Some(0));
        let nowhere = Rect::from_min_size(pos2(5000.0, 5000.0), vec2(800.0, 600.0));
        assert_eq!(screen_of(nowhere, &screens), None, "parked off every screen");
    }

    #[test]
    fn the_pointer_picks_its_screen_or_the_nearest() {
        let screens = two_screens();
        assert_eq!(screen_at(pos2(700.0, 500.0), &screens), Some(0));
        assert_eq!(screen_at(pos2(700.0, -500.0), &screens), Some(1));
        assert_eq!(screen_at(pos2(1600.0, 300.0), &screens), Some(0), "beside the built-in screen, off the bigger one");
        assert_eq!(screen_at(pos2(0.0, 0.0), &[]), None);
    }

    #[test]
    fn a_few_apps_sit_in_one_centred_row_at_full_size() {
        let l = layout(4, SCREEN);
        assert_eq!(l.scale, 1.0);
        assert!((l.panel.center() - pos2(735.0, 478.0)).length() < 0.5);
        assert!(l.previews.windows(2).all(|w| (w[0].top() - w[1].top()).abs() < 0.01 && w[0].right() < w[1].left()));
        assert!(l.previews.iter().all(|r| l.panel.contains_rect(*r)));
    }

    #[test]
    fn many_apps_shrink_to_fit_then_wrap_and_stay_on_screen() {
        for n in [6, 9, 14, 30] {
            let l = layout(n, SCREEN);
            assert!(l.panel.width() <= SCREEN.x * MOST_WIDTH + 0.5, "{n} apps fit across");
            assert!(l.scale >= SMALLEST_SCALE);
            assert!(l.previews.iter().all(|r| l.panel.contains_rect(*r)), "{n} apps inside the panel");
            assert_eq!(l.previews.len(), n);
        }
        assert!(layout(30, SCREEN).panel.height() > layout(6, SCREEN).panel.height(), "wraps into rows");
    }

    fn entries(n: usize) -> Vec<Entry> {
        (0..n).map(|i| Entry::new(i as i32 + 10, format!("App {i}"))).collect()
    }

    #[test]
    fn forward_picks_the_last_app_and_backward_the_oldest_and_both_wrap() {
        let s = Switcher::new(entries(4), true, Some(10), SCREEN);
        assert_eq!(s.selected, 1, "Cmd+Tab goes back to the app used before");
        let mut s = Switcher::new(entries(4), false, Some(10), SCREEN);
        assert_eq!(s.selected, 3, "Cmd+Shift+Tab starts at the oldest");
        s.step_by(1);
        assert_eq!(s.selected, 0, "wraps around");
        s.step_by(-1);
        assert_eq!(s.selected, 3);
        let s = Switcher::new(entries(3), true, Some(99), SCREEN);
        assert_eq!(s.selected, 0, "with something else in front, the newest app is first");
    }

    #[test]
    fn a_quick_tap_never_shows_the_panel_and_a_quit_app_leaves_its_place() {
        let mut s = Switcher::new(entries(3), true, Some(10), SCREEN);
        assert!(!s.showing(), "not straight away");
        s.step(1.0 / 120.0);
        assert!(s.show.value < 0.01);
        s.remove(1);
        assert_eq!(s.entries.len(), 2);
        assert_eq!(s.layout.previews.len(), 2);
        assert!(s.selected < 2);
        s.remove(0);
        s.remove(0);
        assert!(s.closing, "closes when nothing's left");
    }

    #[test]
    fn the_pointer_picks_only_once_it_moves() {
        let mut s = Switcher::new(entries(3), true, Some(10), SCREEN);
        let third = s.layout.previews[2].center();
        s.hover(third);
        assert_eq!(s.selected, 1, "resting there as it opens doesn't count");
        s.hover(third + vec2(3.0, 0.0));
        assert_eq!(s.selected, 2);
    }
}
