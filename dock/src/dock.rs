//! The dock itself: lays the tiles out, answers the pointer, and turns everything into black liquid blobs with
//! icons, dots and labels painted on top.

use std::path::PathBuf;
use std::time::Instant;

use egui::{Align2, Color32, Pos2, Rect, Shape, Vec2, pos2, vec2};
use notchbase_paint::color::{GRAY, PEARL, RED, white};
use notchbase_paint::font;
use notchbase_paint::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};

use crate::app_menu::{self, AppItem};
use crate::config::Config;
use crate::layout::{self, Placed, Slot};
use crate::liquid::Blob;
use crate::minimized::Minimized;
use crate::os;
use crate::tiles::{self, Kind, Tile};
use crate::usage::{self, Provider, Usage};
use crate::windows::{self, Win};

mod ambient;
mod stacks;
use stacks::{FileIcons, StackPopup, StackState};

pub const WINDOW_HEIGHT: f32 = 680.0;
// Spacing on a 4 point grid.
const BOTTOM_GAP: f32 = 10.0;
/// Above the icons and below them, in points, measured to the icon's canvas. App icons carry their own margin inside
/// the canvas (about 9% each side), so on screen this reads as about 8 above and 11 below, room for the dots.
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 7.0;
/// Past the first and last icon's canvas.
const EDGE: f32 = 3.0;
/// The art inside an app icon's canvas, as macOS draws it: what the corners and dots line up with.
const ART_INSET: f32 = 0.09;
const SEPARATOR: f32 = 16.0;
/// The separator's popup, which belongs to no tile.
const SEPARATOR_KEY: &str = "::separator";
/// How small and large dragging the separator makes the icons, and the standard size.
const SMALLEST: f32 = 16.0;
const LARGEST: f32 = 96.0;
const STANDARD: f32 = 48.0;
/// How far either side of the separator's line still grabs it.
const SEPARATOR_GRAB: f32 = 7.0;
const DRAG_START: f32 = 5.0;
const HOLD_FOR_MENU: f32 = 0.55;
const REMOVE_ABOVE: f32 = 64.0;
/// How far the hovered icon lifts, in points, from 32 point icons up. Smaller icons lift proportionally less, so a
/// 16 point icon doesn't jump a quarter of its own height.
const HOVER_LIFT: f32 = 4.0;
/// How far past its edge the pointer may go before the lifted icon hands over to its neighbour.
const HOVER_HOLD: f32 = 4.0;

// The notch's own springs, so the dock moves the way the island does. Where each comes from in the notchbase app:
/// Menus and tooltips growing out: the island opening (`src/notch/moves/transition.rs`, Island Open).
const OOZE: SpringConfig = SpringConfig::new(0.40, 0.76);
/// And shutting: the panel closing (`PANEL_OUT`).
const OOZE_SHUT: SpringConfig = SpringConfig::new(0.16, 1.0);
/// The longest step a popup takes in one frame, in seconds.
const LONGEST_POPUP_STEP: f32 = 0.025;
/// Fading out for Mission Control and back: about 200ms, critically damped, so it eases like Mission Control's own
/// animation and never overshoots into a half-faded state.
const AWAY: SpringConfig = SpringConfig::new(0.2, 1.0);
/// Sliding down out of sight over a full-screen app, and back up when the pointer pushes on the screen's edge: up a
/// touch quicker than down, both critically damped so it lands without a bounce.
const TUCK: SpringConfig = SpringConfig::new(0.34, 1.0);
const UNTUCK: SpringConfig = SpringConfig::new(0.28, 1.0);
/// How far the dock settles down as it fades, in points: a hint of motion, not a slide.
const AWAY_SETTLE: f32 = 3.0;
/// Covering in Mission Control: the dock stays, settles this far and its glass darkens this much with the desktop.
const COVER_SETTLE: f32 = 4.0;
/// How far past the system Dock's icons the cover reaches, so their shadows and edges are hidden too.
const COVER_REACH: f32 = 6.0;
/// The glass covering in Mission Control. The system can't blur there, so the glass goes near opaque to stand in for
/// the blur and hide the system Dock underneath.
/// Top and bottom: the system Dock sits in the lower half, so that end is fully dark.
const COVER_TINT: [f32; 2] = [0.93, 0.995];
/// Covering, the whole move (settling, reaching down over the system Dock and darkening) is one critically damped
/// spring with Mission Control's ease-out, in and back out, so it all starts and lands together.
const COVER: SpringConfig = SpringConfig::new(0.28, 1.0);
/// Things that track the pointer (`FOLLOW`).
const FOLLOW: SpringConfig = SpringConfig::new(0.14, 1.0);
/// A menu row's highlight (`HOVER`).
const PLATE: SpringConfig = SpringConfig::new(0.18, 1.0);
/// A dragged icon catching up with the pointer (`FOLLOW`).
const GRAB: SpringConfig = SpringConfig::new(0.14, 1.0);
/// The previews panel gliding from one app to the next: quick, with the faintest settle.
const PANEL_MOVE: SpringConfig = SpringConfig::new(0.26, 0.92);

// Window previews.
/// How long the pointer rests on a running app before its windows show.
const PREVIEW_DELAY: f32 = 0.35;
/// With previews already open, moving to the next app switches almost at once.
const PREVIEW_SWITCH: f32 = 0.06;
/// How long the panel waits after the pointer leaves both it and the icon.
const PREVIEW_LINGER: f32 = 0.25;
/// How often an open panel looks again at the app's windows.
const PREVIEW_RELIST: f32 = 1.0;
const THUMB_HEIGHT: f32 = 112.0;
const CARD_GAP: f32 = 12.0;
const PANEL_PAD: f32 = 12.0;
const TITLE_HEIGHT: f32 = 20.0;
const MOST_WINDOWS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Quit,
    Hide,
    KeepInDock,
    RemoveFromDock,
    ShowInFinder,
    EmptyTrash,
    /// One of the app's own actions from its menu bar, like New Window or Next.
    AppItem(usize),
    /// One of the app's windows, listed at the top of its menu.
    Window(usize),
    ShowAllWindows,
    /// A minimized window's tile: closing that window.
    CloseWindow,
    /// The separator's menu: magnification on or off.
    ToggleMagnification,
    /// The separator's menu: back to the standard size.
    ResetSize,
}

#[derive(Clone, Debug)]
struct Item {
    words: String,
    action: Option<Action>,
    danger: bool,
    /// A setting that's on or off, shown with a check mark.
    check: Option<bool>,
}

impl Item {
    fn new(words: impl Into<String>, action: Action) -> Self {
        Self { words: words.into(), action: Some(action), danger: false, check: None }
    }
    fn line() -> Self {
        Self { words: String::new(), action: None, danger: false, check: None }
    }
    fn height(&self) -> f32 {
        match (&self.action, self.words.is_empty()) {
            (Some(_), _) => 28.0,
            (None, true) => 9.0,
            (None, false) => 24.0,
        }
    }
}

enum PopupKind {
    Menu(Vec<Item>),
    /// A folder's items, as a fan or a grid.
    Stack(Box<StackPopup>),
    /// A running app's windows, side by side.
    Previews {
        pid: i32,
        wins: Vec<Win>,
    },
}

struct Popup {
    key: String,
    kind: PopupKind,
    open: Spring,
    shut: bool,
    hover: Option<usize>,
    plate: [Spring; 2],
    plate_shown: Spring,
    rect: Rect,
    /// Where the panel is drawn this frame, for its glass.
    drawn: Rect,
    /// Previews only: the panel's centre x and y, width, height, and the stem's x, all gliding between apps.
    frame: Option<[Spring; 5]>,
    /// Previews only: the contents fading in after switching apps.
    fade: Spring,
    /// Previews only: the window whose close button is under the pointer.
    closing: Option<usize>,
}

impl Popup {
    fn previews(&self) -> bool {
        matches!(self.kind, PopupKind::Previews { .. })
    }
}

#[derive(Default)]
struct Dwell {
    key: Option<String>,
    secs: f32,
    /// Already acted on: nothing more until the pointer moves to another icon.
    done: bool,
}

struct Tooltip {
    key: Option<String>,
    words: String,
    open: Spring,
    x: Spring,
    width: Spring,
}

struct Press {
    at: Pos2,
    when: Instant,
    tile: Option<String>,
    /// A file in an open stack, which dragging takes out as a real file.
    entry: Option<PathBuf>,
    right: bool,
    handled: bool,
}

/// Apps dragged over the dock from outside it.
struct Incoming {
    apps: Vec<PathBuf>,
    /// The tile the gap opens in front of; the end of the pinned apps when it's past them.
    before: Option<usize>,
    /// How open the gap is, 0 to 1, on a liquid spring.
    open: Spring,
    /// The cradle's centre, following the gap.
    x: Spring,
    icon: Option<egui::TextureHandle>,
}

/// An icon leaving the dock bursts like a bubble: it swells with a glass rim, then pops into glass droplets.
struct Burst {
    at: Pos2,
    size: f32,
    icon: Option<egui::TextureHandle>,
    t: f32,
    /// Each droplet's starting velocity (points a second) and radius.
    drops: Vec<(Vec2, f32)>,
}

const BURST_SWELL: f32 = 0.13;
const BURST_FLY: f32 = 0.3;
/// How far the bubble rises out of the dock as it swells, in icon radii.
const BURST_RISE: f32 = 1.25;
/// Points a second squared, pulling the droplets down as they fly.
const BURST_GRAVITY: f32 = 380.0;

/// The gap opens with a little give, like the notch taking something in.
const GAP: SpringConfig = SpringConfig::new(0.42, 0.74);

struct Drag {
    key: String,
    at: [Spring; 2],
    removing: Spring,
    pinned_before: bool,
}

/// What the dock asks the app to do outside itself.
pub enum Effect {
    Launched {
        path: PathBuf,
        pid: Option<i32>,
        icon: Rect,
    },
    /// A minimized window's tile was clicked: the app flies it back up from `icon` and restores it.
    Restore {
        pid: i32,
        id: u32,
        icon: Rect,
    },
    /// Files dragged out of a stack: the app starts a system drag of them.
    DragFiles(Vec<PathBuf>),
    /// Space in an open stack: Quick Look these, or close Quick Look if it's up.
    QuickLook(Vec<PathBuf>),
}

/// The modifier keys held at a click.
#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub command: bool,
    pub shift: bool,
}

pub struct Dock {
    pub config: Config,
    dir: PathBuf,
    pub tiles: Vec<Tile>,
    pub size: Vec2,
    pointer: Option<Pos2>,
    press: Option<Press>,
    /// The icon lifted under the pointer.
    hovered: Option<String>,
    /// 1 while it's faded out for Mission Control or the system Dock.
    away: Spring,
    /// 1 when slid down out of sight over a full-screen app.
    tucked: Spring,
    drag: Option<Drag>,
    popup: Option<Popup>,
    tooltip: Tooltip,
    pub files_over: bool,
    /// Files from an open stack are being dragged out: it stays up until they leave it.
    dragging_out: bool,
    drops: Vec<PathBuf>,
    /// Apps being dragged in from Finder or notchbase's Find: a gap opens where they'll land.
    incoming: Option<Incoming>,
    bursts: Vec<Burst>,
    /// The running apps as last told, so a new pin shows straight away.
    running: Vec<os::Running>,
    pub trash_full: bool,
    /// The badges apps show on their icons.
    pub badges: Vec<crate::badges::Badge>,
    /// How many icons may still be redrawn this frame.
    icon_budget: u32,
    confirm_empty: bool,
    pub effects: Vec<Effect>,
    /// The modifiers held at the latest press, set by the app.
    pub mods: Mods,
    /// Keys reach the open stack (the event tap is running), so its search can show.
    pub stack_keys: bool,
    pub time: f32,
    interactive: Rect,
    filled: bool,
    /// Talking to the real system: asking for Screen Recording, capturing and raising windows. Off in tests and
    /// snapshots.
    live: bool,
    /// Lists a running app's windows.
    pub list_windows: fn(i32) -> Vec<Win>,
    thumbs: Option<windows::Thumbs>,
    /// Minimized windows, oldest first, from the app's watch.
    minimized: Vec<Minimized>,
    /// Windows just restored from their tile, left out until the watch sees them back up.
    restored: Vec<(u32, Instant)>,
    /// Windows the dock minimized itself, shown before the watch catches up.
    expected: Vec<(Minimized, Instant)>,
    /// Tiles of windows still flying down into them: their slot is open but nothing's drawn yet.
    veiled: Vec<u32>,
    /// Pictures taken as windows went down, for their tiles.
    pictures: Vec<(u32, egui::ColorImage)>,
    /// Dragging the separator to resize: where the drag started, and the size then.
    resizing: Option<(f32, f32)>,
    /// The pointer is over the separator, which shows the resize cursor.
    pub over_separator: bool,
    dwell: Dwell,
    linger: f32,
    relist: f32,
    /// Option held: Quit becomes Force Quit.
    pub option_held: bool,
    /// The system appearance, which the menus' dividers follow.
    pub dark: bool,
    /// A blurred copy of the desktop picture is ready to stand in for the live blur in Mission Control.
    pub fake_ready: bool,
    /// The system Dock's icons in Mission Control, in window points, while it's showing: the cover reaches over them.
    pub real_dock: Option<Rect>,
    /// The windows listed in the open menu.
    menu_wins: Vec<Win>,
    /// The app's own actions listed in the open menu.
    menu_actions: Vec<AppItem>,
    /// Reads apps' own actions ahead of their menus opening.
    app_menus: Option<app_menu::Reader>,
    /// Claude's and ChatGPT's plan usage, for the widget at the foot of their menus.
    pub usage: Usage,
    /// The open menu's app has a usage widget.
    menu_usage: Option<Provider>,
    /// Folders shown as stacks, by path: what's in them.
    stacks: std::collections::HashMap<PathBuf, StackState>,
    /// Pictures of the files in stacks.
    files: FileIcons,
    /// Browsers' downloads, as bars on their tiles.
    bars: Vec<ambient::Bar>,
    /// The player playing and its beat, 0 to 1.
    beat: Option<(i32, f32)>,
}

impl Dock {
    pub fn new(config: Config, dir: PathBuf, size: Vec2) -> Self {
        Self {
            config,
            dir,
            tiles: Vec::new(),
            size,
            pointer: None,
            press: None,
            hovered: None,
            away: Spring::new(0.0, AWAY, UNIT_PRECISION),
            tucked: Spring::new(0.0, TUCK, UNIT_PRECISION),
            drag: None,
            popup: None,
            tooltip: Tooltip {
                key: None,
                words: String::new(),
                open: Spring::new(0.0, OOZE, UNIT_PRECISION),
                x: Spring::new(0.0, FOLLOW, POINT_PRECISION),
                width: Spring::new(60.0, FOLLOW, POINT_PRECISION),
            },
            files_over: false,
            dragging_out: false,
            drops: Vec::new(),
            incoming: None,
            bursts: Vec::new(),
            running: Vec::new(),
            trash_full: false,
            badges: Vec::new(),
            icon_budget: 3,
            confirm_empty: false,
            effects: Vec::new(),
            mods: Mods::default(),
            stack_keys: false,
            time: 0.0,
            interactive: Rect::NOTHING,
            filled: false,
            live: false,
            list_windows: windows::of_app,
            thumbs: None,
            minimized: Vec::new(),
            restored: Vec::new(),
            expected: Vec::new(),
            veiled: Vec::new(),
            pictures: Vec::new(),
            resizing: None,
            over_separator: false,
            dwell: Dwell::default(),
            linger: 0.0,
            relist: 0.0,
            option_held: false,
            dark: true,
            fake_ready: false,
            real_dock: None,
            menu_wins: Vec::new(),
            menu_actions: Vec::new(),
            app_menus: None,
            usage: Usage::default(),
            menu_usage: None,
            stacks: std::collections::HashMap::new(),
            files: FileIcons::default(),
            bars: Vec::new(),
            beat: None,
        }
    }

    /// For the real dock on screen, as opposed to tests and snapshots.
    pub fn live(mut self) -> Self {
        self.live = true;
        self
    }

    fn tile_size(&self) -> f32 {
        self.config.tile_size
    }

    fn peak(&self) -> f32 {
        self.config.magnified() / self.config.tile_size
    }

    /// An icon's scale now: grown while hovered, a touch smaller while pressed.
    fn scale_of(&self, t: &Tile) -> f32 {
        1.0 + (self.peak() - 1.0) * t.hover.value - 0.08 * t.down.value
    }

    /// How far the hovered icon lifts: [`HOVER_LIFT`], scaled down with icons smaller than 32 points.
    fn hover_lift(&self) -> f32 {
        HOVER_LIFT * (self.tile_size() / 32.0).clamp(0.5, 1.0)
    }

    fn slot_width(&self) -> f32 {
        self.tile_size() + 8.0
    }

    fn body_height(&self) -> f32 {
        self.tile_size() + PAD_TOP + PAD_BOTTOM
    }

    /// How far into the Mission Control cover move: 0 at rest, 1 covering. Everything in the move reads this.
    fn cover_progress(&self) -> f32 {
        if self.covering() { self.away.value.clamp(0.0, 1.0) } else { 0.0 }
    }

    /// For logging the cover move: its progress, the icons' baseline, the slab's bottom and the glass's top tint.
    pub fn cover_state(&self) -> (f32, f32, f32, f32) {
        let p = self.cover_progress();
        let slab = if p > 0.0 { self.cover_slab_bottom(24.0) } else { self.bottom() };
        (p, self.bottom(), slab, self.glass_band().tint[0])
    }

    /// The cover move's one curve: settling, reaching past the screen's edge, darkening and the blur going all ride
    /// it together. A strong ease-out, so the slab is down over the edge before the system Dock rising under it can
    /// peek out, and the rest lands softly.
    fn cover_ease(&self) -> f32 {
        1.0 - (1.0 - self.cover_progress()).powi(3)
    }

    fn cover_slab_bottom(&self, past: f32) -> f32 {
        lerp(self.bottom(), self.size.y + past, self.cover_ease())
    }

    pub fn cover_moving(&self) -> bool {
        self.covering() && !self.away.is_settled()
    }

    fn covering(&self) -> bool {
        self.config.mission_control == crate::config::MissionControl::Cover
    }

    fn bottom(&self) -> f32 {
        // Tucked away below the screen's edge, all of it.
        let tuck = self.tucked.value.max(0.0) * (self.body_height() + BOTTOM_GAP + 16.0);
        if self.covering() {
            return self.size.y - BOTTOM_GAP + self.cover_ease() * COVER_SETTLE + tuck;
        }
        self.size.y - BOTTOM_GAP + self.away.value.clamp(0.0, 1.0) * AWAY_SETTLE + tuck
    }

    /// Settings changed elsewhere (notchbase's Dock settings): taken as they are; sizes and the glass follow on
    /// their springs.
    pub fn take_config(&mut self, config: Config) {
        let resort = config.stack_sort != self.config.stack_sort;
        self.config = config;
        if resort {
            self.resort_stacks();
        }
    }

    /// Over a full-screen app the dock slides down out of sight, as the Dock does, and back up when asked.
    pub fn set_tucked(&mut self, tucked: bool) {
        if tucked && self.tucked.target == 0.0 {
            self.close_popup();
            self.press = None;
            self.drag = None;
        }
        if tucked {
            self.tucked.animate_to(1.0, TUCK);
        } else {
            self.tucked.animate_to(0.0, UNTUCK);
        }
    }

    /// Mission Control (or the system Dock shown): fading out in place and back, or, covering, staying put and only
    /// settling a touch and darkening with the dimmed desktop.
    pub fn set_away(&mut self, away: bool) {
        if away && self.away.target == 0.0 {
            self.close_popup();
            self.press = None;
            self.drag = None;
        }
        let spring = if self.covering() { COVER } else { AWAY };
        self.away.animate_to(if away { 1.0 } else { 0.0 }, spring);
    }

    /// How much of the bottom of the screen the dock takes, with a little air above it, in points.
    pub fn reserved_height(&self) -> f32 {
        BOTTOM_GAP + self.body_height() + 4.0
    }

    /// The body's top and bottom, which the glass gradient runs between.
    pub fn glass_band(&self) -> crate::liquid::Band {
        crate::liquid::Band {
            top: self.body_top(),
            bottom: self.bottom(),
            pointer: self.pointer,
            tint: {
                // With the desktop picture standing in for the blur, the glass keeps its own tint; without it, it
                // darkens with the dimmed desktop instead of showing the system's flat grey.
                let lead = if self.fake_ready { 0.0 } else { self.cover_ease() };
                [
                    lerp(self.config.glass_tint_top, COVER_TINT[0], lead),
                    lerp(self.config.glass_tint_bottom, COVER_TINT[1], lead),
                ]
            },
            panel: self.popup.as_ref().map(|p| p.drawn).filter(|r| r.is_positive()).map(|r| (r.top(), r.bottom())),
        }
    }

    /// How opaque the dock is as it fades for Mission Control.
    pub fn opacity(&self) -> f32 {
        if self.covering() {
            return 1.0;
        }
        1.0 - smoothstep(0.0, 0.85, self.away.value)
    }

    /// How much of the real blur shows. In Mission Control the system can't blur what's behind the dock and shows a
    /// flat light grey instead. Covering, the blurred desktop picture takes over, opaque, over the blur; without one
    /// the blur fades out with the move and the tint alone makes the glass.
    pub fn blur_opacity(&self) -> f32 {
        if self.fake_ready {
            return self.opacity() * self.untucked();
        }
        self.opacity() * (1.0 - self.cover_ease()) * self.untucked()
    }

    /// How much of the blur rides along as the dock slides in and out over a full-screen app. The blur's mask goes to
    /// the window server apart from the glass drawn here, so at speed it can land a frame off the glass and show as a
    /// band above or below it; it comes in as the dock slows to a stop and goes as it starts to move off.
    fn untucked(&self) -> f32 {
        1.0 - smoothstep(0.08, 0.6, self.tucked.value)
    }

    /// How much the blurred desktop picture stands in for the live blur: it rides the cover move in and out, so the
    /// glass looks just as it did and the system Dock under it never shows through.
    pub fn fake_glass(&self) -> f32 {
        if self.fake_ready { self.cover_ease() } else { 0.0 }
    }

    fn body_top(&self) -> f32 {
        self.bottom() - self.body_height()
    }

    fn icon_bottom(&self) -> f32 {
        self.bottom() - PAD_BOTTOM
    }

    /// Takes the running apps and the config into the tile list.
    pub fn refresh(&mut self, running: &[os::Running]) {
        running.clone_into(&mut self.running);
        let trash = os::trash_dir().unwrap_or_else(|| PathBuf::from("/tmp/.Trash"));
        self.restored.retain(|(_, at)| at.elapsed() < RESTORED_FOR);
        self.expected.retain(|(_, at)| at.elapsed() < RESTORED_FOR);
        let mut minimized: Vec<Minimized> = self.minimized.clone();
        for (m, _) in &self.expected {
            if !minimized.iter().any(|n| n.win.id == m.win.id) {
                minimized.push(m.clone());
            }
        }
        minimized.retain(|m| !self.restored.iter().any(|(id, _)| *id == m.win.id));
        let wanted = tiles::wanted(&self.config, running, &minimized, &trash);
        let arriving = self.filled;
        tiles::merge(&mut self.tiles, wanted, name_of, arriving);
        self.filled = true;
        for t in &mut self.tiles {
            if t.kind == Kind::Window
                && let Some(m) = minimized.iter().find(|m| tiles::window_key(m.win.id) == t.key)
            {
                if !m.win.title.is_empty() {
                    t.name = m.win.title.clone();
                }
                t.window = Some((m.pid, m.win.clone()));
                continue;
            }
            if let Some(r) = t.pid.and_then(|pid| running.iter().find(|r| r.pid == pid))
                && !r.name.is_empty()
            {
                t.name = r.name.clone();
            }
        }
    }

    /// The minimized windows the watch found. True when they changed, and the tiles need a refresh.
    pub fn set_minimized(&mut self, minimized: Vec<Minimized>) -> bool {
        if minimized == self.minimized {
            return false;
        }
        // A restored window the watch already sees back up needn't be held out any longer, nor one it sees down
        // be held in.
        self.restored.retain(|(id, _)| minimized.iter().any(|m| m.win.id == *id));
        self.expected.retain(|(e, _)| !minimized.iter().any(|m| m.win.id == e.win.id));
        self.minimized = minimized;
        true
    }

    /// A window the dock is minimizing itself: its tile opens now, veiled until the window lands in it.
    pub fn expect_minimized(&mut self, m: Minimized, picture: Option<egui::ColorImage>) {
        let id = m.win.id;
        self.restored.retain(|(r, _)| *r != id);
        self.expected.retain(|(e, _)| e.win.id != id);
        self.expected.push((m, Instant::now()));
        if !self.veiled.contains(&id) {
            self.veiled.push(id);
        }
        if let Some(picture) = picture {
            self.pictures.retain(|(p, _)| *p != id);
            self.pictures.push((id, picture));
        }
        let running = self.running.clone();
        self.refresh(&running);
    }

    /// A minimize that didn't happen after all (the app never minimized the window): its tile goes.
    pub fn forget_expected(&mut self, id: u32) {
        self.expected.retain(|(e, _)| e.win.id != id);
        self.veiled.retain(|v| *v != id);
        self.pictures.retain(|(p, _)| *p != id);
        let running = self.running.clone();
        self.refresh(&running);
    }

    /// Whether window `id` is down in the dock (or about to be).
    pub fn holds_window(&self, id: u32) -> bool {
        self.window_tile(id).is_some() || self.expected.iter().any(|(e, _)| e.win.id == id)
    }

    /// A running app's name, for the log of flights.
    pub fn app_name(&self, pid: i32) -> String {
        self.running.iter().find(|r| r.pid == pid).map_or_else(|| format!("pid {pid}"), |r| r.name.clone())
    }

    /// Clicks the newest minimized window's tile, for trying restoring by hand.
    /// Where the newest minimized window's tile is (or, with `by_app`, its app's icon), in dock points.
    pub fn newest_window_target(&self, by_app: bool) -> Option<Rect> {
        let t = self.tiles.iter().rev().find(|t| t.kind == Kind::Window && !t.leaving)?;
        let key = if by_app {
            let pid = t.window.as_ref().map(|w| w.0);
            self.tiles.iter().find(|a| a.kind == Kind::App && a.pid == pid && pid.is_some())?.key.clone()
        } else {
            t.key.clone()
        };
        self.tile_index(&key).map(|i| self.icon_rect(i))
    }

    /// With `by_app`, clicks that window's app's icon instead.
    pub fn restore_newest(&mut self, by_app: bool) {
        let Some(t) = self.tiles.iter().rev().find(|t| t.kind == Kind::Window && !t.leaving) else { return };
        let key = if by_app {
            let pid = t.window.as_ref().map(|w| w.0);
            match self.tiles.iter().find(|a| a.kind == Kind::App && a.pid == pid && pid.is_some()) {
                Some(a) => a.key.clone(),
                None => return,
            }
        } else {
            t.key.clone()
        };
        self.activate(&key);
    }

    /// The picture a minimized window's tile shows, if it has one.
    pub fn window_picture(&self, id: u32) -> Option<egui::ColorImage> {
        self.pictures.iter().find(|p| p.0 == id).map(|p| p.1.clone())
    }

    /// Where a minimized window was before it went down, as its app last said.
    pub fn minimized_frame(&self, id: u32) -> Option<Rect> {
        self.minimized.iter().find(|m| m.win.id == id).map(|m| m.win.frame)
    }

    /// Shows a window's tile once the window has landed in it.
    pub fn unveil(&mut self, id: u32) {
        let was = self.veiled.len();
        self.veiled.retain(|v| *v != id);
        // The tile takes the drop in with a little swell, the way a launching app's icon does.
        if self.veiled.len() < was
            && let Some(i) = self.tile_index(&tiles::window_key(id))
        {
            let t = &mut self.tiles[i];
            t.gulp.velocity = tiles::GULP_KICK * 0.5;
            t.gulp.animate_to(0.0, tiles::SWELL);
        }
    }

    /// Where a window's tile's icon is now, in dock points.
    pub fn window_tile(&self, id: u32) -> Option<Rect> {
        let key = tiles::window_key(id);
        self.tile_index(&key).map(|i| self.icon_rect(i))
    }

    pub fn set_pointer(&mut self, at: Option<Pos2>) {
        self.pointer = at;
    }

    /// The area that takes the mouse; everything else in the window clicks through.
    pub fn interactive(&self) -> bool {
        if self.away.target > 0.0 || self.tucked.target > 0.0 {
            return false;
        }
        self.press.is_some()
            || self.resizing.is_some()
            || self.drag.is_some()
            || self.files_over
            || self.popup.as_ref().is_some_and(|p| !p.shut)
            || self.pointer.is_some_and(|p| self.interactive.contains(p))
    }

    pub fn popup_open(&self) -> bool {
        self.popup.as_ref().is_some_and(|p| !p.shut)
    }

    /// A menu or stack open, which takes over the pointer. Previews don't: the dock still answers under them.
    fn menu_open(&self) -> bool {
        self.popup.as_ref().is_some_and(|p| !p.shut && !p.previews())
    }

    fn previews_key(&self) -> Option<&str> {
        self.popup.as_ref().filter(|p| !p.shut && p.previews()).map(|p| p.key.as_str())
    }

    pub fn close_popup(&mut self) {
        if let Some(p) = &mut self.popup {
            p.shut = true;
            p.open.animate_to(0.0, OOZE_SHUT);
        }
        self.confirm_empty = false;
    }

    pub fn animating(&self) -> bool {
        !self.away.is_settled()
            || !self.tucked.is_settled()
            || self.tiles.iter().any(|t| !t.settled())
            || self.popup.is_some()
            || !self.tooltip.open.is_settled()
            || !self.tooltip.x.is_settled()
            || !self.tooltip.width.is_settled()
            || self.drag.is_some()
            || self.press.as_ref().is_some_and(|p| !p.handled)
            || !self.drops.is_empty()
            || self.incoming.is_some()
            || !self.bursts.is_empty()
            || self.resizing.is_some()
            || self.stacks_animating()
    }

    /// Whether `p` is on the separator, between the apps and the folders.
    fn separator_at(&self, p: Pos2, placed: &[Placed], refs: &[Option<usize>]) -> bool {
        if self.menu_open() || self.drag.is_some() || p.y < self.body_top() || p.y > self.size.y {
            return false;
        }
        placed.iter().zip(refs).any(|(s, r)| r.is_none() && !s.gap && (p.x - s.center()).abs() <= SEPARATOR_GRAB)
    }

    /// For offscreen pictures: a menu over the first app with dividers, as a running app's has.
    pub fn show_sample_menu(&mut self) {
        let Some(key) = self.tiles.first().map(|t| t.key.clone()) else { return };
        let items = vec![
            Item::new("New Window", Action::ResetSize),
            Item::line(),
            Item::new("Options", Action::ResetSize),
            Item::new("Show All Windows", Action::ResetSize),
            Item::line(),
            Item::new("Hide", Action::ResetSize),
            Item::new("Quit", Action::Quit),
        ];
        self.menu_usage = None;
        self.show_popup(key, PopupKind::Menu(items));
    }

    fn open_separator_menu(&mut self) {
        let mut magnify = Item::new("Magnification", Action::ToggleMagnification);
        magnify.check = Some(self.config.magnification);
        let mut items = vec![magnify];
        if self.config.tile_size != STANDARD {
            items.push(Item::line());
            items.push(Item::new("Standard Size", Action::ResetSize));
        }
        self.menu_usage = None;
        self.show_popup(SEPARATOR_KEY.to_owned(), PopupKind::Menu(items));
    }

    fn slots(&self) -> (Vec<Slot>, Vec<Option<usize>>) {
        let mut slots = Vec::with_capacity(self.tiles.len() + 1);
        let mut refs = Vec::with_capacity(self.tiles.len() + 1);
        let has_apps = self.tiles.iter().any(|t| t.section() == 0 && !t.leaving);
        let mut separated = false;
        let gap = self.incoming.as_ref().filter(|g| g.open.value > 0.001).map(|g| {
            let slot = Slot {
                width: self.slot_width() * g.open.value.max(0.0),
                icon: 0.0,
                scale: 1.0,
                gap: true,
                presence: 1.0,
            };
            (g.before.unwrap_or(self.tiles.len()), slot)
        });
        for (i, t) in self.tiles.iter().enumerate() {
            if let Some((at, slot)) = gap
                && at == i
            {
                slots.push(slot);
                refs.push(None);
            }
            if t.section() == 1 && !separated {
                separated = true;
                if has_apps {
                    slots.push(Slot { width: SEPARATOR, icon: 0.0, scale: 1.0, gap: false, presence: 1.0 });
                    refs.push(None);
                }
            }
            let presence = t.presence.value.max(0.0);
            let scale = self.scale_of(t);
            slots.push(Slot {
                width: self.slot_width() * presence,
                icon: self.tile_size() * scale * presence,
                scale,
                gap: false,
                presence,
            });
            refs.push(Some(i));
        }
        if let Some((at, slot)) = gap
            && at >= self.tiles.len()
        {
            slots.push(slot);
            refs.push(None);
        }
        (slots, refs)
    }

    fn place(&self) -> (Vec<Placed>, Vec<Option<usize>>) {
        let (slots, refs) = self.slots();
        let placed = layout::place(&slots, self.size.x * 0.5, self.slot_width() - self.tile_size());
        (placed, refs)
    }

    fn tile_index(&self, key: &str) -> Option<usize> {
        self.tiles.iter().position(|t| t.key == key)
    }

    /// The tile under a point, if it's over the dock.
    fn tile_at(&self, p: Pos2, placed: &[Placed], refs: &[Option<usize>]) -> Option<usize> {
        let tile = self.tile_size();
        placed.iter().zip(refs).find_map(|(slot, r)| {
            let i = (*r)?;
            let t = &self.tiles[i];
            let top = self.icon_bottom() - tile * slot.scale - PAD_TOP;
            let inside = p.x >= slot.left && p.x < slot.right() && p.y >= top && p.y <= self.size.y;
            (inside && !t.leaving).then_some(i)
        })
    }

    pub fn press(&mut self, right: bool) {
        let Some(at) = self.pointer else { return };
        if self.stack_press(at, right) {
            return;
        }
        if let Some(popup) = &self.popup
            && !popup.shut
        {
            if popup.rect.expand(4.0).contains(at) {
                self.press = Some(Press { at, when: Instant::now(), tile: None, entry: None, right, handled: false });
                return;
            }
            let previews = popup.previews();
            self.close_popup();
            if !previews {
                self.press = Some(Press { at, when: Instant::now(), tile: None, entry: None, right, handled: true });
                return;
            }
            // Previews just go; the click still lands on the dock. They don't come back until the pointer moves on.
            self.dwell.done = true;
        }
        let (placed, refs) = self.place();
        if self.separator_at(at, &placed, &refs) {
            if right {
                self.open_separator_menu();
            } else {
                self.resizing = Some((at.y, self.tile_size()));
            }
            self.press = Some(Press { at, when: Instant::now(), tile: None, entry: None, right, handled: true });
            return;
        }
        let tile = self.tile_at(at, &placed, &refs).map(|i| self.tiles[i].key.clone());
        if right && let Some(key) = &tile {
            self.open_menu(key.clone());
            self.press = Some(Press { at, when: Instant::now(), tile, entry: None, right, handled: true });
            return;
        }
        self.press = Some(Press { at, when: Instant::now(), tile, entry: None, right, handled: false });
    }

    pub fn release(&mut self) {
        if self.resizing.take().is_some() {
            self.press = None;
            self.config = std::mem::take(&mut self.config).sane();
            self.save();
            // Icons were drawn for the old size: they're redrawn sharp for the new one.
            for t in &mut self.tiles {
                t.stale = true;
            }
            return;
        }
        let press = self.press.take();
        if let Some(drag) = self.drag.take() {
            self.finish_drag(drag);
            return;
        }
        let Some(press) = press else { return };
        if press.handled || press.right {
            return;
        }
        let Some(at) = self.pointer else { return };
        if self.stack_release() {
            return;
        }
        if let Some(popup) = &self.popup
            && !popup.shut
        {
            if let Some(i) = popup.hover {
                self.choose(i);
            }
            return;
        }
        if let Some(key) = press.tile {
            let (placed, refs) = self.place();
            if self.tile_at(at, &placed, &refs).is_some_and(|i| self.tiles[i].key == key) {
                self.activate(&key);
            }
        }
    }

    pub fn hover_files(&mut self, over: bool) {
        self.files_over = over;
    }

    /// A file dragged over the dock. Apps get a gap to land in.
    pub fn hover_file(&mut self, path: PathBuf) {
        self.files_over = true;
        if !os::is_app(&path) {
            return;
        }
        let incoming = self.incoming.get_or_insert_with(|| Incoming {
            apps: Vec::new(),
            before: None,
            open: Spring::new(0.0, GAP, UNIT_PRECISION),
            x: Spring::new(0.0, FOLLOW, POINT_PRECISION),
            icon: None,
        });
        if !incoming.apps.contains(&path) {
            incoming.apps.push(path);
        }
    }

    pub fn drop_file(&mut self, path: PathBuf) {
        self.drops.push(path);
        self.files_over = false;
    }

    fn icon_top(&self, scale: f32) -> f32 {
        self.icon_bottom() - self.tile_size() * scale
    }

    /// Where the icon is drawn right now, in dock points.
    pub(crate) fn icon_rect(&self, i: usize) -> Rect {
        let (placed, refs) = self.place();
        let slot = refs.iter().position(|r| *r == Some(i)).map(|s| placed[s]);
        let x = slot.map_or(self.size.x * 0.5, |s| s.center());
        let size = self.tile_size() * slot.map_or(1.0, |s| s.scale);
        Rect::from_min_max(pos2(x - size * 0.5, self.icon_bottom() - size), pos2(x + size * 0.5, self.icon_bottom()))
    }

    fn activate(&mut self, key: &str) {
        let Some(i) = self.tile_index(key) else { return };
        let (kind, path, pid) = (self.tiles[i].kind, self.tiles[i].path.clone(), self.tiles[i].pid);
        match kind {
            Kind::App => {
                // An app whose only windows are minimized: its newest one comes back through its own tile, the same
                // smooth way, rather than the app popping it up wherever it was put away.
                if let Some(pid) = pid
                    && !crate::windows::every_window().iter().any(|(p, _, on)| *p == pid && *on)
                    && let Some(key) = self
                        .tiles
                        .iter()
                        .rev()
                        .find(|t| t.kind == Kind::Window && !t.leaving && t.window.as_ref().is_some_and(|w| w.0 == pid))
                        .map(|t| t.key.clone())
                {
                    self.activate(&key);
                    return;
                }
                // A running app comes forward this instant; `open` (a separate process, a beat slower) then asks it
                // for a window if it has none showing, as the Dock does.
                if let Some(pid) = pid {
                    os::activate(pid);
                }
                os::open(&path);
                let icon = self.icon_rect(i);
                let swells = self.config.launch_effect_for(&path) != crate::config::LaunchEffect::Off;
                let t = &mut self.tiles[i];
                if swells {
                    t.gulp.velocity = tiles::GULP_KICK;
                    t.gulp.animate_to(0.0, tiles::SWELL);
                }
                if pid.is_none() {
                    t.launching = Some(Instant::now());
                }
                self.effects.push(Effect::Launched { path, pid, icon });
            }
            Kind::Window => {
                if let Some((pid, win)) = self.tiles[i].window.clone() {
                    let icon = self.icon_rect(i);
                    self.effects.push(Effect::Restore { pid, id: win.id, icon });
                    // Its tile goes at once, as the window comes back up.
                    self.restored.push((win.id, Instant::now()));
                    let running = self.running.clone();
                    self.refresh(&running);
                }
            }
            Kind::Folder => self.open_stack(key.to_owned()),
            Kind::Trash => os::open(&path),
        }
    }

    fn open_menu(&mut self, key: String) {
        if let Some(pid) = self.tile_index(&key).and_then(|i| self.tiles[i].pid) {
            self.read_app_menu(pid);
        }
        let Some(items) = self.menu_items(&key) else { return };
        self.show_popup(key, PopupKind::Menu(items));
        if let Some(provider) = self.menu_usage {
            self.usage.opened(provider);
        }
    }

    /// Starts reading an app's own actions (on the real system only), so its menu has them.
    fn read_app_menu(&mut self, pid: i32) {
        if self.live {
            self.app_menus.get_or_insert_with(app_menu::Reader::new).want(pid);
        }
    }

    /// The open menu's usage widget: fresh numbers while it's up, and its bars and height gliding to them.
    fn step_usage(&mut self, dt: f32) {
        let menu = self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Menu(_)));
        let Some(provider) = self.menu_usage.filter(|_| menu) else { return };
        if self.live {
            self.usage.poll(provider);
        }
        self.usage.step(provider, dt);
    }

    /// When an app's actions arrive while its menu is open, they slot in at the top.
    fn take_app_menus(&mut self) {
        let Some(reader) = &mut self.app_menus else { return };
        let arrived = reader.collect();
        let Some(popup) = self.popup.as_ref().filter(|p| !p.shut && matches!(p.kind, PopupKind::Menu(_))) else {
            return;
        };
        let key = popup.key.clone();
        let pid = self.tile_index(&key).and_then(|i| self.tiles[i].pid);
        if pid.is_none_or(|pid| !arrived.contains(&pid)) || self.confirm_empty {
            return;
        }
        let actions = match (pid, &self.app_menus) {
            (Some(pid), Some(reader)) => reader.get(pid).to_vec(),
            _ => Vec::new(),
        };
        if actions == self.menu_actions {
            return;
        }
        if let Some(items) = self.menu_items(&key)
            && let Some(popup) = &mut self.popup
        {
            popup.kind = PopupKind::Menu(items);
            popup.hover = None;
        }
    }

    fn menu_items(&mut self, key: &str) -> Option<Vec<Item>> {
        let i = self.tile_index(key)?;
        let t = &self.tiles[i];
        let mut items = Vec::new();
        let provider = if t.kind == Kind::App { Provider::of(&t.path) } else { None };
        match t.kind {
            Kind::App => {
                let (finder, running, pinned, pid) = (is_finder(t), t.running(), t.pinned, t.pid);
                let name = t.name.clone();
                // The app's own actions first (New Window, Play…), then its windows, like the Dock.
                let actions: Vec<AppItem> = match (pid, &self.app_menus) {
                    (Some(pid), Some(reader)) => reader.get(pid).to_vec(),
                    _ => Vec::new(),
                };
                for (n, action) in actions.iter().enumerate() {
                    items.push(Item::new(action.title.clone(), Action::AppItem(n)));
                }
                if !actions.is_empty() {
                    items.push(Item::line());
                }
                self.menu_actions = actions;
                // Its windows first, like the Dock.
                let wins: Vec<Win> = pid
                    .map(|pid| (self.list_windows)(pid))
                    .unwrap_or_default()
                    .into_iter()
                    .take(MOST_WINDOWS)
                    .collect();
                for (n, w) in wins.iter().enumerate() {
                    let title = if w.title.is_empty() { name.as_str() } else { w.title.as_str() };
                    let mut words: String = title.chars().take(44).collect();
                    if title.chars().count() > 44 {
                        words.push('\u{2026}');
                    }
                    items.push(Item::new(words, Action::Window(n)));
                }
                if !wins.is_empty() {
                    items.push(Item::line());
                }
                if !running {
                    items.push(Item::new("Open", Action::Open));
                }
                if !finder {
                    let keep = if pinned {
                        Item::new("Remove from Dock", Action::RemoveFromDock)
                    } else {
                        Item::new("Keep in Dock", Action::KeepInDock)
                    };
                    items.push(keep);
                }
                if running {
                    if !wins.is_empty() {
                        items.push(Item::new("Show All Windows", Action::ShowAllWindows));
                    }
                    if !finder {
                        items.push(Item::new("Hide", Action::Hide));
                        items.push(Item::new("Quit", Action::Quit));
                    }
                }
                self.menu_wins = wins;
            }
            Kind::Window => {
                items.push(Item::new("Restore", Action::Open));
                items.push(Item::new("Close", Action::CloseWindow));
            }
            Kind::Folder => {
                items.push(Item::new(format!("Open \u{201c}{}\u{201d}", t.name), Action::Open));
                items.push(Item::new(os::SHOW_IN_FILES, Action::ShowInFinder));
                items.push(Item::line());
                items.push(Item::new("Remove from Dock", Action::RemoveFromDock));
            }
            Kind::Trash => {
                items.push(Item::new("Open", Action::Open));
                if self.trash_full {
                    items.push(Item::line());
                    items.push(Item::new(os::EMPTY_TRASH, Action::EmptyTrash));
                }
            }
        }
        self.menu_usage = provider;
        Some(items)
    }

    fn show_popup(&mut self, key: String, kind: PopupKind) {
        let mut open = Spring::new(0.0, OOZE, UNIT_PRECISION);
        open.animate_to(1.0, OOZE);
        self.popup = Some(Popup {
            key,
            kind,
            open,
            shut: false,
            hover: None,
            plate: [Spring::new(0.0, PLATE, POINT_PRECISION), Spring::new(0.0, PLATE, POINT_PRECISION)],
            plate_shown: Spring::new(0.0, PLATE, UNIT_PRECISION),
            rect: Rect::NOTHING,
            drawn: Rect::NOTHING,
            frame: None,
            fade: Spring::new(1.0, PLATE, UNIT_PRECISION),
            closing: None,
        });
        self.tooltip.open.animate_to(0.0, OOZE_SHUT);
    }

    fn choose(&mut self, index: usize) {
        let Some(popup) = &self.popup else { return };
        let key = popup.key.clone();
        let action = match &popup.kind {
            PopupKind::Menu(items) => items.get(index).and_then(|i| i.action),
            PopupKind::Stack(_) => {
                self.choose_in_stack(index);
                return;
            }
            PopupKind::Previews { pid, wins } => {
                let (pid, closing) = (*pid, popup.closing);
                let Some(win) = wins.get(index).cloned() else { return };
                self.pick_window(pid, win, closing == Some(index));
                return;
            }
        };
        let Some(action) = action else { return };
        if action == Action::EmptyTrash && !self.confirm_empty {
            // Emptying can't be undone: ask once more, in place.
            self.confirm_empty = true;
            if let Some(Popup { kind: PopupKind::Menu(items), .. }) = &mut self.popup
                && let Some(item) = items.get_mut(index)
            {
                item.words = os::EMPTY_TRASH_CONFIRM.into();
                item.danger = true;
            }
            return;
        }
        self.run(&key, action);
        self.close_popup();
    }

    /// Raises a window from the previews, or closes it with its close button.
    fn pick_window(&mut self, pid: i32, win: Win, close: bool) {
        if !close {
            if self.live {
                let win = win.clone();
                std::thread::spawn(move || windows::raise(pid, &win));
            }
            self.close_popup();
            self.dwell.done = true;
            return;
        }
        if self.live {
            let win = win.clone();
            std::thread::spawn(move || windows::close(pid, &win));
        }
        // It leaves the panel now; the next look at the app's windows confirms it (an app may ask to save first).
        let mut empty = false;
        if let Some(Popup { kind: PopupKind::Previews { wins, .. }, closing, hover, .. }) = &mut self.popup {
            wins.retain(|w| w.id != win.id);
            empty = wins.is_empty();
            *closing = None;
            *hover = None;
        }
        self.relist = PREVIEW_RELIST - 0.5;
        if empty {
            self.close_popup();
        }
    }

    /// Opens the windows of the app under `key`, or glides an open panel over to it.
    fn open_previews(&mut self, key: String) {
        let Some(pid) = self.tile_index(&key).and_then(|i| self.tiles[i].pid) else { return };
        let wins = (self.list_windows)(pid);
        if wins.is_empty() {
            if self.previews_key().is_some() {
                self.close_popup();
            }
            return;
        }
        if self.live {
            // macOS asks once to allow Screen Recording; until then the panel shows titles and the app's icon.
            windows::can_capture(true);
        }
        match &mut self.popup {
            Some(popup) if !popup.shut && popup.previews() => {
                popup.key = key;
                popup.kind = PopupKind::Previews { pid, wins };
                popup.fade.snap_to(0.0);
                popup.fade.animate_to(1.0, PLATE);
                popup.hover = None;
                popup.closing = None;
                popup.plate_shown.snap_to(0.0);
            }
            _ => self.show_popup(key, PopupKind::Previews { pid, wins }),
        }
        self.linger = 0.0;
        self.relist = 0.0;
    }

    /// Rest on a running app to see its windows; move along the dock and the panel follows; leave and it goes.
    fn update_previews(&mut self, dt: f32, under: Option<usize>) {
        let busy = self.press.is_some() || self.drag.is_some() || self.menu_open() || self.away.target > 0.0;
        let candidate = under.filter(|&i| {
            let t = &self.tiles[i];
            !busy && t.kind == Kind::App && t.pid.is_some() && !t.leaving && self.config.previews_for(&t.path)
        });
        let key = candidate.map(|i| self.tiles[i].key.clone());
        if key == self.dwell.key {
            self.dwell.secs += dt;
        } else {
            self.dwell = Dwell { key: key.clone(), secs: 0.0, done: false };
        }
        let open = self.previews_key().map(str::to_owned);
        if let Some(key) = key
            && !self.dwell.done
        {
            let wait = if open.is_some() { PREVIEW_SWITCH } else { PREVIEW_DELAY };
            if open.as_deref() == Some(key.as_str()) {
                self.dwell.done = true;
            } else if self.dwell.secs >= wait {
                self.dwell.done = true;
                self.open_previews(key);
            }
        }

        let Some(popup) = self.popup.as_ref().filter(|p| !p.shut && p.previews()) else { return };
        let over_icon = under.is_some_and(|i| self.tiles[i].key == popup.key);
        let in_panel = self.pointer.is_some_and(|p| popup.rect.expand(10.0).contains(p));
        if over_icon || in_panel {
            self.linger = 0.0;
        } else {
            self.linger += dt;
            if self.linger > PREVIEW_LINGER {
                self.close_popup();
                return;
            }
        }
        self.relist += dt;
        if self.relist > PREVIEW_RELIST {
            self.relist = 0.0;
            let Some(Popup { kind: PopupKind::Previews { pid, .. }, .. }) = &self.popup else { return };
            let wins = (self.list_windows)(*pid);
            if wins.is_empty() {
                self.close_popup();
            } else if let Some(Popup { kind: PopupKind::Previews { wins: shown, .. }, .. }) = &mut self.popup
                && *shown != wins
            {
                // Pictures of windows that closed are dropped.
                let gone: Vec<u32> =
                    shown.iter().map(|w| w.id).filter(|id| !wins.iter().any(|w| w.id == *id)).collect();
                if let Some(thumbs) = &mut self.thumbs {
                    thumbs.keep(|id| !gone.contains(&id));
                }
                *shown = wins;
            }
        }
    }

    fn run(&mut self, key: &str, action: Action) {
        match action {
            Action::ToggleMagnification => {
                self.config.magnification = !self.config.magnification;
                if self.config.magnification && self.config.large_size <= self.config.tile_size {
                    self.config.large_size = (self.config.tile_size * 1.25).min(160.0);
                }
                self.config = std::mem::take(&mut self.config).sane();
                self.save();
                return;
            }
            Action::ResetSize => {
                self.config.tile_size = STANDARD;
                self.config = std::mem::take(&mut self.config).sane();
                self.save();
                for t in &mut self.tiles {
                    t.stale = true;
                }
                return;
            }
            _ => {}
        }
        let Some(i) = self.tile_index(key) else { return };
        let path = self.tiles[i].path.clone();
        match action {
            Action::Open => self.activate(key),
            Action::Quit => {
                if let Some(pid) = self.tiles[i].pid {
                    if self.option_held { os::force_quit(pid) } else { os::quit(pid) }
                }
            }
            Action::AppItem(n) => {
                if let (Some(pid), Some(action)) = (self.tiles[i].pid, self.menu_actions.get(n)) {
                    app_menu::press(pid, action.clone());
                }
            }
            Action::Window(n) => {
                if let (Some(pid), Some(win)) = (self.tiles[i].pid, self.menu_wins.get(n).cloned()) {
                    std::thread::spawn(move || windows::raise(pid, &win));
                }
            }
            Action::ShowAllWindows => {
                if let Some(pid) = self.tiles[i].pid {
                    os::app_expose(pid);
                }
            }
            Action::Hide => {
                if let Some(pid) = self.tiles[i].pid {
                    os::hide(pid);
                }
            }
            Action::KeepInDock => {
                let at = self.tiles[..i].iter().filter(|t| t.pinned && t.kind == Kind::App).count();
                self.config.pin(path, at);
                self.save();
                // It joins the pinned apps now, not on the next look at what's running.
                let running = self.running.clone();
                self.refresh(&running);
            }
            Action::RemoveFromDock => {
                match self.tiles[i].kind {
                    Kind::Folder => {
                        // Downloads comes back from Settings › Dock (Show Downloads), so removing it turns that off.
                        if crate::config::downloads_folder().as_ref() == Some(&path) {
                            self.config.show_downloads = false;
                        }
                        self.config.others.retain(|p| p != &path);
                    }
                    _ => self.config.unpin(&path),
                }
                self.save();
                // A running app stays, only unpinned, and moves over to the running apps at once; anything that leaves
                // bursts.
                if self.tiles[i].pid.is_none() {
                    let at = self.icon_rect(i);
                    self.burst(i, at.center(), at.width());
                } else {
                    let running = self.running.clone();
                    self.refresh(&running);
                }
            }
            // Handled above, without a tile.
            Action::ToggleMagnification | Action::ResetSize => {}
            Action::CloseWindow => {
                if let Some((pid, win)) = self.tiles[i].window.clone() {
                    if self.live {
                        let win = win.clone();
                        std::thread::spawn(move || windows::close(pid, &win));
                    }
                    self.restored.push((win.id, Instant::now()));
                    let running = self.running.clone();
                    self.refresh(&running);
                }
            }
            Action::ShowInFinder => os::reveal(&path),
            Action::EmptyTrash => {
                // The Trash shows empty at once; Finder empties it in the background.
                os::empty_trash();
                self.trash_full = false;
                self.confirm_empty = false;
            }
        }
    }

    pub fn save(&mut self) {
        // Written in the background, so a slow disk never holds up the dock's answer. A save that comes in late
        // never overwrites a newer one.
        static ASKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
        let (config, dir) = (self.config.clone(), self.dir.clone());
        let n = ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let write = move || {
            let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
            if n > *written {
                config.save(&dir);
                *written = n;
            }
        };
        if cfg!(test) {
            write();
        } else {
            std::thread::spawn(write);
        }
        // Pinning changes what's wanted: the app refreshes with the running list on its next tick.
        for t in &mut self.tiles {
            if t.kind == Kind::App {
                t.pinned = self.config.is_pinned(&t.path);
            }
        }
    }

    fn finish_drag(&mut self, drag: Drag) {
        let Some(i) = self.tile_index(&drag.key) else { return };
        if drag.removing.target > 0.5 {
            let path = self.tiles[i].path.clone();
            self.config.unpin(&path);
            self.save();
            // A running app stays, unpinned; a pinned one that isn't running melts away from where it was dropped.
            if self.tiles[i].pid.is_none() {
                let at = pos2(drag.at[0].value, drag.at[1].value);
                self.burst(i, at, self.tile_size() * 0.95);
            }
            return;
        }
        // Settle the order into the config: every app left of it that's pinned, plus this one.
        let pinned_order: Vec<PathBuf> = self
            .tiles
            .iter()
            .filter(|t| t.kind == Kind::App && !is_finder(t) && (t.pinned || t.key == drag.key))
            .map(|t| t.path.clone())
            .collect();
        let was = self.config.pinned.clone();
        self.config.pinned = pinned_order;
        if was != self.config.pinned || !drag.pinned_before {
            self.save();
        }
        // The icon springs from where it was let go back into its slot.
        let (placed, refs) = self.place();
        if let Some(s) = refs.iter().position(|r| *r == Some(i)) {
            let home = placed[s].center();
            let t = &mut self.tiles[i];
            t.slide.snap_to(drag.at[0].value - home);
            t.slide.velocity = drag.at[0].velocity;
            t.slide.animate_to(0.0, tiles::SLIDE);
        }
    }

    fn update_drag(&mut self, at: Pos2) {
        let removing = at.y < self.body_top() - REMOVE_ABOVE;
        let Some(drag) = &mut self.drag else { return };
        drag.at[0].animate_to(at.x, GRAB);
        drag.at[1].animate_to(at.y, GRAB);
        drag.removing.animate_to(if removing { 1.0 } else { 0.0 }, FOLLOW);
        let key = drag.key.clone();
        // Move the tile to where it's hovering among the apps; the others slide over.
        let Some(from) = self.tile_index(&key) else { return };
        let (placed, refs) = self.place();
        let apps: Vec<(usize, f32)> = refs
            .iter()
            .zip(&placed)
            .filter_map(|(r, p)| r.map(|i| (i, p.center())))
            .filter(|(i, _)| self.tiles[*i].kind == Kind::App && !self.tiles[*i].leaving)
            .collect();
        let mut to = apps.iter().filter(|(i, c)| *i != from && *c < at.x).count();
        // Running apps nobody pinned stay after the pinned ones.
        let pinned_count = apps.iter().filter(|(i, _)| *i != from && self.tiles[*i].pinned).count();
        to = to.clamp(1, pinned_count.max(1));
        let current = apps.iter().position(|(i, _)| *i == from).unwrap_or(0);
        if to == current {
            return;
        }
        let before: Vec<(String, f32)> =
            refs.iter().zip(&placed).filter_map(|(r, p)| r.map(|i| (self.tiles[i].key.clone(), p.center()))).collect();
        let target_index =
            apps.iter().filter(|(i, _)| *i != from).nth(to).map_or(apps.last().map_or(from, |a| a.0 + 1), |a| a.0);
        let tile = self.tiles.remove(from);
        let insert = if target_index > from { target_index - 1 } else { target_index };
        self.tiles.insert(insert.min(self.tiles.len()), tile);
        if let Some(t) = self.tiles.iter_mut().find(|t| t.key == key) {
            t.pinned = true;
        }
        let (placed, refs) = self.place();
        for (r, p) in refs.iter().zip(&placed) {
            let Some(i) = *r else { continue };
            if let Some((_, old)) = before.iter().find(|(k, _)| *k == self.tiles[i].key) {
                let t = &mut self.tiles[i];
                t.slide.snap_to(t.slide.value + old - p.center());
                t.slide.animate_to(0.0, tiles::SLIDE);
            }
        }
    }

    fn handle_drops(&mut self, placed: &[Placed], refs: &[Option<usize>]) {
        if self.drops.is_empty() {
            return;
        }
        let files = std::mem::take(&mut self.drops);
        if self.land(&files) {
            return;
        }
        let Some(at) = self.pointer else { return };
        let over = self.tile_at(at, placed, refs);
        let apps: Vec<&PathBuf> = files.iter().filter(|f| os::is_app(f)).collect();
        match over.map(|i| (self.tiles[i].kind, self.tiles[i].path.clone(), i)) {
            Some((Kind::Trash, _, _)) => {
                for f in &files {
                    os::move_to_trash(f);
                }
                self.trash_full = true;
            }
            Some((Kind::App, app, i)) if apps.is_empty() => {
                os::open_with(&app, &files);
                self.tiles[i].gulp.velocity = tiles::GULP_KICK;
            }
            Some((Kind::Folder, folder, _)) => os::open_with(&PathBuf::from(tiles::FINDER), &[folder]),
            _ if !apps.is_empty() && at.y > self.body_top() - 40.0 => {
                // Apps dropped on the app side are kept in the dock where they land.
                let at_index = refs
                    .iter()
                    .zip(placed)
                    .filter_map(|(r, p)| r.map(|i| (i, p.center())))
                    .filter(|(i, c)| self.tiles[*i].pinned && self.tiles[*i].kind == Kind::App && *c < at.x)
                    .count();
                for (n, app) in apps.into_iter().enumerate() {
                    self.config.pin(app.clone(), at_index + n);
                }
                self.save();
            }
            _ => {}
        }
    }

    /// Bursts tile `i` from `at`: its slot stays open while the bubble swells, then closes as it pops.
    pub(crate) fn burst(&mut self, i: usize, at: Pos2, size: f32) {
        let t = &mut self.tiles[i];
        t.leaving = true;
        t.popped = true;
        t.hold = BURST_SWELL;
        t.presence.animate_to(t.presence.value.max(0.0), tiles::LEAVE);
        // Seven droplets all round, flung mostly up and out, each a little different, the same every time for the
        // same app.
        let mut seed = t.key.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 1000) as f32 / 1000.0
        };
        let unit = size / 48.0;
        let drops = (0..7)
            .map(|n| {
                let angle = lerp(-3.4, 0.26, (n as f32 + 0.2 + next() * 0.6) / 7.0);
                let speed = lerp(70.0, 120.0, next()) * unit;
                (vec2(angle.cos(), angle.sin()) * speed, lerp(4.0, 7.5, next()) * unit)
            })
            .collect();
        self.bursts.push(Burst { at, size, icon: t.icon.clone(), t: 0.0, drops });
    }

    /// A pop of the last app's icon, drawn over its tile, without touching any tile (a test, to film the pop).
    pub fn test_burst(&mut self) {
        let Some(i) = self.tiles.iter().rposition(|t| t.kind == Kind::App && !t.leaving) else { return };
        let at = self.icon_rect(i);
        let icon = self.tiles[i].icon.clone();
        let mut drops = Vec::new();
        let unit = at.width() / 48.0;
        for n in 0..7 {
            let angle = lerp(-3.4, 0.26, (n as f32 + 0.5) / 7.0);
            drops.push((vec2(angle.cos(), angle.sin()) * 95.0 * unit, 5.5 * unit));
        }
        self.bursts.push(Burst { at: at.center(), size: at.width(), icon, t: 0.0, drops });
    }

    /// Where each tile's centre is now, by key, so a change of order can slide instead of jump.
    fn centers(&self) -> Vec<(String, f32)> {
        let (placed, refs) = self.place();
        refs.iter().zip(&placed).filter_map(|(r, p)| r.map(|i| (self.tiles[i].key.clone(), p.center()))).collect()
    }

    /// Slides every tile from where it was (`centers`) to where it is now.
    fn slide_from(&mut self, before: &[(String, f32)]) {
        let (placed, refs) = self.place();
        for (r, p) in refs.iter().zip(&placed) {
            let Some(i) = *r else { continue };
            if let Some((_, old)) = before.iter().find(|(k, _)| *k == self.tiles[i].key) {
                let t = &mut self.tiles[i];
                t.slide.snap_to(t.slide.value + old - p.center());
                t.slide.animate_to(0.0, tiles::SLIDE);
            }
        }
    }

    /// The tile a dragged-in app would land in front of, at `x` along the dock: among the pinned apps, after Finder,
    /// and never on the folders' side, where a drop opens or trashes instead.
    fn insertion_before(&self, x: f32) -> Option<usize> {
        let (placed, refs) = self.place();
        let mut first_other = None;
        let mut after_pinned = None;
        for (r, p) in refs.iter().zip(&placed) {
            let Some(i) = *r else { continue };
            let t = &self.tiles[i];
            if t.section() == 1 {
                first_other.get_or_insert(i);
                if x > p.left {
                    return None;
                }
                continue;
            }
            if t.leaving || t.kind != Kind::App {
                continue;
            }
            if is_finder(t) {
                after_pinned = after_pinned.or(Some(i + 1));
                continue;
            }
            if t.pinned {
                if p.center() > x {
                    return Some(i);
                }
                after_pinned = Some(i + 1);
            }
        }
        after_pinned.or(first_other).or(Some(self.tiles.len()))
    }

    /// Opens, moves and closes the gap for apps dragged over the dock.
    fn update_incoming(&mut self, dt: f32) {
        let Some(incoming) = &self.incoming else { return };
        let over = self.files_over
            && !incoming.apps.is_empty()
            && self.away.target == 0.0
            && self.drag.is_none()
            && self.pointer.is_some_and(|p| p.y > self.body_top() - 40.0);
        let before = if over { self.pointer.and_then(|p| self.insertion_before(p.x)) } else { None };
        let was = incoming.before;
        let open = incoming.open.value;
        if let Some(before) = before
            && was != Some(before)
        {
            let centers = self.centers();
            if let Some(g) = &mut self.incoming {
                g.before = Some(before);
            }
            if open > 0.001 {
                self.slide_from(&centers);
            }
        }
        let gap_x = {
            let (placed, _) = self.place();
            placed.iter().find(|p| p.gap).map(|p| p.center())
        };
        let Some(g) = &mut self.incoming else { return };
        g.open.animate_to(if before.is_some() { 1.0 } else { 0.0 }, GAP);
        g.open.step(dt);
        if let Some(x) = gap_x {
            if g.open.value < 0.05 {
                g.x.snap_to(x);
            }
            g.x.animate_to(x, FOLLOW);
        }
        g.x.step(dt);
        if !self.files_over && g.open.is_settled() && g.open.value <= 0.0 {
            self.incoming = None;
        }
    }

    /// Dropped apps take the open gap's place and hop in. Returns whether the drop was theirs.
    fn land(&mut self, files: &[PathBuf]) -> bool {
        let apps: Vec<PathBuf> = files.iter().filter(|f| os::is_app(f)).cloned().collect();
        let Some(incoming) = self.incoming.take_if(|g| g.open.value > 0.15 && g.before.is_some()) else {
            return false;
        };
        if apps.is_empty() {
            self.incoming = Some(incoming);
            return false;
        }
        let before = incoming.before.unwrap_or(self.tiles.len()).min(self.tiles.len());
        // In the saved order: in front of the pinned app the gap was in front of, or after them all.
        let index = self
            .tiles
            .get(before)
            .filter(|t| t.pinned && t.kind == Kind::App && !is_finder(t))
            .and_then(|t| self.config.pinned.iter().position(|p| *p == t.path))
            .unwrap_or(self.config.pinned.len());
        for (n, app) in apps.iter().enumerate() {
            self.config.pin(app.clone(), index + n);
        }
        self.save();
        let centers = self.centers();
        let running = self.running.clone();
        self.refresh(&running);
        self.slide_from(&centers);
        let share = incoming.open.value.max(0.0) / apps.len() as f32;
        for app in &apps {
            let key = tiles::key_of(app);
            if let Some(t) = self.tiles.iter_mut().find(|t| t.key == key) {
                if !centers.iter().any(|(k, _)| *k == key) {
                    // New here: it fills the gap exactly, so nothing beside it moves.
                    t.presence.snap_to(share);
                    t.presence.animate_to(1.0, tiles::ARRIVE);
                }
                t.fall.snap_to(4.0);
                t.fall.velocity = tiles::HOP;
                t.fall.animate_to(0.0, tiles::LAND);
                t.gulp.velocity = tiles::GULP_KICK * 0.6;
            }
        }
        true
    }

    pub fn step(&mut self, dt: f32) {
        self.time += dt;
        self.away.step(dt);
        self.tucked.step(dt);
        // Dragging the separator up makes the icons bigger, down smaller, point for point, as in the Dock.
        if let (Some((from, size)), Some(p)) = (self.resizing, self.pointer) {
            let wanted = (size + (from - p.y)).clamp(SMALLEST, LARGEST).round();
            if wanted != self.config.tile_size {
                self.config.tile_size = wanted;
                self.config.large_size = self.config.large_size.max(wanted);
            }
        }
        let (placed, refs) = self.place();
        self.over_separator =
            self.resizing.is_some() || self.pointer.is_some_and(|p| self.separator_at(p, &placed, &refs));
        self.handle_drops(&placed, &refs);
        self.update_incoming(dt);
        for b in &mut self.bursts {
            b.t += dt;
        }
        self.bursts.retain(|b| b.t < BURST_SWELL + BURST_FLY);
        self.step_stacks(dt.min(LONGEST_POPUP_STEP));
        self.step_bars(dt);

        // Hold to open the menu, move to drag.
        if let (Some(press), Some(at)) = (&mut self.press, self.pointer)
            && !press.handled
            && let Some(key) = press.tile.clone()
        {
            let moved = (at - press.at).length() > DRAG_START;
            let is_app = self.tiles.iter().find(|t| t.key == key).is_some_and(|t| t.kind == Kind::App && !is_finder(t));
            if moved && is_app && self.drag.is_none() {
                press.handled = true;
                let pinned_before = self.tiles.iter().find(|t| t.key == key).is_some_and(|t| t.pinned);
                let i = self.tile_index(&key).unwrap_or(0);
                let start = self.icon_rect(i).center();
                let mut ax = Spring::new(start.x, GRAB, POINT_PRECISION);
                let mut ay = Spring::new(start.y, GRAB, POINT_PRECISION);
                ax.animate_to(at.x, GRAB);
                ay.animate_to(at.y, GRAB);
                self.drag =
                    Some(Drag { key, at: [ax, ay], removing: Spring::new(0.0, FOLLOW, UNIT_PRECISION), pinned_before });
            } else if !moved && press.when.elapsed().as_secs_f32() > HOLD_FOR_MENU {
                press.handled = true;
                self.open_menu(key);
            }
        }
        if let (Some(at), true) = (self.pointer, self.drag.is_some()) {
            self.update_drag(at);
        }
        if let Some(drag) = &mut self.drag {
            drag.at[0].step(dt);
            drag.at[1].step(dt);
            drag.removing.step(dt);
        }

        // Magnify while the pointer is over the dock.
        let hot = self.pointer.is_some_and(|p| {
            let top = self.icon_top(self.peak()) - PAD_TOP;
            let span = placed.first().zip(placed.last()).map(|(a, b)| (a.left - EDGE, b.right() + EDGE));
            span.is_some_and(|(l, r)| p.x >= l && p.x <= r && p.y >= top && p.y <= self.size.y)
        }) && !self.menu_open();

        // Only the icon under the pointer lifts; the one pressed sinks a little.
        // An app held over the gap is landing, not being dropped onto its neighbour.
        let landing = self.incoming.as_ref().is_some_and(|g| g.open.target > 0.0);
        let under = if (hot || self.files_over) && self.drag.is_none() && !landing {
            self.pointer.and_then(|p| {
                // Hold on to the icon already lifted until the pointer is clearly past its edge, so a pointer
                // resting on the line between two icons doesn't flick between them.
                let held = self.hovered.as_deref().and_then(|key| self.tile_index(key)).filter(|&i| {
                    refs.iter().position(|r| *r == Some(i)).is_some_and(|s| {
                        let slot = placed[s];
                        p.x >= slot.left - HOVER_HOLD && p.x < slot.right() + HOVER_HOLD
                    })
                });
                held.or_else(|| self.tile_at(p, &placed, &refs))
            })
        } else {
            None
        };
        // Over the previews, their app's icon stays lifted; under an open menu it stays lifted throughout, so the
        // menu sits just above the lifted icon rather than high over a flat dock.
        let under = under.or_else(|| {
            let popup = self.popup.as_ref().filter(|p| !p.shut)?;
            let inside = self.pointer.is_some_and(|p| popup.rect.contains(p));
            if (inside || !popup.previews()) && self.drag.is_none() { self.tile_index(&popup.key) } else { None }
        });
        let newly = under.map(|i| self.tiles[i].key.clone());
        // Resting on a running app reads its own actions ahead of a right-click.
        if newly != self.hovered
            && let Some(pid) = under.and_then(|i| self.tiles[i].pid)
        {
            self.read_app_menu(pid);
        }
        // Resting on Claude or ChatGPT reads its usage ahead too, so the menu opens with the numbers in.
        if self.live
            && let Some(provider) = under.and_then(|i| Provider::of(&self.tiles[i].path))
        {
            self.usage.poll(provider);
        }
        self.hovered = newly;
        self.update_previews(dt, under);
        self.take_app_menus();
        self.step_usage(dt);
        let pressed = self.press.as_ref().filter(|p| !p.handled && self.drag.is_none()).and_then(|p| p.tile.clone());
        for (i, t) in self.tiles.iter_mut().enumerate() {
            // In with the notch's HOVER, out a little softer, both critically damped: the same spring retargeted,
            // so it eases back from wherever it is with its speed kept.
            if under == Some(i) {
                t.hover.animate_to(1.0, tiles::HOVER);
            } else {
                t.hover.animate_to(0.0, tiles::HOVER_OUT);
            }
            t.down.animate_to(if pressed.as_deref() == Some(&t.key) { 1.0 } else { 0.0 }, tiles::PRESS);
            t.step(dt);
        }
        let before = self.tiles.len();
        self.tiles.retain(|t| !t.gone());
        if self.tiles.len() != before {
            self.drag = self.drag.take().filter(|d| self.tiles.iter().any(|t| t.key == d.key));
        }

        // Tooltip follows the icon under the pointer.
        let hovered =
            if self.config.show_names && hot && self.press.as_ref().is_none_or(|p| p.handled) { under } else { None };
        match hovered {
            Some(i) => {
                let slot = refs.iter().position(|r| *r == Some(i)).map(|s| placed[s]);
                if let Some(slot) = slot {
                    let x = slot.center() + self.tiles[i].slide.value;
                    if self.tooltip.key.is_none() || self.tooltip.open.value < 0.05 {
                        self.tooltip.x.snap_to(x);
                    }
                    self.tooltip.x.animate_to(x, FOLLOW);
                }
                if self.tooltip.key.as_deref() != Some(&self.tiles[i].key) {
                    self.tooltip.key = Some(self.tiles[i].key.clone());
                    self.tooltip.words = self.tiles[i].name.clone();
                }
                self.tooltip.open.animate_to(1.0, OOZE);
            }
            None => {
                self.tooltip.open.animate_to(0.0, OOZE_SHUT);
                if self.tooltip.open.value < 0.02 {
                    self.tooltip.key = None;
                }
            }
        }
        self.tooltip.open.step(dt);
        self.tooltip.x.step(dt);
        self.tooltip.width.step(dt);

        if let Some(popup) = &mut self.popup {
            // A late frame (an app launching as the stack closes, say) slows a popup down a touch rather than
            // making it leap, as with the restore flight.
            let dt = dt.min(LONGEST_POPUP_STEP);
            popup.open.step(dt);
            for s in &mut popup.plate {
                s.step(dt);
            }
            popup.plate_shown.step(dt);
            popup.fade.step(dt);
            for s in popup.frame.iter_mut().flatten() {
                s.step(dt);
            }
            let folded = match &popup.kind {
                PopupKind::Stack(s) => s.settled_shut(),
                _ => true,
            };
            if popup.shut && popup.open.value < 0.02 && folded {
                self.popup = None;
            }
        }
    }

    /// Paints icons, dots and labels with `painter` and returns the liquid underneath them.
    pub fn compose(&mut self, painter: &egui::Painter) -> Vec<Blob> {
        self.icon_budget = 3;
        self.upload_pictures(painter.ctx());
        let mut faded = painter.clone();
        faded.set_opacity(self.opacity());
        let painter = &faded;
        let mut blobs = Vec::with_capacity(48);
        let (placed, refs) = self.place();
        let tile = self.tile_size();
        let bottom = self.bottom();
        let top = self.body_top();
        let body_h = self.body_height();
        // Concentric with the icons: their art's corner plus the space around it.
        let radius = (tile * (0.225 * (1.0 - 2.0 * ART_INSET) + ART_INSET) + PAD_TOP).min(body_h * 0.5);
        let time = self.time;

        // The body: one blob per section, joined by a liquid neck where the separator is.
        // The body: one calm slab, with a hairline where the apps end.
        let mut bounds = Rect::NOTHING;
        if let (Some(first), Some(last)) = (placed.first(), placed.last()) {
            // Covering in Mission Control, the slab reaches down past the screen's edge, so the system Dock rising
            // under it is hidden all the way up; its rounded bottom corners go off screen.
            let p = self.cover_progress();
            let slab_bottom = if p > 0.0 { self.cover_slab_bottom(radius + 2.0) } else { bottom };
            let rect = Rect::from_min_max(pos2(first.left - EDGE, top), pos2(last.right() + EDGE, slab_bottom));
            bounds = rect;
            blobs.push(Blob::rect(rect, radius, 0.0));
            // Wherever the system Dock actually is (its size and how far Mission Control raises it differ between
            // Macs and macOS versions), the glass reaches over it, melting out of the slab.
            if let Some(real) = self.real_dock.filter(|_| p > 0.0).map(|r| r.expand(COVER_REACH))
                && !rect.contains_rect(real)
            {
                let reach =
                    real.union(Rect::from_min_max(pos2(real.left(), real.top()), pos2(real.right(), slab_bottom)));
                blobs.push(Blob::rect(reach, (reach.height() * 0.5).min(radius), 14.0 * self.cover_ease()));
            }
        }
        if let Some(sep) = placed.iter().zip(&refs).find_map(|(s, r)| (r.is_none() && !s.gap).then_some(*s)) {
            let ppp = painter.pixels_per_point();
            let x = (sep.center() * ppp).round() / ppp + 0.5 / ppp;
            let h = tile * 0.6;
            let cy = top + body_h * 0.5;
            painter.line_segment([pos2(x, cy - h * 0.5), pos2(x, cy + h * 0.5)], (1.0, white(0.15)));
        }

        // The cradle: the glass swells softly where a dragged-in app will land, holding a ghost of it.
        let icon_bottom = self.icon_bottom();
        if let Some(g) = &mut self.incoming
            && g.open.value > 0.01
        {
            let o = g.open.value.max(0.0);
            let x = g.x.value;
            let rise = 5.0 * o.min(1.2);
            let w = tile * 0.92 * o.min(1.1);
            let rect = Rect::from_min_max(pos2(x - w * 0.5, top - rise), pos2(x + w * 0.5, top + body_h * 0.5));
            blobs.push(Blob::rect(rect, tile * 0.3, 22.0 * (1.0 - (-rise / 6.0).exp())));
            if g.icon.is_none()
                && let Some(app) = g.apps.first()
                && let Some(img) = os::icon_for_path(app, 128)
            {
                g.icon = Some(painter.ctx().load_texture(format!("incoming:{}", app.display()), img, icon_options()));
            }
            if let Some(tex) = &g.icon {
                let bob = (time * 3.2).sin() * 1.2 * o.min(1.0);
                let size = tile * lerp(0.6, 0.9, o.min(1.0));
                let c = pos2(x, icon_bottom - tile * 0.5 - rise - bob);
                let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                let alpha = 0.45 * smoothstep(0.3, 1.0, o);
                painter.image(
                    tex.id(),
                    Rect::from_center_size(c, Vec2::splat(size)),
                    uv,
                    Color32::WHITE.gamma_multiply(alpha),
                );
            }
        }

        // Bursting bubbles: the dock's black glass wraps the icon and swells with a little wobble while the icon
        // sinks into it, then the skin snaps and breaks into droplets of the same liquid, with a ripple.
        for b in &self.bursts {
            let r = b.size * 0.5;
            if b.t < BURST_SWELL {
                let k = b.t / BURST_SWELL;
                let grow = 1.0 - (1.0 - k).powi(3);
                let radius = r * lerp(0.92, 1.32, grow);
                // Surface tension: it wobbles a touch wider, then taller, as it fills.
                let wobble = 0.05 * (k * std::f32::consts::TAU * 1.5).sin() * (1.0 - k * 0.5);
                let half = vec2(radius * (1.0 + wobble), radius * (1.0 - wobble));
                // It buds up out of the dock's glass, still joined to it by a melting neck.
                let c = b.at - vec2(0.0, r * BURST_RISE * grow);
                let melt = 20.0 * (1.0 - grow * 0.6);
                blobs.push(Blob::rect(Rect::from_center_size(c, half * 2.0), half.x.min(half.y), melt));
                if let Some(tex) = &b.icon {
                    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                    let size = b.size * lerp(1.0, 0.62, grow);
                    let alpha = 1.0 - smoothstep(0.15, 0.95, k);
                    painter.image(tex.id(), Rect::from_center_size(c, Vec2::splat(size)), uv, white(alpha));
                }
                // The glossy rim: a thin highlight high on the left.
                let glint: Vec<Pos2> = (0..12)
                    .map(|n| {
                        let a = lerp(-2.75, -1.75, n as f32 / 11.0);
                        c + vec2(a.cos() * half.x, a.sin() * half.y) * 0.86
                    })
                    .collect();
                painter.add(Shape::line(glint, (1.5, white(0.5 * smoothstep(0.0, 0.4, k)))));
                // The keyline the dock's glass has, so the black bubble reads against a dark window.
                let rim = Shape::ellipse_stroke(c, half, (1.0, white(0.16 * smoothstep(0.0, 0.5, k))));
                painter.add(rim);
                continue;
            }
            let s = b.t - BURST_SWELL;
            let u = (s / BURST_FLY).clamp(0.0, 1.0);
            let c = b.at - vec2(0.0, r * BURST_RISE);
            // What's left of the skin collapses into the middle as it snaps open.
            let core = r * 1.32 * (1.0 - smoothstep(0.0, 0.35, u)).powf(1.5);
            if core > 0.5 {
                blobs.push(Blob::circle(c, core, 0.0));
            }
            // The droplets leave from the rim and stay joined to it for a moment, like liquid tearing.
            let melt = 12.0 * (1.0 - smoothstep(0.0, 0.3, u));
            for (v, dr) in &b.drops {
                let out = v.normalized() * r * 1.05;
                let at = c + out + *v * s + vec2(0.0, 0.5 * BURST_GRAVITY * s * s);
                let size = dr * (1.0 - u).powf(0.9);
                if size > 0.4 {
                    blobs.push(Blob::circle(at, size, melt));
                }
            }
            let ring = 1.0 - (1.0 - u).powi(2);
            painter.circle_stroke(c, r * 1.32 + 16.0 * ring, (1.0, white(0.22 * (1.0 - ring))));
        }

        // Running dots sit in the middle of the space under the icons' art.
        let dot_y = (self.icon_bottom() - tile * ART_INSET + bottom) * 0.5;
        let dragged = self.drag.as_ref().map(|d| d.key.clone());
        let mut icons: Vec<(usize, Rect, f32)> = Vec::new();
        for (slot, r) in placed.iter().zip(&refs) {
            let Some(i) = *r else { continue };
            let t = &self.tiles[i];
            let p = t.presence.value;
            let cx = slot.center() + t.slide.value;
            let size = tile * slot.scale * self.beat_of(t);
            let hover = t.hover.value.max(0.0);
            // The launch swell rises under the icon and carries it up, like the notch's gulp.
            let gulp = t.gulp.value;
            // A dropped-in app hops off the cradle: stretched while it flies, squashed for a moment as it lands.
            let fall = t.fall.value;
            let lift = self.hover_lift() * hover + gulp.max(0.0) + fall;
            let squash = if t.fall.is_settled() {
                1.0
            } else {
                let flying = (t.fall.velocity.abs() * 0.0005).min(0.07);
                let landing = (fall.min(0.0) * 0.06).max(-0.14);
                1.0 + flying + landing
            };

            // A soft rise in the liquid under the hovered (or swelling) icon. At rest it sits exactly on the body.
            if (hover > 0.004 || gulp.abs() > 0.05) && !t.leaving {
                let rise_top = self.icon_bottom() - size - lift - PAD_TOP * lerp(1.0, 0.55, hover);
                let rect = Rect::from_min_max(
                    pos2(cx - size * 0.5 + 2.0, rise_top.min(top + body_h * 0.5 - 1.0)),
                    pos2(cx + size * 0.5 - 2.0, top + body_h * 0.5),
                );
                // The melt grows smoothly with how far the rise stands above the body. A full melt on a rise that's
                // barely there would still bulge the body by k/6, and the bulge would vanish in one frame when the
                // rise is dropped.
                let height = (top - rect.top()).max(0.0);
                blobs.push(Blob::rect(rect, size * 0.32, 22.0 * (1.0 - (-height / 6.0).exp())));
            }

            if t.popped {
                continue;
            }
            if t.leaving {
                // A droplet carries the icon up and off the body, then melts.
                let rise = (1.0 - p) * tile * 1.25;
                let r = tile * 0.5 * p.max(0.0).powf(0.55) + 0.5;
                let c = pos2(cx, top + tile * 0.25 - rise);
                blobs.push(Blob::circle(c, r, 22.0 * p.max(0.0).powf(1.4)));
                let icon = Rect::from_center_size(c, Vec2::splat(tile * 0.86 * p.max(0.0).powf(0.7)));
                icons.push((i, icon, p.clamp(0.0, 1.0)));
                continue;
            }
            if p < 0.999 {
                // A droplet swells out of the body where the new slot opens.
                let bead = (p.clamp(0.0, 1.0) * std::f32::consts::PI).sin();
                let r = tile * 0.46 * bead;
                if r > 0.5 {
                    blobs.push(Blob::circle(pos2(cx, top + 2.0 - bead * tile * 0.18), r, 16.0));
                }
            }
            if dragged.as_deref() == Some(&t.key) {
                continue;
            }
            let grow = p.max(0.0);
            let w = size * grow * (2.0 - squash);
            let h = size * grow * squash;
            let icon = Rect::from_min_max(
                pos2(cx - w * 0.5, self.icon_bottom() - lift - h),
                pos2(cx + w * 0.5, self.icon_bottom() - lift),
            );
            icons.push((i, icon, smoothstep(0.0, 0.6, p)));

            if t.kind == Kind::Folder {
                self.paint_progress(painter, i, pos2(cx, dot_y), tile);
            }
            self.paint_tile_bar(painter, &t.key, pos2(cx, dot_y), tile);
            let t = &self.tiles[i];
            // A download's bar takes the running dot's place while it shows.
            let dot_alpha = 1.0 - self.bar_shown(&t.key);
            // While it launches, the dot breathes; once running it settles solid.
            if let Some(started) = t.launching {
                let since = started.elapsed().as_secs_f32();
                let breathe = 0.5 - 0.5 * (since * std::f32::consts::TAU / 1.1).cos();
                let c = pos2(cx, dot_y);
                painter.circle_filled(c, 1.5, white((0.2 + 0.5 * breathe) * smoothstep(0.0, 0.15, since)));
            }

            // Running dots arrive as falling droplets.
            let d = t.dot.value;
            if d > 0.02 {
                let stretch = (t.dot.velocity.abs() * 0.12).min(0.9);
                let r = 1.5 * d.min(1.2);
                let c = pos2(cx, dot_y - stretch * 2.0);
                painter.add(Shape::ellipse_filled(
                    c,
                    vec2(r * (1.0 - stretch * 0.3), r * (1.0 + stretch)),
                    white(0.7 * d.min(1.0) * dot_alpha),
                ));
            }
        }

        // The dragged icon rides its own droplet: it melts into the body nearby and pinches off when pulled away.
        if let Some(drag) = &self.drag
            && let Some(i) = self.tile_index(&drag.key)
        {
            let c = pos2(drag.at[0].value, drag.at[1].value);
            let size = tile * lerp(1.06, 0.9, drag.removing.value);
            blobs.push(Blob::circle(c, size * 0.58, 26.0));
            let alpha = lerp(1.0, 0.55, drag.removing.value);
            icons.push((i, Rect::from_center_size(c, Vec2::splat(size)), alpha));
            if drag.removing.value > 0.05 {
                let words = "Remove";
                let a = drag.removing.value;
                let at = c - vec2(0.0, size * 0.5 + 18.0);
                let galley = painter.layout_no_wrap(words.into(), font::semibold(12.0), PEARL);
                let w = galley.size().x + 20.0;
                blobs.push(Blob::rect(Rect::from_center_size(at, vec2(w, 22.0) * a.max(0.2)), 11.0, 10.0 * (1.0 - a)));
                painter.text(
                    at,
                    Align2::CENTER_CENTER,
                    words,
                    font::semibold(12.0),
                    PEARL.gamma_multiply(smoothstep(0.5, 1.0, a)),
                );
            }
        }

        // Tooltip: oozes up out of the swell, pinches off and floats above the icon.
        if let Some(key) = &self.tooltip.key
            && self.tooltip.open.value > 0.01
            && let Some(i) = self.tile_index(key)
        {
            let galley = painter.layout_no_wrap(self.tooltip.words.clone(), font::semibold(13.0), PEARL);
            let w = galley.size().x + 26.0;
            if (self.tooltip.width.target - w).abs() > 0.5 {
                if self.tooltip.open.value < 0.1 {
                    self.tooltip.width.snap_to(w);
                }
                self.tooltip.width.animate_to(w, FOLLOW);
            }
            let o = self.tooltip.open.value;
            let x = self.tooltip.x.value;
            let slot_scale = refs.iter().position(|r| *r == Some(i)).map_or(1.0, |s| placed[s].scale);
            let from_y = self.icon_top(slot_scale) - 2.0;
            let rest_y = self.icon_top(self.peak()) - PAD_TOP - 22.0;
            let y = lerp(from_y, rest_y, o.min(1.15));
            let size =
                vec2(lerp(16.0, self.tooltip.width.value, o.clamp(0.0, 1.1)), lerp(10.0, 26.0, o.clamp(0.0, 1.1)));
            let rect = Rect::from_center_size(pos2(x, y), size);
            if o < 0.97 {
                let neck_w = lerp(12.0, 1.0, smoothstep(0.2, 0.95, o));
                let neck = Rect::from_min_max(pos2(x - neck_w * 0.5, y), pos2(x + neck_w * 0.5, from_y + 8.0));
                blobs.push(Blob::rect(neck, neck_w * 0.5, 12.0));
            }
            blobs.push(Blob::rect(rect, size.y * 0.5, 14.0 * (1.0 - smoothstep(0.5, 1.0, o))));
            let a = smoothstep(0.55, 0.95, o);
            if a > 0.0 {
                let inside = painter.with_clip_rect(rect.shrink(2.0));
                let ppp = painter.pixels_per_point();
                let at = rect.center() - galley.size() * 0.5;
                let at = pos2((at.x * ppp).round() / ppp, (at.y * ppp).round() / ppp);
                inside.galley(at, galley, PEARL.gamma_multiply(a));
            }
        }

        let popup_blobs = self.compose_popup(painter, &placed, &refs);
        blobs.extend(popup_blobs);

        for (i, rect, alpha) in icons {
            self.paint_icon(painter, i, rect, alpha);
        }

        // The region that takes the mouse: the body with its swells, and an open popup.
        let swell_top = self.icon_top(self.peak()) - PAD_TOP;
        let mut zone = bounds;
        if zone.is_positive() {
            zone.min.y = zone.min.y.min(swell_top);
            zone.max.y = self.size.y;
        }
        if let Some(p) = &self.popup {
            zone = zone.union(p.rect);
        }
        self.interactive = zone.expand(2.0);
        let _ = time;
        blobs
    }

    fn compose_popup(&mut self, painter: &egui::Painter, placed: &[Placed], refs: &[Option<usize>]) -> Vec<Blob> {
        let mut blobs = Vec::new();
        let peak = self.peak();
        let icon_top_rest = self.icon_top(1.0);
        let size_x = self.size.x;
        let pointer = self.pointer;
        let icon_bottom = self.icon_bottom();
        // A floating panel clears the hovered icon's rise, not just the dock's edge.
        let float_bottom = (self.icon_top(self.peak()) - self.hover_lift() - PAD_TOP).min(self.body_top());
        let float = self.config.popup_attach == crate::config::PopupAttach::Float;
        let body_top = self.body_top();
        let tile_size = self.tile_size();
        let option_held = self.option_held;
        let dark = self.dark;
        let with_usage = self.menu_usage.map(|provider| (provider, self.usage.height_now()));
        if self.popup.as_ref().is_some_and(Popup::previews) {
            return self.compose_previews(painter, placed, refs);
        }
        if self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Stack(_))) {
            return self.compose_stack(painter, placed, refs);
        }
        let Some(popup) = &mut self.popup else { return blobs };
        let i = self.tiles.iter().position(|t| t.key == popup.key);
        // The separator's menu sits over the separator.
        let slot = if popup.key == SEPARATOR_KEY {
            placed.iter().zip(refs).find(|(s, r)| r.is_none() && !s.gap).map(|(s, _)| *s)
        } else {
            let Some(i) = i else {
                popup.shut = true;
                return blobs;
            };
            refs.iter().position(|r| *r == Some(i)).map(|s| placed[s])
        };
        let cx = slot.map_or(size_x * 0.5, |s| s.center());
        let scale = slot.map_or(1.0, |s| s.scale);
        let icon_top = icon_bottom - tile_size * scale;
        let _ = (peak, icon_top_rest);

        // Content size.
        let (w, h) = match &popup.kind {
            PopupKind::Menu(items) => {
                let widest = items
                    .iter()
                    .map(|i| painter.layout_no_wrap(i.words.clone(), font::regular(13.0), PEARL).size().x)
                    .fold(0.0, f32::max);
                let h: f32 = items.iter().map(Item::height).sum();
                let indent = if items.iter().any(|i| i.check.is_some()) { 14.0 } else { 0.0 };
                let (least, extra) = with_usage.map_or((180.0, 0.0), |(_, h)| (usage::WIDTH, h));
                ((widest + 44.0 + indent).max(least), h + extra + 12.0)
            }
            PopupKind::Stack(_) | PopupKind::Previews { .. } => return blobs,
        };
        let o = popup.open.value;
        let gap = if float { FLOAT_GAP } else { NECK_GAP };
        let final_rect = {
            let half = w * 0.5 + 8.0;
            let x = cx.clamp(half, (size_x - half).max(half));
            let base = if float { float_bottom } else { body_top };
            Rect::from_min_max(pos2(x - w * 0.5, base - gap - h), pos2(x + w * 0.5, base - gap))
        };
        let rect = panel_now(final_rect, cx, icon_top, o, float);
        let size = rect.size();
        popup.rect = final_rect;
        popup.drawn = rect;
        if !float {
            blobs.extend(neck(cx, rect, body_top, tile_size));
        }
        blobs.push(Blob::rect(rect, 15.0_f32.min(size.y * 0.5), if float { 0.0 } else { 8.0 }));

        // Contents only show inside the liquid as it grows.
        let clipped = painter.with_clip_rect(rect.shrink(3.0));
        let painter = &clipped;
        // A floating panel shows its contents once it has nearly reached full size.
        let a = if float { smoothstep(0.8, 1.0, o) } else { smoothstep(0.55, 0.95, o) };
        // Hover: which row or cell is under the pointer.
        let mut hover = None;
        let mut plate_rect = None;
        match &popup.kind {
            PopupKind::Menu(items) => {
                let mut y = final_rect.top() + 6.0;
                let indent = if items.iter().any(|i| i.check.is_some()) { 14.0 } else { 0.0 };
                for (n, item) in items.iter().enumerate() {
                    if item.action.is_none() && !item.words.is_empty() {
                        shaded_text(
                            painter,
                            pos2(final_rect.left() + 18.0, y + 14.0),
                            Align2::LEFT_CENTER,
                            &item.words,
                            font::semibold(11.0),
                            GRAY.gamma_multiply(a),
                        );
                        y += item.height();
                        continue;
                    }
                    if item.action.is_none() {
                        // A hairline one pixel tall on the pixel grid, inset like the menu's rows, in the ink the
                        // system's own menu separators use for this appearance.
                        let ppp = painter.pixels_per_point();
                        let ly = ((y + 4.5) * ppp).floor() / ppp;
                        let line = Rect::from_min_max(
                            pos2(final_rect.left() + 18.0, ly),
                            pos2(final_rect.right() - 18.0, ly + 1.0 / ppp),
                        );
                        painter.rect_filled(line, 0.0, divider_ink(dark).gamma_multiply(a));
                        y += 9.0;
                        continue;
                    }
                    let row =
                        Rect::from_min_max(pos2(final_rect.left() + 6.0, y), pos2(final_rect.right() - 6.0, y + 28.0));
                    if pointer.is_some_and(|p| row.contains(p)) {
                        hover = Some(n);
                        plate_rect = Some(row);
                    }
                    let color = if item.danger { RED } else { PEARL };
                    if item.check == Some(true) {
                        let c = pos2(row.left() + 15.0, row.center().y);
                        let stroke = egui::Stroke::new(1.6, PEARL.gamma_multiply(a));
                        painter.line_segment([c + vec2(-3.6, 0.2), c + vec2(-1.0, 2.8)], stroke);
                        painter.line_segment([c + vec2(-1.0, 2.8), c + vec2(3.8, -2.8)], stroke);
                    }
                    let words =
                        if item.action == Some(Action::Quit) && option_held { "Force Quit" } else { &item.words };
                    shaded_text(
                        painter,
                        pos2(row.left() + 12.0 + indent, row.center().y),
                        Align2::LEFT_CENTER,
                        words,
                        font::regular(13.0),
                        color.gamma_multiply(a),
                    );
                    y += 28.0;
                }
                if let Some((provider, _)) = with_usage {
                    self.usage.paint(painter, provider, final_rect, y, a, divider_ink(dark).gamma_multiply(a));
                }
            }
            PopupKind::Stack(_) => {}
            PopupKind::Previews { .. } => {}
        }
        popup.hover = hover;
        if let Some(r) = plate_rect {
            if popup.plate_shown.value < 0.05 {
                popup.plate[0].snap_to(r.top());
                popup.plate[1].snap_to(r.left());
            }
            popup.plate[0].animate_to(r.top(), PLATE);
            popup.plate[1].animate_to(r.left(), PLATE);
            popup.plate_shown.animate_to(1.0, PLATE);
            let lit = Rect::from_min_size(pos2(popup.plate[1].value, popup.plate[0].value), r.size());
            painter.rect_filled(lit, 8.0, white(0.11 * popup.plate_shown.value * a));
        } else {
            popup.plate_shown.animate_to(0.0, PLATE);
        }
        blobs
    }

    /// The previews panel: grows out of the icon like the menus, then glides from app to app with the pointer.
    fn compose_previews(&mut self, painter: &egui::Painter, placed: &[Placed], refs: &[Option<usize>]) -> Vec<Blob> {
        let mut blobs = Vec::new();
        let ppp = painter.pixels_per_point();
        if self.live && self.thumbs.is_none() {
            self.thumbs = Some(windows::Thumbs::new());
        }
        if let Some(thumbs) = &mut self.thumbs {
            thumbs.collect(painter.ctx());
        }
        let size_x = self.size.x;
        let pointer = self.pointer;
        // A floating panel clears the hovered icon's rise, not just the dock's edge.
        let float_bottom = (self.icon_top(self.peak()) - self.hover_lift() - PAD_TOP).min(self.body_top());
        let float = self.config.popup_attach == crate::config::PopupAttach::Float;
        let body_top = self.body_top();
        let tile_size = self.tile_size();
        let icon_bottom = self.icon_bottom();
        let Some(popup) = &mut self.popup else { return blobs };
        let Some(i) = self.tiles.iter().position(|t| t.key == popup.key) else {
            popup.shut = true;
            return blobs;
        };
        let PopupKind::Previews { wins, .. } = &popup.kind else { return blobs };
        let wins: Vec<Win> = wins.iter().take(MOST_WINDOWS).cloned().collect();
        let app_icon = self.tiles[i].icon.clone();
        let app_name = self.tiles[i].name.clone();
        let slot = refs.iter().position(|r| *r == Some(i)).map(|s| placed[s]);
        let cx = slot.map_or(size_x * 0.5, |s| s.center());
        let icon_top = icon_bottom - tile_size * slot.map_or(1.0, |s| s.scale);

        // Each window keeps its own shape, from its picture once there is one.
        let thumbs = &mut self.thumbs;
        let aspect = |w: &Win| -> f32 {
            let from_picture = thumbs.as_ref().and_then(|t| t.get(w.id)).map(|t| {
                let [x, y] = t.size();
                x as f32 / y.max(1) as f32
            });
            from_picture.unwrap_or(w.frame.width() / w.frame.height().max(1.0)).clamp(0.7, 1.9)
        };
        let aspects: Vec<f32> = wins.iter().map(aspect).collect();
        let n = wins.len().max(1) as f32;
        let natural: f32 = aspects.iter().sum::<f32>().max(0.7);
        let room = size_x - 48.0 - 2.0 * PANEL_PAD - CARD_GAP * (n - 1.0);
        let thumb_h = THUMB_HEIGHT.min(room / natural).max(56.0);
        let w = natural * thumb_h + CARD_GAP * (n - 1.0) + 2.0 * PANEL_PAD;
        let h = PANEL_PAD + thumb_h + 6.0 + TITLE_HEIGHT + PANEL_PAD - 4.0;
        let gap = if float { FLOAT_GAP } else { NECK_GAP };
        let half = w * 0.5 + 8.0;
        let target_x = cx.clamp(half, (size_x - half).max(half));
        let base = if float { float_bottom } else { body_top };
        let target = [target_x, base - gap - h * 0.5, w, h, cx];

        let frame = popup.frame.get_or_insert_with(|| target.map(|v| Spring::new(v, PANEL_MOVE, POINT_PRECISION)));
        for (s, v) in frame.iter_mut().zip(target) {
            s.animate_to(v, PANEL_MOVE);
        }
        let [fx, fy, fw, fh, stem_x] = frame.each_ref().map(|s| s.value);
        let o = popup.open.value;
        let settled = Rect::from_center_size(pos2(fx, fy), vec2(fw, fh));
        let rect = panel_now(settled, stem_x, icon_top, o, float);
        let size = rect.size();
        popup.drawn = rect;
        // The panel and the strip down to the icon, so the pointer can travel between them.
        let bridge = Rect::from_min_max(
            pos2(stem_x - tile_size * 0.5, settled.bottom()),
            pos2(stem_x + tile_size * 0.5, icon_top + 2.0),
        );
        popup.rect = settled.union(bridge);

        if !float {
            blobs.extend(neck(stem_x, rect, body_top, tile_size));
        }
        blobs.push(Blob::rect(rect, 18.0_f32.min(size.y * 0.5), if float { 0.0 } else { 8.0 }));

        let clipped = painter.with_clip_rect(rect.shrink(3.0));
        let painter = &clipped;
        let a = if float { smoothstep(0.8, 1.0, o) } else { smoothstep(0.55, 0.95, o) } * popup.fade.value;
        let snap = |v: f32| (v * ppp).round() / ppp;
        let mut x = fx - fw * 0.5 + PANEL_PAD;
        let top = snap(fy - fh * 0.5 + PANEL_PAD);
        let mut hover = None;
        let mut closing = None;
        let mut plate_rect = None;
        let mut close_buttons = Vec::new();
        for (n, (win, aspect)) in wins.iter().zip(&aspects).enumerate() {
            let card = Rect::from_min_size(pos2(snap(x), top), vec2(snap(aspect * thumb_h), thumb_h));
            x += aspect * thumb_h + CARD_GAP;
            let cell = Rect::from_min_max(card.min, pos2(card.right(), card.bottom() + 6.0 + TITLE_HEIGHT));
            let close_at = card.min + vec2(11.0, 11.0);
            if pointer.is_some_and(|p| cell.expand(CARD_GAP * 0.5).contains(p)) {
                hover = Some(n);
                plate_rect = Some(cell.expand(6.0));
                if pointer.is_some_and(|p| (p - close_at).length() < 10.0) {
                    closing = Some(n);
                }
            }
            if let Some(thumbs) = thumbs.as_mut() {
                thumbs.want(win.id, (thumb_h * ppp).ceil() as u32, 1.0);
            }
            // A minimized window shows a little dimmer, as it sits in the dock rather than on screen.
            let shade = a * if win.on_screen { 1.0 } else { 0.7 };
            let picture = thumbs.as_ref().and_then(|t| t.get(win.id));
            match picture {
                Some(tex) => {
                    let [pw, ph] = tex.size();
                    let fit = (card.width() / pw as f32).min(card.height() / ph as f32);
                    let shown = Rect::from_center_size(card.center(), vec2(pw as f32, ph as f32) * fit);
                    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                    painter.add(
                        egui::epaint::RectShape::filled(shown, 8.0, Color32::WHITE.gamma_multiply(shade))
                            .with_texture(tex.id(), uv),
                    );
                    painter.rect_stroke(shown, 8.0, (1.0, white(0.08 * shade)), egui::StrokeKind::Inside);
                }
                None => {
                    painter.rect_filled(card, 8.0, white(0.05 * shade));
                    if let Some(tex) = &app_icon {
                        let side = (thumb_h * 0.42).min(56.0);
                        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                        let at = Rect::from_center_size(card.center(), Vec2::splat(side));
                        painter.image(tex.id(), at, uv, Color32::WHITE.gamma_multiply(shade));
                    }
                }
            }
            let title = if win.title.is_empty() { app_name.as_str() } else { win.title.as_str() };
            let words = ellipsize(painter, title, card.width().max(40.0), font::regular(12.0));
            // Laid out uncoloured, so the colour passed when drawing (with the panel's fade) is the one used.
            let galley = painter.layout_no_wrap(words.clone(), font::regular(12.0), Color32::PLACEHOLDER);
            let at = pos2(
                card.center().x - galley.size().x * 0.5,
                card.bottom() + 6.0 + (TITLE_HEIGHT - galley.size().y) * 0.5,
            );
            let shade = painter.layout_no_wrap(words, font::regular(12.0), Color32::PLACEHOLDER);
            painter.galley(pos2(snap(at.x), snap(at.y) + 1.0), shade, Color32::from_black_alpha((110.0 * a) as u8));
            painter.galley(pos2(snap(at.x), snap(at.y)), galley, PEARL.gamma_multiply(a * 0.92));
            if hover == Some(n) {
                close_buttons.push((close_at, closing == Some(n)));
            }
        }
        popup.hover = hover;
        popup.closing = closing;
        if let Some(r) = plate_rect {
            if popup.plate_shown.value < 0.05 {
                popup.plate[0].snap_to(r.top());
                popup.plate[1].snap_to(r.left());
            }
            popup.plate[0].animate_to(r.top(), PLATE);
            popup.plate[1].animate_to(r.left(), PLATE);
            popup.plate_shown.animate_to(1.0, PLATE);
            let lit = Rect::from_min_size(pos2(popup.plate[1].value, popup.plate[0].value), r.size());
            // Under the pictures: painted first in their layer would need a second pass, so it's kept faint.
            painter.rect_filled(lit, 10.0, white(0.07 * popup.plate_shown.value * a));
        } else {
            popup.plate_shown.animate_to(0.0, PLATE);
        }
        for (c, lit) in close_buttons {
            painter.circle_filled(c, 8.0, Color32::from_gray(if lit { 78 } else { 46 }).gamma_multiply(a));
            painter.circle_stroke(c, 8.0, (0.8, Color32::from_black_alpha((90.0 * a) as u8)));
            let d = 2.8;
            let stroke = egui::Stroke::new(1.4, PEARL.gamma_multiply(a));
            painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
            painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
        }
        blobs
    }

    fn paint_icon(&mut self, painter: &egui::Painter, i: usize, rect: Rect, alpha: f32) {
        if rect.width() < 1.0 || alpha <= 0.0 {
            return;
        }
        let px =
            ((self.config.magnified() * painter.ctx().pixels_per_point()).ceil() as u32).next_multiple_of(32).min(512);
        let trash_full = self.trash_full;
        // Sharper icons for a new size come in a few a frame, the old ones showing meanwhile, so letting go of the
        // separator never stalls a frame.
        let fresh = self.tiles[i].stale && self.icon_budget > 0;
        if fresh {
            self.icon_budget -= 1;
        }
        let t = &mut self.tiles[i];
        let wants_full = t.kind == Kind::Trash && trash_full;
        if fresh {
            t.stale = false;
        }
        if t.icon.is_none() || fresh || (t.kind == Kind::Trash && t.icon_full != wants_full) {
            let image = match t.kind {
                Kind::Trash => os::trash_icon(wants_full, px).or_else(|| os::icon_for_path(&t.path, px)),
                Kind::App => t.pid.and_then(|pid| os::icon_for_pid(pid, px)).or_else(|| os::icon_for_path(&t.path, px)),
                Kind::Window => t
                    .window
                    .as_ref()
                    .and_then(|(pid, _)| os::icon_for_pid(*pid, px))
                    .or_else(|| os::icon_for_path(&t.path, px)),
                Kind::Folder => os::icon_for_path(&t.path, px),
            };
            t.icon_full = wants_full;
            if let Some(image) = image {
                t.icon = Some(painter.ctx().load_texture(t.key.clone(), image, icon_options()));
            }
        }
        // Folders and the Trash fill more of their canvas than app icons do; drawn a touch smaller, they read as the
        // same size as the apps, on the same baseline.
        let optical = match t.kind {
            Kind::App | Kind::Window => 1.0,
            Kind::Folder => 0.86,
            Kind::Trash => 0.9,
        };
        let rect = Rect::from_min_max(
            pos2(rect.center().x - rect.width() * optical * 0.5, rect.bottom() - rect.height() * (1.0 + optical) * 0.5),
            pos2(rect.center().x + rect.width() * optical * 0.5, rect.bottom() - rect.height() * (1.0 - optical) * 0.5),
        );
        // Whole device pixels, so icons stay razor sharp.
        let ppp = painter.pixels_per_point();
        let snap = |v: f32| (v * ppp).round() / ppp;
        let rect = Rect::from_min_size(
            pos2(snap(rect.min.x), snap(rect.min.y)),
            vec2(snap(rect.width()), snap(rect.height())),
        );
        if t.kind == Kind::Window {
            self.paint_window(painter, i, rect, alpha);
            return;
        }
        if t.kind == Kind::Folder && self.paint_pile(painter, i, rect, alpha) {
            return;
        }
        let t = &self.tiles[i];
        if let Some(tex) = &t.icon {
            // A soft shadow under the art lifts it off the glass.
            if t.kind == Kind::App {
                let art = rect.shrink(rect.width() * ART_INSET);
                let shadow = egui::epaint::Shadow {
                    offset: [0, 2],
                    blur: 6,
                    spread: 0,
                    color: Color32::from_black_alpha((60.0 * alpha) as u8),
                };
                painter.add(shadow.as_shape(art, (art.width() * 0.225) as u8));
            }
            let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
            painter.image(tex.id(), rect, uv, Color32::WHITE.gamma_multiply(alpha));
        }
        if t.kind == Kind::App
            && let Some(label) = crate::badges::for_app(&self.badges, &t.path, &t.name)
        {
            crate::badges::paint(painter, rect.shrink(rect.width() * ART_INSET), label, alpha);
        }
    }
}

impl Dock {
    /// A minimized window's tile: a picture of the window, fitted to the tile, with its app's icon in the corner,
    /// like the Dock. Without a picture (no Screen Recording, or none yet), the app's icon with a window badge.
    fn paint_window(&mut self, painter: &egui::Painter, i: usize, rect: Rect, alpha: f32) {
        let ppp = painter.pixels_per_point();
        // Asks for Screen Recording once, the first time a window goes down; until it's allowed, the badge stands in.
        if self.live && self.thumbs.is_none() && windows::can_capture(true) {
            self.thumbs = Some(windows::Thumbs::new());
        }
        let id = self.tiles[i].window.as_ref().map(|(_, w)| w.id);
        if id.is_some_and(|id| self.veiled.contains(&id)) {
            return;
        }
        if self.thumbs.is_none() && !self.pictures.is_empty() {
            self.thumbs = Some(windows::Thumbs::new());
        }
        if let Some(thumbs) = &mut self.thumbs {
            for (id, picture) in self.pictures.drain(..) {
                thumbs.put(painter.ctx(), id, picture);
            }
        }
        if let (Some(thumbs), Some(id)) = (&mut self.thumbs, id) {
            thumbs.collect(painter.ctx());
            thumbs.want(id, (rect.height() * ppp * 2.0).ceil() as u32, WINDOW_PICTURE_EVERY);
        }
        let picture = id.and_then(|id| self.thumbs.as_ref()?.get(id)).map(|t| (t.id(), t.size_vec2()));
        let t = &self.tiles[i];
        let art = rect.shrink(rect.width() * ART_INSET);
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        match picture {
            Some((texture, size)) if size.x > 0.0 && size.y > 0.0 => {
                let fit = (art.width() / size.x).min(art.height() / size.y);
                let shown = Rect::from_center_size(pos2(art.center().x, art.bottom() - size.y * fit * 0.5), size * fit);
                let shown = Rect::from_min_max(
                    pos2((shown.min.x * ppp).round() / ppp, (shown.min.y * ppp).round() / ppp),
                    pos2((shown.max.x * ppp).round() / ppp, (shown.max.y * ppp).round() / ppp),
                );
                let shadow = egui::epaint::Shadow {
                    offset: [0, 2],
                    blur: 6,
                    spread: 0,
                    color: Color32::from_black_alpha((70.0 * alpha) as u8),
                };
                painter.add(shadow.as_shape(shown, 4));
                painter.add(
                    egui::epaint::RectShape::filled(shown, 4.0, Color32::WHITE.gamma_multiply(alpha))
                        .with_texture(texture, uv),
                );
                painter.rect_stroke(shown, 4.0, (0.75, white(0.12 * alpha)), egui::StrokeKind::Inside);
                if let Some(icon) = &t.icon {
                    let badge = rect.width() * 0.42;
                    let at = Rect::from_min_size(
                        pos2(art.right() - badge * 0.78, art.bottom() - badge * 0.86),
                        vec2(badge, badge),
                    );
                    painter.image(icon.id(), at, uv, Color32::WHITE.gamma_multiply(alpha));
                }
            }
            _ => {
                if let Some(icon) = &t.icon {
                    painter.image(icon.id(), rect, uv, Color32::WHITE.gamma_multiply(alpha * 0.92));
                }
                // A small window in the corner says it's a window, not the app.
                let w = rect.width() * 0.34;
                let badge =
                    Rect::from_min_size(pos2(art.right() - w * 0.8, art.bottom() - w * 0.72), vec2(w, w * 0.74));
                painter.rect_filled(badge, 3.0, Color32::from_rgb(28, 28, 30).gamma_multiply(alpha));
                painter.rect_stroke(badge, 3.0, (1.0, white(0.75 * alpha)), egui::StrokeKind::Inside);
                let bar = Rect::from_min_max(badge.min, pos2(badge.max.x, badge.min.y + badge.height() * 0.28));
                painter.rect_filled(bar.shrink(1.0), 2.0, white(0.55 * alpha));
            }
        }
    }
}

/// Text on the glass, with a soft dark shadow a point below so it reads over bright wallpapers.
fn shaded_text(
    painter: &egui::Painter,
    at: Pos2,
    align: Align2,
    text: impl ToString,
    font: egui::FontId,
    color: Color32,
) -> Rect {
    let text = text.to_string();
    let shade = Color32::from_black_alpha((110.0 * color.a() as f32 / 255.0) as u8);
    painter.text(at + vec2(0.0, 1.0), align, &text, font.clone(), shade);
    painter.text(at, align, text, font, color)
}

/// The longest a restored window's tile stays out (or a minimizing one's stays in) while the watch catches up. The
/// watch seeing the window ends it sooner; until then the tile holds still. At 1.5 s a slow app outlasted it, and
/// the tile went and came back: the "twice" after some minimizes and restores.
const RESTORED_FOR: std::time::Duration = std::time::Duration::from_secs(6);
/// How often a minimized window's picture is taken again. It hardly changes while it's down.
const WINDOW_PICTURE_EVERY: f32 = 5.0;

/// The space between the dock and a floating panel, and the longer one a neck spans.
const FLOAT_GAP: f32 = 4.0;
const NECK_GAP: f32 = 16.0;

/// Where a popup is drawn while it opens (`open` 0 to 1, a little past on the spring) toward `settled`. A floating
/// panel grows out of a point just above its icon; one with a neck rises out of the icon's top.
/// A menu divider's ink: the menu's own light text colour, faint, like the system's separators on dark glass. The
/// glass stays dark with light text in both appearances, so a dark line would vanish into it; in Light Mode it shows
/// brighter wallpapers through, so the line is a touch stronger there to read the same.
fn divider_ink(dark: bool) -> Color32 {
    white(if dark { 0.13 } else { 0.18 })
}

fn panel_now(settled: Rect, icon_x: f32, icon_top: f32, open: f32, float: bool) -> Rect {
    let k = open.clamp(0.0, 1.12);
    if float {
        let inset = 18.0_f32.min(settled.width() * 0.5);
        let anchor = pos2(icon_x.clamp(settled.left() + inset, settled.right() - inset), settled.bottom());
        let s = lerp(0.3, 1.0, open.clamp(0.0, 1.06));
        return Rect::from_min_max(anchor + (settled.min - anchor) * s, anchor + (settled.max - anchor) * s);
    }
    let center = pos2(lerp(icon_x, settled.center().x, k.min(1.0)), lerp(icon_top + 6.0, settled.center().y, k));
    Rect::from_center_size(center, vec2(lerp(26.0, settled.width(), k), lerp(14.0, settled.height(), k)))
}

/// The join between the dock and a popup: short and wide, melting into both with a smooth fillet, like the notch's
/// panel joining the island. It keeps its width as the popup grows and shrinks, so it never thins to a string.
fn neck(x: f32, popup: Rect, body_top: f32, tile: f32) -> Option<Blob> {
    let w = (tile * 0.65).min(popup.width());
    let neck = Rect::from_min_max(pos2(x - w * 0.5, popup.bottom() - 6.0), pos2(x + w * 0.5, body_top + 6.0));
    (neck.height() > 0.0 && w > 1.0).then(|| Blob::rect(neck, 4.0, 12.0))
}

fn is_finder(t: &Tile) -> bool {
    t.path == std::path::Path::new(tiles::FINDER)
}

fn icon_options() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: Some(egui::TextureFilter::Linear),
    }
}

fn name_of(path: &std::path::Path, kind: Kind) -> String {
    match kind {
        Kind::Trash => os::TRASH_NAME.into(),
        _ => os::display_name(path),
    }
}

pub(crate) fn ellipsize(painter: &egui::Painter, text: &str, width: f32, font: egui::FontId) -> String {
    let fits = |s: &str| painter.layout_no_wrap(s.to_owned(), font.clone(), PEARL).size().x <= width;
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let s: String = chars[..n].iter().collect::<String>() + "\u{2026}";
        if fits(&s) {
            return s;
        }
    }
    "\u{2026}".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 120.0;
    /// A snap is a sudden change in speed: the most the liquid's top may speed up or slow down in one frame.
    const SMOOTH: f32 = 0.8;

    fn dock() -> Dock {
        let config = Config { tile_size: 40.0, large_size: 49.0, magnification: true, ..Config::default() };
        let dir = std::env::temp_dir().join("notchbase-dock-hover-tests");
        let mut dock = Dock::new(config, dir, vec2(1000.0, WINDOW_HEIGHT));
        for n in 0..5 {
            let path = PathBuf::from(format!("/Applications/Test {n}.app"));
            dock.tiles.push(Tile::new(format!("t{n}"), Kind::App, path, format!("Test {n}"), false));
        }
        dock.filled = true;
        dock
    }

    fn minimized(id: u32) -> Minimized {
        let frame = Rect::from_min_size(pos2(100.0, 100.0), vec2(600.0, 400.0));
        Minimized { pid: 42, win: Win { id, title: "Notes".into(), frame, on_screen: false } }
    }

    fn notes_running() -> Vec<os::Running> {
        let path = Some(PathBuf::from("/Applications/Notes.app"));
        vec![os::Running { pid: 42, path, name: "Notes".into(), active: false, hidden: false }]
    }

    fn window_tiles(dock: &Dock) -> Vec<(String, bool)> {
        dock.tiles.iter().filter(|t| t.kind == Kind::Window).map(|t| (t.key.clone(), t.leaving)).collect()
    }

    #[test]
    fn a_minimized_windows_tile_comes_once_and_stays_while_the_watch_catches_up() {
        let mut d = dock();
        d.refresh(&notes_running());
        d.expect_minimized(minimized(9), None);
        assert_eq!(window_tiles(&d), vec![(tiles::window_key(9), false)], "its tile opens at once");
        // The watch hasn't seen it go down yet, once and again: the tile holds still.
        for _ in 0..3 {
            d.set_minimized(Vec::new());
            d.refresh(&notes_running());
            assert_eq!(window_tiles(&d), vec![(tiles::window_key(9), false)]);
        }
        // Then it does: the same tile, not a second one arriving.
        assert!(d.set_minimized(vec![minimized(9)]));
        d.refresh(&notes_running());
        assert_eq!(window_tiles(&d), vec![(tiles::window_key(9), false)]);
        assert!(d.expected.is_empty(), "the watch took over");
    }

    #[test]
    fn a_restored_windows_tile_goes_once_and_stays_gone_while_the_watch_catches_up() {
        let mut d = dock();
        d.refresh(&notes_running());
        d.set_minimized(vec![minimized(9)]);
        d.refresh(&notes_running());
        let key = tiles::window_key(9);
        let i = d.tile_index(&key).expect("its tile");
        d.activate(&d.tiles[i].key.clone());
        assert!(matches!(d.effects.as_slice(), [Effect::Restore { id: 9, .. }]), "one restore");
        let gone = |d: &Dock| d.tiles.iter().all(|t| t.key != key || t.leaving);
        assert!(gone(&d), "its tile goes at once");
        // The watch still lists it as down for a while: it doesn't come back.
        for _ in 0..3 {
            d.set_minimized(vec![minimized(9)]);
            d.refresh(&notes_running());
            assert!(gone(&d));
        }
        d.set_minimized(Vec::new());
        d.refresh(&notes_running());
        assert!(gone(&d) && d.restored.is_empty());
    }

    fn fake_windows(pid: i32) -> Vec<Win> {
        (0..2)
            .map(|n| Win {
                id: pid as u32 * 10 + n,
                title: format!("Window {n}"),
                frame: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 500.0)),
                on_screen: true,
            })
            .collect()
    }

    /// Every app running, each with two windows.
    fn previews_dock() -> Dock {
        let mut dock = dock();
        dock.list_windows = fake_windows;
        for (n, t) in dock.tiles.iter_mut().enumerate() {
            t.pid = Some(n as i32 + 1);
        }
        dock
    }

    fn run_for(dock: &mut Dock, ctx: &egui::Context, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            frame(dock, ctx);
        }
    }

    #[test]
    fn previews_open_after_a_short_rest_and_go_after_leaving() {
        let ctx = egui::Context::default();
        let mut dock = previews_dock();
        dock.set_pointer(Some(center_of(&dock, 2)));
        run_for(&mut dock, &ctx, 0.25);
        assert_eq!(dock.previews_key(), None, "not before the rest is over");
        run_for(&mut dock, &ctx, 0.2);
        assert_eq!(dock.previews_key(), Some("t2"));

        // Up through the gap into the panel: it stays, and its app's icon stays lifted.
        let r = dock.popup.as_ref().unwrap().rect;
        dock.set_pointer(Some(pos2(r.center().x, r.top() + 20.0)));
        run_for(&mut dock, &ctx, 0.6);
        assert_eq!(dock.previews_key(), Some("t2"));
        assert!(dock.tiles[2].hover.value > 0.9);

        dock.set_pointer(None);
        run_for(&mut dock, &ctx, 0.15);
        assert_eq!(dock.previews_key(), Some("t2"), "a moment's grace after leaving");
        run_for(&mut dock, &ctx, 0.2);
        assert_eq!(dock.previews_key(), None);
    }

    #[test]
    fn previews_glide_to_the_next_app_without_closing() {
        let ctx = egui::Context::default();
        let mut dock = previews_dock();
        dock.set_pointer(Some(center_of(&dock, 2)));
        run_for(&mut dock, &ctx, 0.5);
        let from = dock.popup.as_ref().unwrap().frame.as_ref().unwrap()[0].value;
        dock.set_pointer(Some(center_of(&dock, 3)));
        for _ in 0..(0.15 / DT) as usize {
            frame(&mut dock, &ctx);
            assert!(dock.previews_key().is_some(), "the panel never closes on the way");
        }
        assert_eq!(dock.previews_key(), Some("t3"));
        run_for(&mut dock, &ctx, 0.6);
        let to = dock.popup.as_ref().unwrap().frame.as_ref().unwrap()[0].value;
        assert!(to > from + 20.0, "the panel glided over: {from} -> {to}");
    }

    #[test]
    fn previews_wait_for_a_press_to_end_and_stay_off_once_clicked_away() {
        let ctx = egui::Context::default();
        let mut dock = previews_dock();
        dock.config.window_previews = false;
        dock.set_pointer(Some(center_of(&dock, 1)));
        run_for(&mut dock, &ctx, 0.6);
        assert_eq!(dock.previews_key(), None, "off in the config");

        dock.config.window_previews = true;
        dock.set_pointer(Some(center_of(&dock, 2)));
        run_for(&mut dock, &ctx, 0.5);
        assert_eq!(dock.previews_key(), Some("t2"));
        // Pressing the icon closes them and they don't come straight back. (Let go off the icon, so nothing opens.)
        dock.press(false);
        assert_eq!(dock.previews_key(), None);
        dock.set_pointer(Some(center_of(&dock, 2) - vec2(0.0, 200.0)));
        dock.release();
        dock.set_pointer(Some(center_of(&dock, 2)));
        run_for(&mut dock, &ctx, 0.8);
        assert_eq!(dock.previews_key(), None);
    }

    fn center_of(dock: &Dock, n: usize) -> Pos2 {
        let (placed, refs) = dock.place();
        let s = refs.iter().position(|r| *r == Some(n)).unwrap();
        pos2(placed[s].center(), dock.icon_bottom() - dock.tile_size() * 0.5)
    }

    /// The top of the liquid straight above `x`.
    fn liquid_top(blobs: &[Blob], x: f32, from: f32) -> f32 {
        let mut y = from - 60.0;
        while y < from + 10.0 && crate::liquid::distance(blobs, pos2(x, y)) > 0.0 {
            y += 0.02;
        }
        y
    }

    fn frame(dock: &mut Dock, ctx: &egui::Context) -> Vec<Blob> {
        dock.step(DT);
        let mut blobs = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| blobs = dock.compose(ui.painter()));
        output.textures_delta.clear();
        blobs
    }

    /// Follows a height frame by frame and fails on any snap.
    #[derive(Default)]
    struct Track {
        last: Option<f32>,
        speed: Option<f32>,
    }

    impl Track {
        fn see(&mut self, y: f32, what: &str) {
            if let Some(last) = self.last {
                let speed = y - last;
                if let Some(before) = self.speed {
                    assert!((speed - before).abs() < SMOOTH, "{what} snapped: {before} then {speed} a frame");
                }
                self.speed = Some(speed);
            }
            self.last = Some(y);
        }
    }

    #[test]
    fn letting_go_eases_back_without_a_snap() {
        let ctx = egui::Context::default();
        for leave_after in [0.06, 0.4] {
            let mut d = dock();
            let at = center_of(&d, 2);
            d.set_pointer(Some(at));
            for _ in 0..(leave_after / DT) as usize {
                frame(&mut d, &ctx);
            }
            d.set_pointer(None);
            let mut track = Track::default();
            for _ in 0..90 {
                let blobs = frame(&mut d, &ctx);
                assert!(d.tiles[2].hover.value >= -1e-3, "never dips below rest");
                track.see(liquid_top(&blobs, at.x, d.body_top()), "the rise");
            }
            assert!(d.tiles[2].hover.is_settled() && d.tiles[2].hover.value == 0.0);
            assert!((track.last.unwrap() - d.body_top()).abs() < 0.3, "back flat on the body");
        }
    }

    #[test]
    fn sweeping_across_hands_the_lift_over_smoothly() {
        let ctx = egui::Context::default();
        let mut d = dock();
        let (from, to) = (center_of(&d, 0), center_of(&d, 4));
        let watch = center_of(&d, 2).x;
        let mut track = Track::default();
        for step in 0..=90 {
            let x = from.x + (to.x - from.x) * (step as f32 / 60.0).min(1.0);
            d.set_pointer(Some(pos2(x, from.y)));
            let blobs = frame(&mut d, &ctx);
            track.see(liquid_top(&blobs, watch, d.body_top()), "the liquid over the middle icon");
        }
    }

    #[test]
    fn resting_on_the_line_between_two_icons_does_not_flicker() {
        let ctx = egui::Context::default();
        let mut d = dock();
        let line = (center_of(&d, 1).x + center_of(&d, 2).x) * 0.5;
        let y = center_of(&d, 1).y;
        let mut changes = 0;
        let mut last = None;
        for n in 0..120 {
            let wobble = if n % 2 == 0 { -0.7 } else { 0.7 };
            d.set_pointer(Some(pos2(line + wobble, y)));
            frame(&mut d, &ctx);
            if d.hovered != last {
                changes += 1;
                last = d.hovered.clone();
            }
        }
        assert!(changes <= 1, "it picks one icon and stays: {changes} changes");
    }

    #[test]
    fn the_launch_swell_rises_and_settles_smoothly() {
        let ctx = egui::Context::default();
        let mut d = dock();
        d.tiles[3].gulp.velocity = tiles::GULP_KICK;
        let x = center_of(&d, 3).x;
        let mut track = Track::default();
        let mut highest = 0.0f32;
        for _ in 0..150 {
            let blobs = frame(&mut d, &ctx);
            let y = liquid_top(&blobs, x, d.body_top());
            highest = highest.max(d.body_top() - y);
            track.see(y, "the swell");
        }
        assert!((2.0..10.0).contains(&highest), "a swell, like the notch's: {highest}");
        assert!((track.last.unwrap() - d.body_top()).abs() < 0.3, "and it settles flat");
    }

    /// A dock built from a config like a real one: Finder, three pinned apps, then Downloads and the Trash.
    fn pinned_dock(name: &str) -> Dock {
        let pinned = ["Calculator", "Chess", "Stickies"]
            .iter()
            .map(|n| PathBuf::from(format!("/System/Applications/{n}.app")))
            .collect();
        let config = Config { tile_size: 40.0, large_size: 49.0, pinned, ..Config::default() };
        let dir = std::env::temp_dir().join(format!("notchbase-dock-drop-tests-{}-{name}", std::process::id()));
        let mut dock = Dock::new(config, dir, vec2(1000.0, WINDOW_HEIGHT));
        dock.refresh(&[]);
        dock
    }

    fn total_width(dock: &Dock) -> f32 {
        dock.place().0.iter().map(|p| p.width).sum()
    }

    #[test]
    fn the_smallest_dock_keeps_its_icons_and_dots_inside_the_body() {
        let mut d = pinned_dock("smallest");
        d.config.tile_size = SMALLEST;
        d.config = std::mem::take(&mut d.config).sane();
        assert_eq!(d.config.tile_size, 16.0, "the config takes 16 point icons as they are");
        d.step(DT);
        let body = Rect::from_min_max(pos2(0.0, d.body_top()), pos2(d.size.x, d.bottom()));
        for i in 0..d.tiles.len() {
            let icon = d.icon_rect(i);
            assert!(body.contains_rect(icon), "icon {i} sits inside the body at rest");
        }
        let dot_y = (d.icon_bottom() - SMALLEST * ART_INSET + d.bottom()) * 0.5;
        assert!(dot_y > d.icon_bottom() - SMALLEST * ART_INSET && dot_y < d.bottom(), "the dots sit below the art");
        assert_eq!(d.hover_lift(), HOVER_LIFT * 0.5, "a 16 point icon lifts half as far");
        d.config.tile_size = 48.0;
        assert_eq!(d.hover_lift(), HOVER_LIFT, "standard icons lift as before");
    }

    #[test]
    fn dragging_the_separator_resizes_the_icons_and_saves_on_letting_go() {
        let mut d = pinned_dock("resize");
        let (placed, refs) = d.place();
        let sep = placed.iter().zip(&refs).find(|(s, r)| r.is_none() && !s.gap).map(|(s, _)| s.center()).unwrap();
        let y = d.body_top() + 10.0;
        d.set_pointer(Some(pos2(sep + 3.0, y)));
        d.step(DT);
        assert!(d.over_separator, "the resize cursor shows over it");
        d.press(false);
        d.set_pointer(Some(pos2(sep, y - 20.0)));
        d.step(DT);
        assert_eq!(d.config.tile_size, 60.0, "20 points up is 20 points bigger");
        d.set_pointer(Some(pos2(sep, y - 400.0)));
        d.step(DT);
        assert_eq!(d.config.tile_size, LARGEST, "never past the largest");
        d.set_pointer(Some(pos2(sep, y + 400.0)));
        d.step(DT);
        assert_eq!(d.config.tile_size, SMALLEST, "nor below the smallest");
        d.set_pointer(Some(pos2(sep, y)));
        d.step(DT);
        d.release();
        assert_eq!(d.config.tile_size, 40.0);
        let saved = Config::load_or_seed(&d.dir, || None);
        assert_eq!(saved.tile_size, 40.0, "saved when let go");

        // A right-click there opens its own menu, over the separator.
        d.set_pointer(Some(pos2(sep, y)));
        d.press(true);
        assert!(d.popup.as_ref().is_some_and(|p| p.key == SEPARATOR_KEY && !p.shut));
    }

    #[test]
    fn an_app_dragged_over_opens_a_gap_among_the_pinned_ones() {
        let ctx = egui::Context::default();
        let mut d = pinned_dock("gap");
        run_for(&mut d, &ctx, 0.5);
        let rest = total_width(&d);
        let first = d.tiles.iter().position(|t| t.pinned && !is_finder(t)).unwrap();
        let x = (center_of(&d, first).x + center_of(&d, first + 1).x) * 0.5;
        d.set_pointer(Some(pos2(x, center_of(&d, first).y)));
        d.hover_file(PathBuf::from("/System/Applications/TextEdit.app"));
        let mut track = Track::default();
        for _ in 0..60 {
            frame(&mut d, &ctx);
            track.see(total_width(&d), "the dock's width");
        }
        assert_eq!(d.incoming.as_ref().and_then(|g| g.before), Some(first + 1), "between the first two pinned apps");
        assert!((total_width(&d) - rest - d.slot_width()).abs() < 0.5, "one slot wider");
        // Dragged on past the end and away again: the gap closes and goes.
        d.set_pointer(Some(pos2(980.0, center_of(&d, first).y)));
        run_for(&mut d, &ctx, 0.3);
        assert!(d.incoming.as_ref().is_some_and(|g| g.open.target == 0.0), "no gap over the folders");
        d.hover_files(false);
        run_for(&mut d, &ctx, 1.0);
        assert!(d.incoming.is_none());
        assert!((total_width(&d) - rest).abs() < 0.5);
    }

    #[test]
    fn a_dropped_app_takes_the_gap_and_hops_in_without_a_jump() {
        let ctx = egui::Context::default();
        let mut d = pinned_dock("land");
        run_for(&mut d, &ctx, 0.5);
        let first = d.tiles.iter().position(|t| t.pinned && !is_finder(t)).unwrap();
        let x = (center_of(&d, first).x + center_of(&d, first + 1).x) * 0.5;
        d.set_pointer(Some(pos2(x, center_of(&d, first).y)));
        let new = PathBuf::from("/System/Applications/TextEdit.app");
        d.hover_file(new.clone());
        run_for(&mut d, &ctx, 0.15);
        d.drop_file(new.clone());
        let mut track = Track::default();
        let mut highest = 0.0f32;
        for _ in 0..150 {
            frame(&mut d, &ctx);
            track.see(total_width(&d), "the dock's width");
            if let Some(t) = d.tiles.iter().find(|t| t.path == new) {
                highest = highest.max(t.fall.value);
            }
        }
        assert_eq!(d.config.pinned[1], new, "kept where it was dropped");
        let t = d.tiles.iter().find(|t| t.path == new).expect("it's in the dock");
        assert!(t.pinned && t.presence.is_settled() && t.fall.is_settled());
        assert!((6.0..30.0).contains(&highest), "a hop, not a leap: {highest}");
        assert!(d.incoming.is_none());
    }

    #[test]
    fn the_mission_control_cover_is_one_move_in_and_out() {
        let mut d = dock();
        for away in [true, false] {
            d.set_away(away);
            let mut last = d.cover_state();
            let mut frames = 0;
            while !d.away.is_settled() || frames == 0 {
                d.step(DT);
                let now = d.cover_state();
                let dir = if away { 1.0 } else { -1.0 };
                for (a, b, what) in [(last.0, now.0, "progress"), (last.1, now.1, "baseline"), (last.3, now.3, "tint")]
                {
                    assert!((b - a) * dir >= -1e-4, "{what} never turns back: {a} then {b}");
                }
                last = now;
                frames += 1;
                assert!(frames < 120, "it lands within a second");
            }
            let end = if away { 1.0 } else { 0.0 };
            assert!((last.0 - end).abs() < 1e-3, "it lands: {}", last.0);
            assert!(frames as f32 * DT < 0.75, "one quick move: {} s", frames as f32 * DT);
        }
    }

    #[test]
    fn with_the_desktop_picture_ready_mission_control_keeps_the_glass_look() {
        for ready in [true, false] {
            let mut d = dock();
            d.fake_ready = ready;
            let rest = d.glass_band().tint;
            d.set_away(true);
            for _ in 0..120 {
                d.step(DT);
            }
            if ready {
                assert_eq!(d.glass_band().tint, rest, "the same tint as on the desktop, not black");
                assert!((d.fake_glass() - 1.0).abs() < 1e-3, "the picture stands in fully");
                assert!((d.blur_opacity() - 1.0).abs() < 1e-3, "and the blur stays under it");
            } else {
                assert!(d.glass_band().tint[0] > rest[0], "without a picture it darkens, as before");
                assert_eq!(d.fake_glass(), 0.0);
            }
        }
    }

    #[test]
    fn dividers_are_light_in_both_appearances_and_a_little_stronger_in_light() {
        let (dark, light) = (divider_ink(true), divider_ink(false));
        assert!(dark.a() > 0 && light.a() > dark.a(), "{dark:?} {light:?}");
    }

    #[test]
    fn a_removed_icon_bursts_and_its_slot_closes_smoothly() {
        let ctx = egui::Context::default();
        let mut d = dock();
        run_for(&mut d, &ctx, 0.3);
        let rest = total_width(&d);
        let at = d.icon_rect(2);
        d.burst(2, at.center(), at.width());
        let mut last = rest;
        let mut widest_drop = 0.0f32;
        for n in 0..90 {
            let blobs = frame(&mut d, &ctx);
            let width = total_width(&d);
            assert!((width - last).abs() < 3.0, "the dock's width jumped: {last} to {width}");
            last = width;
            if n as f32 * DT < BURST_SWELL - DT {
                assert!((total_width(&d) - rest).abs() < 0.01, "the slot holds while it swells");
            }
            for b in blobs.iter().filter(|b| b.half.x < 7.0) {
                widest_drop = widest_drop.max((b.center - at.center()).length());
            }
        }
        assert!(d.bursts.is_empty(), "the burst is over");
        assert_eq!(d.tiles.len(), 4, "and the tile is gone");
        assert!((rest - total_width(&d) - d.slot_width()).abs() < 0.5, "one slot narrower");
        // From the bubble, which rises about an icon radius out of the dock first.
        assert!((10.0..70.0).contains(&widest_drop), "droplets fly a short way: {widest_drop}");
    }
}
