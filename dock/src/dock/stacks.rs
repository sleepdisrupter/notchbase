//! Folders as stacks, as the Dock shows them: the tile is a little pile of what's newest (the folder alone when it's
//! empty), a file arriving drops onto the pile, and a click opens a fan of a few items or a scrolling grid of many,
//! grown out of the tile in the dock's glass. Items open on a click, drag out as real files (several at once when
//! several are picked), and right-click for Open, Show in Finder and Move to Trash.
//!
//! It stays quiet however full the folder gets: icons and names, and nothing else until it's needed. A search pill
//! shows only in a big folder (or once someone types), date headings only where a big folder spans several days,
//! an item's size and date only while the pointer rests on it, and a scroll indicator only while it scrolls. The
//! keyboard works the whole time without showing anything: arrows, Return, Space for Quick Look, Cmd+Delete,
//! Cmd+A and Esc.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use egui::emath::Rot2;
use egui::{Align2, Color32, Mesh, Pos2, Rect, Shape, Stroke, TextureHandle, Vec2, pos2, vec2};
use notchbase_paint::color::{PEARL, white};
use notchbase_paint::font;
use notchbase_paint::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};

use super::{
    Blob, DRAG_START, Dock, Effect, FLOAT_GAP, Kind, NECK_GAP, OOZE, OOZE_SHUT, PAD_TOP, PLATE, Placed, Popup,
    PopupKind, divider_ink, ellipsize, icon_options, neck, panel_now, shaded_text,
};
use crate::config::StackSort;
use crate::os;
use crate::stack::{self, Item, Key, Listing, Shape as Layout};

/// The "Open in Finder" row or link, after the items.
pub(super) const FINDER: usize = usize::MAX;

/// A file landing on the pile: it drops in from above with a little bounce, like an app dropped into the dock.
const DROP_IN: SpringConfig = SpringConfig::new(0.42, 0.62);
/// The landing nudges the tile's liquid up, a softer version of the launch swell.
const DROP_KICK: f32 = 95.0;
/// The fan's rows, each rising out of the tile on its own spring with a soft settle.
const FAN_ROW: SpringConfig = SpringConfig::new(0.34, 0.74);
/// How long after the row below each row of the fan starts, opening and (twice as fast) closing.
const FAN_STAGGER: f32 = 0.028;
/// A fan row's icon swelling under the pointer.
const LIFT: SpringConfig = SpringConfig::new(0.2, 0.82);
/// The grid's rows fade up one after another, at most this long after the first.
const LINE_STAGGER: f32 = 0.028;
const LINE_STAGGER_MOST: f32 = 0.14;
const CELL_IN: f32 = 0.26;
/// A wheel's clicks glide; the trackpad is followed exactly, its own momentum carrying it.
const SCROLL: SpringConfig = SpringConfig::new(0.1, 1.0);
/// Pulled past either end, the grid springs back like this once let go.
const BOUNCE: SpringConfig = SpringConfig::new(0.3, 1.0);
/// How far past either end it can be pulled, and how much less than the finger it moves there.
const MOST_OVERSCROLL: f32 = 90.0;
const OVERSCROLL_GIVE: f32 = 0.35;
/// The scroll indicator shows while scrolling and fades this long after.
const INDICATOR_STAYS: f32 = 0.7;
/// A picture coming in fades over this long, so nothing pops.
const PICTURE_FADE: f32 = 0.16;
/// Pictures kept as textures at most; past it, the ones seen longest ago go, down to `KEEP_PICTURES`.
const MOST_PICTURES: usize = 400;
const KEEP_PICTURES: usize = 300;
/// Typing in the fan picks the first name that matches what's typed within this long.
const TYPE_SELECT: f32 = 1.2;

// The fan.
const FAN_PITCH: f32 = 44.0;
const FAN_ICON: f32 = 38.0;
/// "Open in Finder" on top of the fan: a smaller row of its own.
const FINDER_PITCH: f32 = 36.0;
const FINDER_ICON: f32 = 26.0;
const FAN_LABEL: f32 = 24.0;
const FAN_TEXT: f32 = 220.0;
const FAN_GAP: f32 = 10.0;

// The grid.
const CELL: Vec2 = vec2(96.0, 92.0);
const CELL_ICON: f32 = 56.0;
const SIDE: f32 = 16.0;
const HEADER: f32 = 48.0;
const FOOTER: f32 = 40.0;
const HEADING: f32 = 30.0;
const EMPTY_HEIGHT: f32 = 156.0;
const RADIUS: f32 = 22.0;
const MOST_HEIGHT: f32 = 620.0;
const SEARCH: Vec2 = vec2(150.0, 26.0);
/// Cells fade out over this much at a scrolling edge.
const EDGE_FADE: f32 = 16.0;
/// The search pill shows past this many items (or once someone types), and date headings past this many.
const SEARCH_FROM: usize = 20;
const HEADINGS_FROM: usize = 20;

// The pile on the tile, the top first: offset (in tile widths), turn (degrees) and size.
const PILE: [(f32, f32, f32, f32); 3] = [(0.0, 0.0, 0.0, 1.0), (-0.07, -0.04, -7.0, 0.93), (0.075, -0.07, 8.0, 0.88)];

// The right-click card on an item.
const CARD_WIDTH: f32 = 172.0;
const CARD_ROW: f32 = 28.0;

const DAY: Duration = Duration::from_secs(86_400);
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// One folder's stack: what's in it, in the order it's shown, and the last file to land on it.
pub(super) struct StackState {
    listing: Option<Listing>,
    pub(super) items: Vec<Item>,
    pub(super) partial: usize,
    /// Goes up whenever `items` changes, so an open grid knows to lay itself out again.
    generation: u64,
    arrive: Spring,
    arrived: Option<PathBuf>,
}

impl StackState {
    fn new() -> Self {
        Self {
            listing: None,
            items: Vec::new(),
            partial: 0,
            generation: 0,
            arrive: Spring::new(1.0, DROP_IN, UNIT_PRECISION),
            arrived: None,
        }
    }

    /// How far along the newest arrival's drop is, while it's dropping.
    fn arriving(&self) -> Option<(&Path, f32)> {
        let path = self.arrived.as_deref()?;
        (!self.arrive.is_settled()).then_some((path, self.arrive.value))
    }
}

/// Pictures of the files in stacks, made off the main thread and kept as textures.
#[derive(Default)]
pub(super) struct FileIcons {
    ready: HashMap<PathBuf, FileIcon>,
    asked: HashSet<PathBuf>,
    wants: Vec<PathBuf>,
    came: Vec<stack::Picture>,
    /// What an open grid has near the screen, for the picture thread to keep to (the rest it drops), when it
    /// changed; and what it was last.
    keep: Option<HashSet<PathBuf>>,
    kept: HashSet<PathBuf>,
}

struct FileIcon {
    tex: TextureHandle,
    thumb: bool,
    /// When it came in, in the dock's time, for its fade.
    since: f32,
    /// When it was last drawn, so the ones seen longest ago go first.
    used: f32,
}

/// An open stack.
pub(super) struct StackPopup {
    folder: PathBuf,
    layout: Layout,
    /// The fan's rows, the bottom one first, then "Open in Finder".
    rows: Vec<Spring>,
    finder: Spring,
    /// The fan's icons swelling under the pointer, the same rows and "Open in Finder" last.
    lift: Vec<Spring>,
    /// Seconds since it opened, and since it started closing.
    age: f32,
    shut_age: f32,
    scroll: Spring,
    scroll_most: f32,
    /// Seconds since it last scrolled, for the indicator and the spring back from past an end.
    since_scroll: f32,
    /// Rows in the fan when it opened: "Open in Finder" rises after them.
    fanned: usize,
    /// Rows the fan drew last (its files, or its one "nothing here"), for folding them away.
    fan_rows: usize,
    menu: Option<EntryMenu>,
    /// What's been typed: the grid's filter, or the fan's type-to-select.
    query: String,
    typed_at: f32,
    /// The item the keyboard is on, the end a Shift-click extends from, and everything picked.
    cursor: Option<PathBuf>,
    anchor: Option<PathBuf>,
    selected: HashSet<PathBuf>,
    /// The cursor moved by keys: it shows a ring until the pointer takes over.
    keyed: bool,
    /// The first match is picked once the filter changes.
    pick_first: bool,
    model: Option<Model>,
    /// Names shortened to fit under their icons, worked out once.
    short: HashMap<PathBuf, String>,
    /// Where the cells scroll, and each item's place as last drawn (the item's index, its rect).
    area: Rect,
    cells: Vec<(usize, Rect)>,
    /// The search pill is showing.
    searching: bool,
}

impl StackPopup {
    pub(super) fn settled_shut(&self) -> bool {
        self.rows.iter().chain([&self.finder]).all(|r| r.value < 0.02)
    }
}

/// The grid laid out: what's shown, in order (indices into the stack's items), and its lines from the top.
struct Model {
    key: ModelKey,
    shown: Vec<usize>,
    lines: Vec<(f32, Line)>,
    height: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct ModelKey {
    query: String,
    generation: u64,
    cols: usize,
    by: StackSort,
    midnight: SystemTime,
}

/// A line of the grid: a date heading, or a row of cells (positions in `shown`).
#[derive(Clone, Debug, PartialEq)]
enum Line {
    Heading(&'static str),
    Cells(Range<usize>),
}

/// The right-click card on one item.
struct EntryMenu {
    path: PathBuf,
    at: Pos2,
    open: Spring,
    hover: Option<usize>,
    rect: Rect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryAction {
    Open,
    Reveal,
    Trash,
}

const CARD_ITEMS: [Option<(&str, EntryAction)>; 4] = [
    Some(("Open", EntryAction::Open)),
    Some((os::SHOW_IN_FILES, EntryAction::Reveal)),
    None,
    Some((os::MOVE_TO_TRASH, EntryAction::Trash)),
];

fn card_height() -> f32 {
    CARD_ITEMS.iter().map(|i| if i.is_some() { CARD_ROW } else { 9.0 }).sum::<f32>() + 12.0
}

fn is_downloads(folder: &Path) -> bool {
    folder.file_name().is_some_and(|n| n == "Downloads")
}

/// Where an open stack sits this frame.
struct At {
    float: bool,
    /// The tile's centre x, its icon's top and size, and the dock's top.
    cx: f32,
    icon_top: f32,
    tile: f32,
    body_top: f32,
    /// The bottom of the space above the dock the stack opens into.
    base: f32,
    tile_icon: Option<TextureHandle>,
    name: String,
    folder: PathBuf,
    generation: u64,
    arriving: Option<(PathBuf, f32)>,
}

impl Dock {
    /// The folders the dock shows as stacks, to watch.
    pub fn stack_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> =
            self.tiles.iter().filter(|t| t.kind == Kind::Folder && !t.leaving).map(|t| t.path.clone()).collect();
        folders.dedup();
        folders
    }

    /// A folder as it is now. A file that wasn't there before drops onto the pile (never on the first reading,
    /// which is just the folder as it was).
    pub fn set_listing(&mut self, folder: PathBuf, listing: Listing) {
        let by = self.config.stack_sort;
        let state = self.stacks.entry(folder.clone()).or_insert_with(StackState::new);
        if state.listing.as_ref() == Some(&listing) {
            return;
        }
        let came = state.listing.as_ref().map(|before| stack::arrivals(before, &listing)).unwrap_or_default();
        let mut items = listing.items.clone();
        stack::sort(&mut items, by);
        // A file changed in place is pictured again.
        let before: HashMap<&Path, SystemTime> = state.items.iter().map(|i| (i.path.as_path(), i.modified)).collect();
        for item in &items {
            if before.get(item.path.as_path()).is_some_and(|m| *m != item.modified) {
                self.files.ready.remove(&item.path);
                self.files.asked.remove(&item.path);
            }
        }
        state.partial = listing.partial;
        state.listing = Some(listing);
        if let Some(top) = items.iter().find(|i| came.contains(&i.path)) {
            state.arrived = Some(top.path.clone());
            state.arrive.snap_to(0.0);
            state.arrive.animate_to(1.0, DROP_IN);
            if let Some(t) = self.tiles.iter_mut().find(|t| t.kind == Kind::Folder && t.path == folder) {
                t.gulp.velocity = t.gulp.velocity.max(DROP_KICK);
                t.gulp.animate_to(0.0, crate::tiles::SWELL);
            }
        }
        state.items = items;
        state.generation += 1;
        // The pile's pictures are wanted at once, before anyone looks.
        let top: Vec<PathBuf> = state.items.iter().take(PILE.len()).map(|i| i.path.clone()).collect();
        for path in top {
            self.want_picture(&path);
        }
        // Pictures of files that went are let go.
        let kept: HashSet<&Path> =
            self.stacks.values().flat_map(|s| s.items.iter().map(|i| i.path.as_path())).collect();
        self.files.ready.retain(|p, _| kept.contains(p.as_path()));
        self.files.asked.retain(|p| kept.contains(p.as_path()));
    }

    /// The sort order changed: every stack is put in the new order.
    pub(super) fn resort_stacks(&mut self) {
        let by = self.config.stack_sort;
        for state in self.stacks.values_mut() {
            stack::sort(&mut state.items, by);
            state.generation += 1;
        }
    }

    /// Files whose pictures are wanted and not yet asked for, for the picture thread.
    pub fn take_picture_wants(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.files.wants)
    }

    /// What the picture thread should keep to, when it changed: pictures of anything else it drops unmade.
    pub fn take_picture_keep(&mut self) -> Option<HashSet<PathBuf>> {
        self.files.keep.take()
    }

    pub fn give_pictures(&mut self, pictures: Vec<stack::Picture>) {
        self.files.came.extend(pictures);
    }

    fn want_picture(&mut self, path: &Path) {
        if !self.files.ready.contains_key(path) && self.files.asked.insert(path.to_path_buf()) {
            self.files.wants.push(path.to_path_buf());
        }
    }

    /// The tops of every pile: their pictures always stay.
    fn piles(&self) -> HashSet<PathBuf> {
        self.stacks.values().flat_map(|s| s.items.iter().take(PILE.len()).map(|i| i.path.clone())).collect()
    }

    /// Pictures that came in become textures, on the next frame. Past `MOST_PICTURES`, the ones seen longest ago
    /// are let go (and made again if they're scrolled back to).
    pub(super) fn upload_pictures(&mut self, ctx: &egui::Context) {
        for p in std::mem::take(&mut self.files.came) {
            let tex = ctx.load_texture(format!("stack:{}", p.path.display()), p.image, icon_options());
            self.files.ready.insert(p.path, FileIcon { tex, thumb: p.thumb, since: self.time, used: self.time });
        }
        self.forget_old_pictures();
    }

    fn forget_old_pictures(&mut self) {
        if self.files.ready.len() <= MOST_PICTURES {
            return;
        }
        let piles = self.piles();
        let mut old: Vec<(f32, PathBuf)> =
            self.files.ready.iter().filter(|(p, _)| !piles.contains(*p)).map(|(p, f)| (f.used, p.clone())).collect();
        old.sort_by(|a, b| a.0.total_cmp(&b.0));
        let extra = self.files.ready.len().saturating_sub(KEEP_PICTURES);
        for (_, path) in old.into_iter().take(extra) {
            self.files.ready.remove(&path);
            self.files.asked.remove(&path);
        }
    }

    /// A file's picture and how far it has faded in, asking for it when it isn't there yet.
    fn picture(&mut self, path: &Path) -> Option<(TextureHandle, bool, f32)> {
        let time = self.time;
        match self.files.ready.get_mut(path) {
            Some(f) => {
                f.used = time;
                Some((f.tex.clone(), f.thumb, smoothstep(0.0, PICTURE_FADE, time - f.since)))
            }
            None => {
                self.want_picture(path);
                None
            }
        }
    }

    /// The grid has `near` on or near the screen: pictures asked for anything else (scrolled past) are dropped
    /// unmade, and asked for again if they come back.
    fn keep_pictures(&mut self, near: HashSet<PathBuf>) {
        let mut keep = near;
        keep.extend(self.piles());
        if keep == self.files.kept {
            return;
        }
        let ready = &self.files.ready;
        self.files.asked.retain(|p| keep.contains(p) || ready.contains_key(p));
        self.files.wants.retain(|p| keep.contains(p));
        self.files.kept.clone_from(&keep);
        self.files.keep = Some(keep);
    }

    /// Whether any stack is moving on its own: a file dropping onto a pile, or a download in progress.
    pub(super) fn stacks_animating(&self) -> bool {
        self.stacks.values().any(|s| !s.arrive.is_settled())
            || self.stack_folders().iter().any(|f| self.stacks.get(f).is_some_and(|s| s.partial > 0))
            || !self.files.came.is_empty()
    }

    /// Dragging an item out of an open stack: the files to drag (everything picked, when it's one of them), once the
    /// press has moved far enough. The app starts the system drag, inside the mouse event when it can.
    pub fn take_file_drag(&mut self) -> Option<Vec<PathBuf>> {
        let (Some(press), Some(at)) = (&mut self.press, self.pointer) else { return None };
        if press.handled || (at - press.at).length() <= DRAG_START {
            return None;
        }
        let path = press.entry.clone()?;
        press.handled = true;
        self.dragging_out = true;
        Some(match &self.popup {
            Some(Popup { kind: PopupKind::Stack(s), .. }) if s.selected.len() > 1 && s.selected.contains(&path) => {
                self.in_order(&s.folder, &s.selected)
            }
            _ => vec![path],
        })
    }

    /// The system drag ended (dropped, or let go): the press it came from is over.
    pub fn file_drag_ended(&mut self) {
        self.dragging_out = false;
        if self.press.as_ref().is_some_and(|p| p.handled) {
            self.press = None;
        }
    }

    pub(super) fn step_stacks(&mut self, dt: f32) {
        for s in self.stacks.values_mut() {
            s.arrive.step(dt);
        }
        if let Some(paths) = self.take_file_drag() {
            self.effects.push(Effect::DragFiles(paths));
        }
        // The stack stays up while the files are dragged over it, and folds away once they leave it.
        if self.dragging_out {
            let over =
                self.pointer.zip(self.popup.as_ref()).is_some_and(|(at, p)| !p.shut && p.rect.expand(6.0).contains(at));
            if !over {
                self.dragging_out = false;
                self.close_popup();
            }
        }
        let Some(Popup { kind: PopupKind::Stack(s), shut, .. }) = &mut self.popup else { return };
        let shut = *shut;
        s.age += dt;
        s.since_scroll += dt;
        if shut {
            s.shut_age += dt;
        }
        // Folding, the fan's rows go top first, starting with the top one shown: rows past what's shown, and every
        // row of a grid (which has none), are already down, so nothing waits on them.
        let top = if s.layout == Layout::Fan { s.fan_rows.min(s.rows.len()) } else { 0 };
        for (k, row) in s.rows.iter_mut().enumerate() {
            if shut {
                if k >= top {
                    row.snap_to(0.0);
                } else if s.shut_age >= (top - 1 - k) as f32 * FAN_STAGGER * 0.5 {
                    row.animate_to(0.0, OOZE_SHUT);
                }
            } else if s.age >= k as f32 * FAN_STAGGER {
                row.animate_to(1.0, FAN_ROW);
            }
            row.step(dt);
        }
        if shut {
            s.finder.animate_to(0.0, OOZE_SHUT);
        } else if s.age >= s.fanned as f32 * FAN_STAGGER {
            s.finder.animate_to(1.0, FAN_ROW);
        }
        s.finder.step(dt);
        for l in &mut s.lift {
            l.step(dt);
        }
        // Let go past an end, it springs back.
        let inside = s.scroll.target.clamp(0.0, s.scroll_most);
        if s.since_scroll > 0.06 && (s.scroll.target - inside).abs() > 0.01 {
            s.scroll.animate_to(inside, BOUNCE);
        }
        s.scroll.step(dt);
        if let Some(m) = &mut s.menu {
            m.open.step(dt);
        }
    }

    /// `paths` in `folder`'s stack, in the order it shows them.
    fn in_order(&self, folder: &Path, paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
        let Some(state) = self.stacks.get(folder) else { return paths.iter().cloned().collect() };
        state.items.iter().filter(|i| paths.contains(&i.path)).map(|i| i.path.clone()).collect()
    }

    pub(super) fn open_stack(&mut self, key: String) {
        let Some(i) = self.tile_index(&key) else { return };
        let folder = self.tiles[i].path.clone();
        // Normally the watch has read it already; if not, it's read now, once.
        if self.stacks.get(&folder).is_none_or(|s| s.listing.is_none())
            && let Some(listing) = stack::list(&folder)
        {
            self.set_listing(folder.clone(), listing);
        }
        let count = self.stacks.get(&folder).map_or(0, |s| s.items.len());
        // A fan rises up out of a dock along the bottom; beside a side dock a folder opens as a grid, as the Dock's
        // own stacks do there.
        let layout = if self.side().vertical() { Layout::Grid } else { stack::shape(self.config.stack_view, count) };
        let fanned = count.min(stack::FAN_MOST);
        let paths: Vec<PathBuf> = self
            .stacks
            .get(&folder)
            .map(|s| s.items.iter().take(fanned.max(21)).map(|i| i.path.clone()).collect())
            .unwrap_or_default();
        for p in &paths {
            self.want_picture(p);
        }
        let springs = |n: usize, config| (0..n).map(|_| Spring::new(0.0, config, UNIT_PRECISION)).collect();
        self.show_popup(
            key,
            PopupKind::Stack(Box::new(StackPopup {
                folder,
                layout,
                rows: springs(stack::FAN_MOST, FAN_ROW),
                finder: Spring::new(0.0, FAN_ROW, UNIT_PRECISION),
                lift: springs(stack::FAN_MOST + 1, LIFT),
                age: 0.0,
                shut_age: 0.0,
                scroll: Spring::new(0.0, SCROLL, POINT_PRECISION),
                scroll_most: 0.0,
                since_scroll: 10.0,
                fanned,
                fan_rows: 0,
                menu: None,
                query: String::new(),
                typed_at: 0.0,
                cursor: None,
                anchor: None,
                selected: HashSet::new(),
                keyed: false,
                pick_first: false,
                model: None,
                short: HashMap::new(),
                area: Rect::NOTHING,
                cells: Vec::new(),
                searching: false,
            })),
        );
    }

    /// Whether an open stack wants the keyboard: arrows, typing, Return, Space, Esc.
    pub fn stack_wants_keys(&self) -> bool {
        matches!(&self.popup, Some(Popup { kind: PopupKind::Stack(_), shut: false, .. }))
    }

    /// A press while a stack is open. True when the stack took it.
    pub(super) fn stack_press(&mut self, at: Pos2, right: bool) -> bool {
        let (command, shift) = (self.mods.command, self.mods.shift);
        let Some(popup) = self.popup.as_mut().filter(|p| !p.shut) else { return false };
        let hover = popup.hover;
        let PopupKind::Stack(s) = &mut popup.kind else { return false };
        let now = std::time::Instant::now();
        if let Some(menu) = &s.menu {
            let inside = menu.rect.contains(at);
            if !inside {
                s.menu = None;
            }
            self.press = Some(super::Press { at, when: now, tile: None, entry: None, right, handled: !inside });
            return true;
        }
        if !popup.rect.expand(4.0).contains(at) {
            return false;
        }
        let items = self.stacks.get(&s.folder).map(|st| st.items.as_slice()).unwrap_or_default();
        let entry = hover.filter(|&k| k != FINDER).and_then(|k| items.get(k)).map(|i| i.path.clone());
        s.keyed = false;
        if right {
            if let Some(path) = entry {
                let mut open = Spring::new(0.0, OOZE, UNIT_PRECISION);
                open.animate_to(1.0, OOZE);
                s.menu = Some(EntryMenu { path, at, open, hover: None, rect: Rect::NOTHING });
            }
            self.press = Some(super::Press { at, when: now, tile: None, entry: None, right, handled: true });
            return true;
        }
        // Cmd picks one more (or one less), Shift everything from the last one picked; neither opens anything.
        if let Some(path) = entry.clone()
            && (command || shift)
        {
            if shift && let Some(from) = s.anchor.clone() {
                let order: Vec<&Path> = match &s.model {
                    Some(m) if s.layout == Layout::Grid => m.shown.iter().map(|&i| items[i].path.as_path()).collect(),
                    _ => items.iter().map(|i| i.path.as_path()).collect(),
                };
                let (a, b) = (order.iter().position(|p| *p == from), order.iter().position(|p| *p == path));
                if let (Some(a), Some(b)) = (a, b) {
                    s.selected = order[a.min(b)..=a.max(b)].iter().map(|p| p.to_path_buf()).collect();
                }
            } else {
                if !s.selected.remove(&path) {
                    s.selected.insert(path.clone());
                }
                s.anchor = Some(path.clone());
            }
            s.cursor = Some(path);
            self.press = Some(super::Press { at, when: now, tile: None, entry: None, right, handled: true });
            return true;
        }
        if let Some(path) = &entry
            && !s.selected.contains(path)
        {
            s.selected.clear();
        }
        if entry.is_some() {
            s.anchor.clone_from(&entry);
            s.cursor.clone_from(&entry);
        }
        self.press = Some(super::Press { at, when: now, tile: None, entry, right, handled: false });
        true
    }

    /// A release while a stack's right-click card is up. True when the card took it.
    pub(super) fn stack_release(&mut self) -> bool {
        let Some(Popup { kind: PopupKind::Stack(s), shut: false, .. }) = &self.popup else { return false };
        let Some(menu) = &s.menu else { return false };
        let Some(action) = menu.hover.and_then(|n| CARD_ITEMS[n]).map(|(_, a)| a) else { return true };
        // The card on one of several picked acts on all of them.
        let paths = if s.selected.len() > 1 && s.selected.contains(&menu.path) {
            self.in_order(&s.folder, &s.selected)
        } else {
            vec![menu.path.clone()]
        };
        match action {
            EntryAction::Open if self.live => paths.iter().for_each(|p| os::open(p)),
            EntryAction::Reveal if self.live => paths.iter().for_each(|p| os::reveal(p)),
            EntryAction::Open | EntryAction::Reveal => {}
            EntryAction::Trash => self.trash(paths),
        }
        self.close_popup();
        true
    }

    /// Moves `paths` to the Trash: gone from the stack at once, the move itself off the main thread.
    fn trash(&mut self, paths: Vec<PathBuf>) {
        let Some(Popup { kind: PopupKind::Stack(s), .. }) = &mut self.popup else { return };
        let gone: HashSet<&PathBuf> = paths.iter().collect();
        s.selected.retain(|p| !gone.contains(p));
        if let Some(state) = self.stacks.get_mut(&s.folder) {
            state.items.retain(|i| !gone.contains(&i.path));
            if let Some(l) = &mut state.listing {
                l.items.retain(|i| !gone.contains(&i.path));
            }
            state.generation += 1;
        }
        if self.live && !paths.is_empty() {
            std::thread::spawn(move || {
                paths.iter().for_each(|p| {
                    let _ = os::move_to_trash(p);
                })
            });
            self.trash_full = true;
        }
    }

    /// A click on an item or on "Open in Finder".
    pub(super) fn choose_in_stack(&mut self, index: usize) {
        let Some(Popup { kind: PopupKind::Stack(s), .. }) = &self.popup else { return };
        let target = if index == FINDER {
            Some(s.folder.clone())
        } else {
            self.stacks.get(&s.folder).and_then(|st| st.items.get(index)).map(|i| i.path.clone())
        };
        if let Some(path) = target
            && self.live
        {
            os::open(&path);
        }
        self.close_popup();
    }

    /// A key while a stack is open (see `stack::on_key`).
    pub fn stack_key(&mut self, key: Key) {
        let Some(Popup { kind: PopupKind::Stack(s), shut: false, .. }) = &mut self.popup else { return };
        let Some(state) = self.stacks.get(&s.folder) else { return };
        let items = &state.items;
        let grid = s.layout == Layout::Grid;
        // The grid's model is redone when the items changed since it was drawn (a file trashed by key, say).
        if let Some(m) = s.model.as_mut().filter(|m| m.key.generation != state.generation) {
            let k = &m.key;
            (m.shown, m.lines, m.height) = model(items, &k.query, k.cols, k.by, k.midnight);
            m.key.generation = state.generation;
        }
        // The items in the order they're shown: the grid's filter, or the fan's rows from the bottom up.
        let order: Vec<usize> = match &s.model {
            Some(m) if grid => m.shown.clone(),
            _ => (0..items.len().min(s.fanned)).collect(),
        };
        let at = s.cursor.as_ref().and_then(|c| order.iter().position(|&i| &items[i].path == c));
        let path_at = |p: usize| order.get(p).map(|&i| items[i].path.clone());
        let picked: Vec<PathBuf> = if s.selected.is_empty() {
            at.and_then(path_at).or_else(|| (!s.query.is_empty()).then(|| path_at(0)).flatten()).into_iter().collect()
        } else {
            items.iter().filter(|i| s.selected.contains(&i.path)).map(|i| i.path.clone()).collect()
        };
        let mut moved = None;
        match key {
            Key::Text(text) if grid => {
                s.query.push_str(&text);
                s.pick_first = true;
            }
            Key::Text(text) => {
                // The fan: the first name that starts with what's been typed (holding it, failing that).
                if s.age - s.typed_at > TYPE_SELECT {
                    s.query.clear();
                }
                s.typed_at = s.age;
                s.query.push_str(&text.to_lowercase());
                let q = s.query.as_str();
                let starts = order.iter().position(|&i| items[i].name.to_lowercase().starts_with(q));
                moved = starts.or_else(|| order.iter().position(|&i| items[i].name.to_lowercase().contains(q)));
            }
            Key::Back => {
                if grid && s.query.pop().is_some() {
                    s.pick_first = !s.query.is_empty();
                }
            }
            Key::Esc => {
                if grid && !s.query.is_empty() {
                    s.query.clear();
                    s.pick_first = false;
                } else {
                    self.close_popup();
                }
                return;
            }
            Key::Up | Key::Down | Key::Left | Key::Right if order.is_empty() => {}
            Key::Up | Key::Down | Key::Left | Key::Right => {
                moved = Some(match (at, &s.model) {
                    (None, _) => 0,
                    (Some(p), Some(m)) if grid => grid_step(&m.lines, p, &key).unwrap_or(p),
                    // The fan rises from the bottom: up is the next one.
                    (Some(p), _) => match key {
                        Key::Up | Key::Right => (p + 1).min(order.len() - 1),
                        _ => p.saturating_sub(1),
                    },
                });
            }
            Key::Open => {
                if self.live {
                    picked.iter().for_each(|p| os::open(p));
                }
                if !picked.is_empty() {
                    self.close_popup();
                }
                return;
            }
            Key::Look => {
                if !picked.is_empty() {
                    self.effects.push(Effect::QuickLook(picked));
                }
                return;
            }
            Key::Trash => {
                self.trash(picked);
                return;
            }
            Key::All => {
                s.selected = order.iter().map(|&i| items[i].path.clone()).collect();
                return;
            }
            Key::Away => {
                self.close_popup();
                return;
            }
        }
        if let Some(p) = moved
            && let Some(path) = path_at(p)
        {
            s.selected = HashSet::from([path.clone()]);
            s.anchor = Some(path.clone());
            s.cursor = Some(path);
            s.keyed = true;
            if let Some(m) = s.model.as_ref().filter(|_| grid) {
                reveal(s.area.height(), &mut s.scroll, m, p);
                s.since_scroll = 0.0;
            }
        }
    }

    /// The `n`th item shown in the open stack, as last drawn: a fan row or a grid cell.
    pub fn stack_cell(&self, n: usize) -> Option<Rect> {
        match &self.popup.as_ref()?.kind {
            PopupKind::Stack(s) => s.cells.get(n).map(|c| c.1),
            _ => None,
        }
    }

    /// The trackpad (`precise`: followed exactly, with some give past either end) or the wheel (gliding) over an
    /// open grid.
    pub fn scroll(&mut self, dy: f32, precise: bool) {
        let Some(Popup { kind: PopupKind::Stack(s), shut: false, .. }) = &mut self.popup else { return };
        if s.layout != Layout::Grid || s.scroll_most <= 0.0 {
            return;
        }
        s.menu = None;
        s.since_scroll = 0.0;
        if precise {
            let now = s.scroll.target;
            let past = now < 0.0 || now > s.scroll_most;
            let to = now - if past { dy * OVERSCROLL_GIVE } else { dy };
            s.scroll.snap_to(to.clamp(-MOST_OVERSCROLL, s.scroll_most + MOST_OVERSCROLL));
        } else {
            let to = (s.scroll.target - dy).clamp(0.0, s.scroll_most);
            s.scroll.animate_to(to, SCROLL);
        }
    }

    /// An open stack, fan or grid, and the right-click card over it.
    pub(super) fn compose_stack(
        &mut self,
        painter: &egui::Painter,
        placed: &[Placed],
        refs: &[Option<usize>],
    ) -> Vec<Blob> {
        let Some(mut popup) = self.popup.take() else { return Vec::new() };
        let blobs = self.compose_stack_in(painter, placed, refs, &mut popup);
        self.popup = Some(popup);
        blobs
    }

    fn compose_stack_in(
        &mut self,
        painter: &egui::Painter,
        placed: &[Placed],
        refs: &[Option<usize>],
        popup: &mut Popup,
    ) -> Vec<Blob> {
        let Some(i) = self.tiles.iter().position(|t| t.key == popup.key) else {
            popup.shut = true;
            return Vec::new();
        };
        let Some(slot) = refs.iter().position(|r| *r == Some(i)).map(|s| placed[s]) else {
            popup.shut = true;
            return Vec::new();
        };
        let PopupKind::Stack(s) = &popup.kind else { return Vec::new() };
        let (folder, layout) = (s.folder.clone(), s.layout);
        // The items are lent out while drawing, never copied: a folder can hold thousands.
        let (items, generation, arriving) = match self.stacks.get_mut(&folder) {
            Some(st) => {
                let arriving = st.arriving().map(|(p, v)| (p.to_path_buf(), v));
                (std::mem::take(&mut st.items), st.generation, arriving)
            }
            None => (Vec::new(), 0, None),
        };
        let float = self.config.popup_attach == crate::config::PopupAttach::Float;
        let body_top = self.body_top();
        let float_bottom = (self.icon_top(self.peak()) - self.hover_lift() - PAD_TOP).min(body_top);
        let at = At {
            float,
            cx: slot.center(),
            icon_top: self.icon_bottom() - self.tile_size() * slot.scale,
            tile: self.tile_size() * slot.scale,
            body_top,
            base: if float { float_bottom - FLOAT_GAP } else { body_top - NECK_GAP },
            tile_icon: self.tiles[i].icon.clone(),
            name: self.tiles[i].name.clone(),
            folder: folder.clone(),
            generation,
            arriving,
        };
        let mut blobs = Vec::new();
        let lit = match layout {
            Layout::Fan => self.compose_fan(painter, popup, &items, &at, &mut blobs),
            Layout::Grid => self.compose_grid(painter, popup, &items, &at, &mut blobs),
        };
        if let Some(st) = self.stacks.get_mut(&folder) {
            st.items = items;
        }
        let a = smoothstep(0.55, 1.0, popup.open.value);
        let strength = if layout == Layout::Fan { 0.15 } else { 0.09 };
        plate(popup, painter, lit, a, strength);
        self.compose_card(painter, popup, at.base);
        blobs
    }

    /// The fan: icons rising from the tile in a gentle arc, each with its name in a small capsule of the dock's
    /// glass beside it, and "Open in Finder" on top. Returns the row to light.
    fn compose_fan(
        &mut self,
        painter: &egui::Painter,
        popup: &mut Popup,
        items: &[Item],
        at: &At,
        blobs: &mut Vec<Blob>,
    ) -> Option<(Rect, f32)> {
        let shut = popup.shut;
        let PopupKind::Stack(s) = &mut popup.kind else { return None };
        let pointer = if s.menu.is_some() { None } else { self.pointer };
        let carded = s.menu.as_ref().map(|m| m.path.clone());
        let room = ((at.base - 8.0 - FINDER_PITCH) / FAN_PITCH).floor().max(1.0) as usize;
        let shown = items.len().min(s.rows.len()).min(room);
        // An empty folder shows one quiet row saying so, under "Open in Finder".
        let empty = items.is_empty();
        let rows_n = if empty { 1 } else { shown };
        s.fan_rows = rows_n;
        let tile_c = pos2(at.cx, at.icon_top + at.tile * 0.5);
        let mut bounds = Rect::NOTHING;
        let mut drawn = Rect::NOTHING;
        let mut hover = None;
        let mut lit = None;
        s.cells.clear();
        let font = font::regular(13.0);
        let finder_font = font::regular(12.0);
        let nothing = if is_downloads(&at.folder) { "No Downloads" } else { "Nothing Here" };
        // One more row than files: "Open in Finder", smaller, on top.
        #[allow(clippy::needless_range_loop)]
        for k in 0..=rows_n {
            let finder = k == rows_n;
            let placeholder = empty && !finder;
            let r = if finder { s.finder.value } else { s.rows[k].value };
            let (y, side) = if finder {
                (at.base - FAN_PITCH * rows_n as f32 - FINDER_PITCH * 0.5, FINDER_ICON)
            } else {
                (at.base - FAN_PITCH * (k as f32 + 0.5), FAN_ICON)
            };
            let x = at.cx + stack::fan_bend(k);
            let icon = Rect::from_center_size(pos2(x, y), Vec2::splat(side));
            let row_font = if finder { finder_font.clone() } else { font.clone() };
            let words = if finder {
                os::OPEN_IN_FILES
            } else if placeholder {
                nothing
            } else {
                items[k].name.as_str()
            };
            let words = ellipsize(painter, words, FAN_TEXT, row_font.clone());
            let text_w = painter.layout_no_wrap(words.clone(), row_font.clone(), PEARL).size().x;
            let pill_h = if finder { FAN_LABEL - 4.0 } else { FAN_LABEL };
            // The quiet row has no icon: its pill sits where the icons line up.
            let right = if placeholder { icon.right() } else { icon.left() - FAN_GAP };
            let label =
                Rect::from_min_max(pos2(right - text_w - 22.0, y - pill_h * 0.5), pos2(right, y + pill_h * 0.5));
            let hit = if placeholder {
                label
            } else {
                label.union(icon).expand2(vec2(6.0, ((if finder { FINDER_PITCH } else { FAN_PITCH }) - side) * 0.5))
            };
            bounds = bounds.union(hit);
            if !finder && !placeholder {
                s.cells.push((k, hit));
            }
            let path = (!finder && !placeholder).then(|| &items[k].path);
            let hovered = !placeholder && pointer.is_some_and(|p| hit.contains(p)) && !shut;
            if hovered {
                hover = Some(if finder { FINDER } else { k });
            }
            let keyed = s.keyed && path.is_some_and(|p| s.cursor.as_ref() == Some(p));
            if hovered || keyed || (path.is_some() && path == carded.as_ref()) {
                lit = Some((label, pill_h * 0.5));
            }
            let last = s.lift.len() - 1;
            let lift = &mut s.lift[if finder { last } else { k.min(last) }];
            lift.animate_to(if hovered { 1.0 } else { 0.0 }, LIFT);
            let lifted = lift.value;
            if r < 0.003 {
                continue;
            }
            // Each row rises out of the tile (straight up at first, leaning into the arc as it goes), and its
            // name's capsule grows out of its side once it's nearly there.
            let p = r.max(0.0);
            let centre = pos2(lerp(tile_c.x, icon.center().x, p.min(1.0).powf(1.6)), lerp(tile_c.y, y, p));
            let size = lerp(at.tile * 0.7, side, p.min(1.04)) * (1.0 + 0.07 * lifted);
            let icon_now = Rect::from_center_size(centre, Vec2::splat(size));
            let g = smoothstep(0.45, 1.0, r);
            if g > 0.0 {
                let right = if placeholder { icon_now.right() } else { icon_now.left() - FAN_GAP * g };
                let w = label.width() * lerp(0.15, 1.0, g);
                let h = lerp(10.0, pill_h, g);
                let now = Rect::from_min_max(pos2(right - w, centre.y - h * 0.5), pos2(right, centre.y + h * 0.5));
                drawn = drawn.union(now);
                blobs.push(Blob::rect(now, h * 0.5, 6.0));
                if path.is_some_and(|p| s.selected.contains(p)) && !keyed {
                    painter.rect_filled(now, h * 0.5, white(0.1 * g));
                }
                let a = smoothstep(0.75, 1.0, r) * if placeholder { 0.55 } else { 1.0 };
                if a > 0.0 {
                    shaded_text(
                        painter,
                        pos2(right - 11.0, centre.y),
                        Align2::RIGHT_CENTER,
                        &words,
                        row_font.clone(),
                        PEARL.gamma_multiply(a),
                    );
                }
            }
            if placeholder {
                continue;
            }
            drawn = drawn.union(icon_now);
            let a = smoothstep(0.0, 0.25, r);
            if finder {
                if let Some(tex) = &at.tile_icon {
                    paint_picture(painter, tex, false, icon_now, 0.0, a);
                }
            } else {
                let item = &items[k];
                let land = at.arriving.as_ref().filter(|(p, _)| *p == item.path).map_or(1.0, |(_, v)| *v);
                if let Some((tex, thumb, fade)) = self.picture(&item.path) {
                    let place = icon_now.translate(vec2(0.0, -(1.0 - land) * 14.0));
                    paint_picture(painter, &tex, thumb, place, 0.0, a * fade * smoothstep(0.0, 0.5, land));
                }
            }
        }
        popup.rect = bounds;
        popup.drawn = drawn;
        popup.hover = hover;
        lit
    }

    /// The grid: a panel of the dock's glass with the folder's name, its things in rows (under small date headings
    /// in a big folder), and "Open in Finder" underneath. Returns the cell to light.
    #[allow(clippy::too_many_lines)]
    fn compose_grid(
        &mut self,
        painter: &egui::Painter,
        popup: &mut Popup,
        items: &[Item],
        at: &At,
        blobs: &mut Vec<Blob>,
    ) -> Option<(Rect, f32)> {
        let shut = popup.shut;
        let open = popup.open.value;
        let stack_keys = self.stack_keys;
        let time = self.time;
        let PopupKind::Stack(s) = &mut popup.kind else { return None };
        let pointer = if s.menu.is_some() { None } else { self.pointer };
        let carded = s.menu.as_ref().map(|m| m.path.clone());
        let f = self.frame();
        // As many columns as fit across the screen along the bottom, or beside a side dock.
        let across = if f.side.vertical() { at.base - 24.0 } else { self.size.x - 48.0 };
        let most_cols = (((across - SIDE * 2.0) / CELL.x).floor() as usize).max(3);
        let cols = stack::grid_columns(items.len()).min(most_cols);
        let midnight = local_midnight(SystemTime::now());
        let by = self.config.stack_sort;
        let key = ModelKey { query: s.query.clone(), generation: at.generation, cols, by, midnight };
        if s.model.as_ref().is_none_or(|m| m.key != key) {
            let (shown, lines, height) = model(items, &s.query, cols, by, midnight);
            s.model = Some(Model { key, shown, lines, height });
        }
        let m = s.model.take()?;
        if s.pick_first {
            s.pick_first = false;
            s.selected.clear();
            s.cursor = m.shown.first().map(|&i| items[i].path.clone());
            s.selected.extend(s.cursor.clone());
            s.keyed = s.cursor.is_some();
            s.scroll.snap_to(0.0);
        }
        s.searching = stack_keys && (items.len() > SEARCH_FROM || !s.query.is_empty());

        let w = cols as f32 * CELL.x + SIDE * 2.0;
        let room = if f.side.vertical() { f.window.y - 16.0 } else { at.base - 8.0 }.min(MOST_HEIGHT);
        let content =
            if m.shown.is_empty() { EMPTY_HEIGHT } else { m.height.min((room - HEADER - FOOTER).max(CELL.y + 12.0)) };
        s.scroll_most = (m.height - content).max(0.0);
        let h = HEADER + content + FOOTER;
        let final_rect = f.beside(at.cx, at.base, vec2(w, h), 8.0);
        // Worked out in the dock's own layout, so beside a side dock it grows out sideways.
        let mut rect = panel_now(f.unrect(final_rect), at.cx, at.icon_top, open, at.float);
        // A floating panel opens from a third of its size; closing, it goes on shrinking into the tile, to nothing
        // by the time it's gone, rather than stopping at that third and vanishing.
        if shut && at.float {
            let k = smoothstep(0.0, 0.45, open);
            let anchor = pos2(at.cx.clamp(rect.left(), rect.right()), rect.bottom());
            rect = Rect::from_min_max(anchor + (rect.min - anchor) * k, anchor + (rect.max - anchor) * k);
        }
        let rect = f.rect(rect);
        popup.rect = final_rect;
        popup.drawn = rect;
        if !at.float {
            blobs.extend(neck(at.cx, f.unrect(rect), at.body_top, self.tile_size()).map(|b| f.blob(b)));
        }
        blobs.push(Blob::rect(rect, RADIUS.min(rect.height() * 0.5), if at.float { 0.0 } else { 8.0 }));
        let clipped = painter.with_clip_rect(rect.shrink(2.0));
        let painter = &clipped;
        let a = if at.float { smoothstep(0.8, 1.0, open) } else { smoothstep(0.55, 0.95, open) };
        let mut hover = None;
        let mut lit = None;

        // The folder's name, and in a big folder (or once someone types) a quiet search pill.
        let head_y = final_rect.top() + HEADER * 0.5 + 2.0;
        shaded_text(
            painter,
            pos2(final_rect.left() + SIDE + 4.0, head_y),
            Align2::LEFT_CENTER,
            &at.name,
            font::semibold(15.0),
            PEARL.gamma_multiply(a),
        );
        if s.searching {
            let pill = Rect::from_center_size(pos2(final_rect.right() - SIDE - SEARCH.x * 0.5, head_y), SEARCH);
            painter.rect_filled(pill, SEARCH.y * 0.5, white(0.07 * a));
            let lens = pos2(pill.left() + 14.0, head_y - 0.5);
            let ink = Stroke::new(1.3, PEARL.gamma_multiply(0.45 * a));
            painter.circle_stroke(lens, 4.0, ink);
            painter.line_segment([lens + vec2(2.9, 2.9), lens + vec2(5.6, 5.6)], ink);
            let text_at = pos2(pill.left() + 25.0, head_y);
            let font = font::regular(12.5);
            if s.query.is_empty() {
                painter.text(text_at, Align2::LEFT_CENTER, "Search", font, PEARL.gamma_multiply(0.36 * a));
            } else {
                let shown = ellipsize_start(painter, &s.query, pill.width() - 40.0, font.clone());
                let r = painter.text(text_at, Align2::LEFT_CENTER, shown, font, PEARL.gamma_multiply(a));
                if (time * 1.9).fract() < 0.55 {
                    let caret = Rect::from_min_size(pos2(r.right() + 1.5, head_y - 7.5), vec2(1.5, 15.0));
                    painter.rect_filled(caret, 0.75, PEARL.gamma_multiply(0.8 * a));
                }
            }
        }

        let area = Rect::from_min_max(
            pos2(final_rect.left(), final_rect.top() + HEADER),
            pos2(final_rect.right(), final_rect.bottom() - FOOTER),
        );
        s.area = area;
        let scroll = s.scroll.value;
        let inside = painter.with_clip_rect(area.intersect(rect.shrink(2.0)));
        // Cells fade at an edge with more beyond it, rather than being cut by a line.
        let top_more = smoothstep(0.0, 12.0, scroll);
        let bottom_more = smoothstep(0.0, 12.0, s.scroll_most - scroll);
        let edge = |y: f32| {
            let top = lerp(1.0, smoothstep(area.top() - 6.0, area.top() + EDGE_FADE, y), top_more);
            let bottom = lerp(1.0, 1.0 - smoothstep(area.bottom() - EDGE_FADE, area.bottom() + 6.0, y), bottom_more);
            top * bottom
        };

        if m.shown.is_empty() {
            let c = area.center();
            let (title, detail) = if !s.query.is_empty() {
                ("No Results".to_owned(), format!("Nothing here matches \u{201c}{}\u{201d}", s.query))
            } else if is_downloads(&at.folder) {
                ("No Downloads".to_owned(), "Files you download show up here".to_owned())
            } else {
                (format!("{} Is Empty", at.name), "Nothing in this folder yet".to_owned())
            };
            if s.query.is_empty()
                && let Some(tex) = &at.tile_icon
            {
                let icon = Rect::from_center_size(c - vec2(0.0, 22.0), Vec2::splat(56.0));
                paint_picture(painter, tex, false, icon, 0.0, a * 0.5);
            }
            let y = if s.query.is_empty() { c.y + 24.0 } else { c.y - 8.0 };
            let ink = PEARL.gamma_multiply(a);
            shaded_text(painter, pos2(c.x, y), Align2::CENTER_CENTER, title, font::semibold(13.0), ink);
            let soft = PEARL.gamma_multiply(a * 0.5);
            shaded_text(painter, pos2(c.x, y + 19.0), Align2::CENTER_CENTER, detail, font::regular(11.5), soft);
        }

        let left = final_rect.left() + SIDE;
        let name_font = font::regular(11.5);
        let mut near = HashSet::new();
        let mut line_no = 0usize;
        s.cells.clear();
        for (top, line) in &m.lines {
            let y0 = area.top() + top - scroll;
            let lh = if matches!(line, Line::Heading(_)) { HEADING } else { CELL.y };
            if y0 + lh < area.top() - area.height() * 0.5 {
                continue;
            }
            if y0 > area.bottom() + area.height() * 0.5 {
                break;
            }
            let visible = y0 + lh > area.top() && y0 < area.bottom();
            let delay = (line_no as f32 * LINE_STAGGER).min(LINE_STAGGER_MOST);
            if visible {
                line_no += 1;
            }
            let ease = ease_out(((s.age - delay) / CELL_IN).clamp(0.0, 1.0));
            let rise = (1.0 - ease) * 8.0;
            match line {
                Line::Heading(title) => {
                    if visible {
                        let ty = y0 + HEADING * 0.62 + rise;
                        let ink = PEARL.gamma_multiply(0.4 * a * ease * edge(ty));
                        inside.text(pos2(left + 8.0, ty), Align2::LEFT_CENTER, *title, font::semibold(11.0), ink);
                    }
                }
                Line::Cells(range) => {
                    for p in range.clone() {
                        let n = m.shown[p];
                        let item = &items[n];
                        let col = (p - range.start) as f32;
                        let cell = Rect::from_min_size(pos2(left + col * CELL.x, y0), CELL);
                        near.insert(item.path.clone());
                        if !visible {
                            self.want_picture(&item.path);
                            continue;
                        }
                        s.cells.push((n, cell));
                        let plate_rect = cell.shrink2(vec2(4.0, 3.0));
                        if pointer.is_some_and(|q| cell.contains(q) && area.contains(q)) && !shut {
                            hover = Some(n);
                            lit = Some((plate_rect, 12.0));
                        }
                        if carded.as_ref() == Some(&item.path) {
                            lit = Some((plate_rect, 12.0));
                        }
                        let land = at.arriving.as_ref().filter(|(q, _)| *q == item.path).map_or(1.0, |(_, v)| *v);
                        let icon_y = cell.top() + 8.0 + CELL_ICON * 0.5 + rise + (1.0 - land.min(1.0)) * 10.0;
                        let ca = a * ease * smoothstep(0.0, 0.6, land) * edge(icon_y);
                        if ca <= 0.0 {
                            continue;
                        }
                        if s.selected.contains(&item.path) {
                            inside.rect_filled(plate_rect, 12.0, white(0.12 * ca));
                        }
                        if s.keyed && s.cursor.as_ref() == Some(&item.path) {
                            let ring = (1.0, white(0.32 * ca));
                            inside.rect_stroke(plate_rect, 12.0, ring, egui::StrokeKind::Inside);
                        }
                        let icon = Rect::from_center_size(
                            pos2(cell.center().x, icon_y),
                            Vec2::splat(CELL_ICON * lerp(0.92, 1.0, ease)),
                        );
                        if let Some((tex, thumb, fade)) = self.picture(&item.path) {
                            paint_picture(&inside, &tex, thumb, icon, 0.0, ca * fade);
                        }
                        let label = s.short.entry(item.path.clone()).or_insert_with(|| {
                            let fits = |t: &str| {
                                painter.layout_no_wrap(t.to_owned(), name_font.clone(), PEARL).size().x <= CELL.x - 14.0
                            };
                            middle(&item.name.chars().collect::<Vec<_>>(), &fits)
                        });
                        let ty = cell.top() + 8.0 + CELL_ICON + 13.0 + rise * 0.5;
                        let ink = PEARL.gamma_multiply(ca * edge(ty) * 0.92);
                        let at_text = pos2(cell.center().x, ty);
                        shaded_text(&inside, at_text, Align2::CENTER_CENTER, label.as_str(), name_font.clone(), ink);
                    }
                }
            }
        }

        // The scroll indicator, only while scrolling: a thin bar that shortens when pulled past an end.
        let shows = 1.0 - smoothstep(INDICATOR_STAYS, INDICATOR_STAYS + 0.35, s.since_scroll);
        if s.scroll_most > 0.0 && shows > 0.0 {
            let track = area.shrink2(vec2(0.0, 8.0));
            let over = (-scroll).max(scroll - s.scroll_most).max(0.0);
            let len = ((track.height() * area.height() / m.height).max(28.0) - over * 0.6).max(12.0);
            let k = (scroll / s.scroll_most).clamp(0.0, 1.0);
            let y = lerp(track.top(), track.bottom() - len, k);
            let bar = Rect::from_min_size(pos2(final_rect.right() - 7.0, y), vec2(3.0, len));
            painter.rect_filled(bar, 1.5, white(0.42 * shows * a));
        }

        // Underneath: the item under the pointer, softly, and the one quiet link to Finder.
        let foot_y = final_rect.bottom() - FOOTER * 0.5 - 1.0;
        let link_font = font::regular(12.0);
        let link_w = painter.layout_no_wrap(os::OPEN_IN_FILES.into(), link_font.clone(), PEARL).size().x;
        let link = Rect::from_min_max(
            pos2(final_rect.right() - SIDE - 4.0 - link_w, foot_y - 9.0),
            pos2(final_rect.right() - SIDE - 4.0, foot_y + 9.0),
        );
        let over_link = pointer.is_some_and(|q| link.expand2(vec2(8.0, 6.0)).contains(q)) && !shut;
        if over_link {
            hover = Some(FINDER);
            lit = Some((link.expand2(vec2(8.0, 4.0)), 9.0));
        }
        let link_ink = PEARL.gamma_multiply(a * if over_link { 0.95 } else { 0.62 });
        shaded_text(painter, link.right_center(), Align2::RIGHT_CENTER, os::OPEN_IN_FILES, link_font.clone(), link_ink);
        let about = hover.filter(|&n| n != FINDER).or_else(|| {
            let c = s.cursor.as_ref().filter(|_| s.keyed)?;
            items.iter().position(|i| &i.path == c)
        });
        if let Some(item) = about.and_then(|n| items.get(n)) {
            let when = if by == StackSort::Modified { item.modified } else { item.added };
            let what = if item.folder { "Folder".to_owned() } else { bytes(item.size) };
            let room = link.left() - (final_rect.left() + SIDE + 4.0) - 16.0;
            // In a narrow grid the name (already under its icon) goes first, rather than cutting the date.
            let when = when_text(when, midnight);
            let fits = |t: &String| painter.layout_no_wrap(t.clone(), link_font.clone(), PEARL).size().x <= room;
            let text = [format!("{}  \u{b7}  {what}  \u{b7}  {when}", item.name), format!("{what}  \u{b7}  {when}")]
                .into_iter()
                .find(fits)
                .unwrap_or_else(|| ellipsize(painter, &when, room, link_font.clone()));
            let ink = PEARL.gamma_multiply(a * 0.5);
            painter.text(pos2(final_rect.left() + SIDE + 4.0, foot_y), Align2::LEFT_CENTER, text, link_font, ink);
        }

        s.model = Some(m);
        popup.hover = hover;
        if !shut {
            self.keep_pictures(near);
        }
        lit
    }

    fn compose_card(&mut self, painter: &egui::Painter, popup: &mut Popup, base: f32) {
        let dark = self.dark;
        let pointer = self.pointer;
        let f = self.frame();
        let width = self.size.x;
        let PopupKind::Stack(s) = &mut popup.kind else { return };
        let Some(menu) = &mut s.menu else { return };
        let h = card_height();
        // Anywhere on the window clear of the dock.
        let room = f.rect(Rect::from_min_max(pos2(8.0, 8.0), pos2(width - 8.0, base)));
        // Like a context menu: its corner at the pointer, opening down and right, or the other way where there's
        // no room.
        let x = if menu.at.x + CARD_WIDTH <= room.right() { menu.at.x } else { menu.at.x - CARD_WIDTH };
        let y = if menu.at.y + h <= room.bottom() { menu.at.y } else { menu.at.y - h };
        let x = x.clamp(room.left(), (room.right() - CARD_WIDTH).max(room.left()));
        let y = y.clamp(room.top(), (room.bottom() - h).max(room.top()));
        let rect = Rect::from_min_size(pos2(x, y), vec2(CARD_WIDTH, h));
        menu.rect = rect;
        let o = menu.open.value;
        let ca = smoothstep(0.2, 0.9, o);
        let grow = lerp(0.94, 1.0, o.clamp(0.0, 1.04));
        let anchor = pos2(menu.at.x.clamp(rect.left(), rect.right()), menu.at.y.clamp(rect.top(), rect.bottom()));
        let now = Rect::from_min_max(anchor + (rect.min - anchor) * grow, anchor + (rect.max - anchor) * grow);
        let shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha((90.0 * ca) as u8),
        };
        painter.add(shadow.as_shape(now, 11));
        painter.rect_filled(now, 11.0, Color32::from_rgba_premultiplied(26, 26, 28, (248.0 * ca) as u8));
        painter.rect_stroke(now, 11.0, (1.0, white(0.09 * ca)), egui::StrokeKind::Inside);
        let mut y = now.top() + 6.0;
        menu.hover = None;
        for (n, item) in CARD_ITEMS.iter().enumerate() {
            let Some((words, _)) = item else {
                let ppp = painter.pixels_per_point();
                let ly = ((y + 4.5) * ppp).floor() / ppp;
                let line = Rect::from_min_max(pos2(now.left() + 14.0, ly), pos2(now.right() - 14.0, ly + 1.0 / ppp));
                painter.rect_filled(line, 0.0, divider_ink(dark).gamma_multiply(ca));
                y += 9.0;
                continue;
            };
            let row = Rect::from_min_max(pos2(now.left() + 5.0, y), pos2(now.right() - 5.0, y + CARD_ROW));
            if pointer.is_some_and(|p| row.contains(p)) && o > 0.6 {
                menu.hover = Some(n);
                painter.rect_filled(row, 7.0, white(0.11 * ca));
            }
            shaded_text(
                painter,
                pos2(row.left() + 11.0, row.center().y),
                Align2::LEFT_CENTER,
                *words,
                font::regular(13.0),
                PEARL.gamma_multiply(ca),
            );
            y += CARD_ROW;
        }
        popup.rect = popup.rect.union(rect);
    }

    /// A folder's tile: a pile of its newest things, the newest on top, or the folder itself when there's nothing
    /// to show. True when it drew the pile.
    pub(super) fn paint_pile(&mut self, painter: &egui::Painter, i: usize, rect: Rect, alpha: f32) -> bool {
        let folder = self.tiles[i].path.clone();
        let Some(state) = self.stacks.get(&folder) else { return false };
        let shown: Vec<PathBuf> = state.items.iter().take(PILE.len()).map(|i| i.path.clone()).collect();
        let arriving = state.arriving().map(|(p, v)| (p.to_path_buf(), v));
        if shown.is_empty() {
            return false;
        }
        let pictures: Vec<_> = shown.iter().map(|p| self.picture(p)).collect();
        // The folder stays until the top of the pile has its picture.
        if pictures[0].is_none() {
            return false;
        }
        let side = rect.width();
        let landing = arriving.as_ref().and_then(|(p, v)| shown.iter().position(|s| s == p).map(|k| (k, *v)));
        for k in (0..shown.len()).rev() {
            let Some((tex, thumb, fade)) = &pictures[k] else { continue };
            let pose = |k: usize| PILE[k.min(PILE.len() - 1)];
            let (mut dx, mut dy, mut turn, mut size) = pose(k);
            let mut a = alpha * fade;
            let mut drop = 0.0;
            if let Some((at, v)) = landing {
                if k > at {
                    // Everything under the new arrival shuffles down a place as it lands.
                    let (px, py, pt, ps) = pose(k - 1);
                    let t = v.clamp(0.0, 1.0);
                    (dx, dy, turn, size) = (lerp(px, dx, t), lerp(py, dy, t), lerp(pt, turn, t), lerp(ps, size, t));
                } else if k == at {
                    drop = (1.0 - v) * side * 1.1;
                    a *= smoothstep(0.0, 0.35, v);
                }
            }
            let c = rect.center() + vec2(dx * side, dy * side - drop);
            let r = Rect::from_center_size(c, Vec2::splat(side * size));
            paint_picture(painter, tex, *thumb, r, turn.to_radians(), a);
        }
        true
    }

    /// Downloads still arriving: a small bar under the tile, a light gliding along it.
    pub(super) fn paint_progress(&self, painter: &egui::Painter, i: usize, at: Pos2, tile: f32) {
        if self.stacks.get(&self.tiles[i].path).is_none_or(|s| s.partial == 0) {
            return;
        }
        super::ambient::paint_bar(painter, at, tile, None, 1.0, self.time, self.side().vertical());
    }
}

/// Lays out the grid: the items whose names hold every word of `query`, `cols` across, under date headings when
/// there are enough of them across more than one day (and they're in date order). Returns what's shown (indices
/// into `items`), the lines with their tops, and the whole height.
fn model(
    items: &[Item],
    query: &str,
    cols: usize,
    by: StackSort,
    midnight: SystemTime,
) -> (Vec<usize>, Vec<(f32, Line)>, f32) {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let shown: Vec<usize> = (0..items.len())
        .filter(|&i| {
            terms.is_empty() || {
                let name = items[i].name.to_lowercase();
                terms.iter().all(|t| name.contains(t.as_str()))
            }
        })
        .collect();
    let time = |i: usize| if by == StackSort::Modified { items[i].modified } else { items[i].added };
    let group = |p: usize| heading(time(shown[p]), midnight);
    let headed =
        terms.is_empty() && by != StackSort::Name && shown.len() > HEADINGS_FROM && group(0) != group(shown.len() - 1);
    let cols = cols.max(1);
    let mut lines = Vec::new();
    let mut y = if headed { 0.0 } else { 6.0 };
    let mut p = 0;
    while p < shown.len() {
        let title = headed.then(|| group(p));
        let mut end = p + 1;
        while end < shown.len() && title.is_none_or(|t| group(end) == t) {
            end += 1;
        }
        if let Some(t) = title {
            lines.push((y, Line::Heading(t)));
            y += HEADING;
        }
        let mut row = p;
        while row < end {
            let to = (row + cols).min(end);
            lines.push((y, Line::Cells(row..to)));
            y += CELL.y;
            row = to;
        }
        p = end;
    }
    (shown, lines, y + 6.0)
}

/// The date heading a time falls under, Finder's way.
fn heading(t: SystemTime, midnight: SystemTime) -> &'static str {
    if t >= midnight {
        "Today"
    } else if t >= midnight - DAY {
        "Yesterday"
    } else if t >= midnight - DAY * 6 {
        "Previous 7 Days"
    } else if t >= midnight - DAY * 29 {
        "Previous 30 Days"
    } else {
        "Earlier"
    }
}

/// The arrow keys in the grid: along a row, or to the same column of the row above or below (across headings).
fn grid_step(lines: &[(f32, Line)], at: usize, key: &Key) -> Option<usize> {
    let rows: Vec<&Range<usize>> = lines
        .iter()
        .filter_map(|(_, l)| match l {
            Line::Cells(r) => Some(r),
            Line::Heading(_) => None,
        })
        .collect();
    let row = rows.iter().position(|r| r.contains(&at))?;
    let col = at - rows[row].start;
    let last = rows.last()?.end - 1;
    match key {
        Key::Left => Some(at.saturating_sub(1)),
        Key::Right => Some((at + 1).min(last)),
        Key::Up => Some(row.checked_sub(1).map_or(at, |r| (rows[r].start + col).min(rows[r].end - 1))),
        Key::Down => Some(rows.get(row + 1).map_or(at, |r| (r.start + col).min(r.end - 1))),
        _ => None,
    }
}

/// Scrolls so the `p`th item shown is in view, with a little room around it.
fn reveal(view: f32, scroll: &mut Spring, m: &Model, p: usize) {
    let Some(top) = m.lines.iter().find_map(|(y, l)| matches!(l, Line::Cells(r) if r.contains(&p)).then_some(*y))
    else {
        return;
    };
    let most = (m.height - view).max(0.0);
    let to = if top < scroll.target + 4.0 {
        top - 10.0
    } else if top + CELL.y > scroll.target + view - 4.0 {
        top + CELL.y - view + 10.0
    } else {
        return;
    };
    scroll.animate_to(to.clamp(0.0, most), SCROLL);
}

/// The start of today, local time.
fn local_midnight(now: SystemTime) -> SystemTime {
    let Some(tm) = local(now) else { return now };
    let into = tm.tm_hour.max(0) as u64 * 3600 + tm.tm_min.max(0) as u64 * 60 + tm.tm_sec.max(0) as u64;
    now - Duration::from_secs(into) - Duration::from_nanos(u64::from(now_nanos(now)))
}

fn now_nanos(t: SystemTime) -> u32 {
    t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())
}

fn local(t: SystemTime) -> Option<libc::tm> {
    crate::clock::local(t.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// "Today at 14:02", "Yesterday at 9:10", "15 Oct at 16:55", or "15 Oct 2025" for another year.
fn when_text(t: SystemTime, midnight: SystemTime) -> String {
    let Some(tm) = local(t) else { return String::new() };
    let clock = format!("{}:{:02}", tm.tm_hour, tm.tm_min);
    if t >= midnight {
        return format!("Today at {clock}");
    }
    if t >= midnight - DAY {
        return format!("Yesterday at {clock}");
    }
    let month = MONTHS[tm.tm_mon.clamp(0, 11) as usize];
    if local(midnight).map(|n| n.tm_year) == Some(tm.tm_year) {
        format!("{} {month} at {clock}", tm.tm_mday)
    } else {
        format!("{} {month} {}", tm.tm_mday, tm.tm_year + 1900)
    }
}

/// A size as Finder writes it, in thousands.
fn bytes(n: u64) -> String {
    match n {
        1 => "1 byte".to_owned(),
        0..1000 => format!("{n} bytes"),
        1000..1_000_000 => format!("{:.0} KB", n as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", n as f64 / 1e6),
        _ => format!("{:.2} GB", n as f64 / 1e9),
    }
}

/// The highlight under the hovered row or cell, gliding from one to the next.
fn plate(popup: &mut Popup, painter: &egui::Painter, lit: Option<(Rect, f32)>, a: f32, strength: f32) {
    if let Some((r, radius)) = lit {
        if popup.plate_shown.value < 0.05 {
            popup.plate[0].snap_to(r.top());
            popup.plate[1].snap_to(r.left());
        }
        popup.plate[0].animate_to(r.top(), PLATE);
        popup.plate[1].animate_to(r.left(), PLATE);
        popup.plate_shown.animate_to(1.0, PLATE);
        let at = Rect::from_min_size(pos2(popup.plate[1].value, popup.plate[0].value), r.size());
        painter.rect_filled(at, radius, white(strength * popup.plate_shown.value * a));
    } else {
        popup.plate_shown.animate_to(0.0, PLATE);
    }
}

/// Shortened in the middle until it fits, keeping its start and a little more of its end (the extension).
fn middle(chars: &[char], fits: &dyn Fn(&str) -> bool) -> String {
    let whole: String = chars.iter().collect();
    if fits(&whole) {
        return whole;
    }
    for keep in (2..chars.len()).rev() {
        let tail = (keep * 2).div_ceil(5).max(1);
        let head = keep - tail;
        let s: String =
            chars[..head].iter().chain(['\u{2026}'].iter()).chain(chars[chars.len() - tail..].iter()).collect();
        if fits(&s) {
            return s;
        }
    }
    "\u{2026}".into()
}

/// What's been typed, its start cut off when it's too long for the pill, so the end being typed shows.
fn ellipsize_start(painter: &egui::Painter, text: &str, width: f32, font: egui::FontId) -> String {
    let fits = |s: &str| painter.layout_no_wrap(s.to_owned(), font.clone(), PEARL).size().x <= width;
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    (1..chars.len())
        .map(|k| std::iter::once('\u{2026}').chain(chars[k..].iter().copied()).collect::<String>())
        .find(|s| fits(s))
        .unwrap_or_else(|| "\u{2026}".into())
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// A file's picture fitted in `rect`, turned by `turn` radians. A thumbnail of the contents is framed like a
/// print, with a thin light border and a soft shadow, as Finder shows them; icons are drawn as they are.
fn paint_picture(painter: &egui::Painter, tex: &TextureHandle, thumb: bool, rect: Rect, turn: f32, alpha: f32) {
    if alpha <= 0.0 || rect.width() < 1.0 {
        return;
    }
    let [w, h] = tex.size().map(|v| v.max(1) as f32);
    let side = rect.width().min(rect.height());
    let k = if thumb { side * 0.84 } else { side } / w.max(h);
    let size = vec2(w * k, h * k);
    let c = rect.center();
    let rot = Rot2::from_angle(turn);
    let quad = |r: Rect| -> Vec<Pos2> {
        [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]
            .into_iter()
            .map(|p| c + rot * (p - c))
            .collect()
    };
    if thumb {
        let frame = Rect::from_center_size(c, size + Vec2::splat((side * 0.05).max(1.5)));
        painter.add(Shape::convex_polygon(
            quad(frame.translate(vec2(0.0, 1.0)).expand(0.5)),
            Color32::from_black_alpha((70.0 * alpha) as u8),
            Stroke::NONE,
        ));
        painter.add(Shape::convex_polygon(quad(frame), Color32::from_white_alpha((245.0 * alpha) as u8), Stroke::NONE));
    }
    let mut mesh = Mesh::with_texture(tex.id());
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    mesh.add_rect_with_uv(Rect::from_center_size(c, size), uv, Color32::WHITE.gamma_multiply(alpha));
    if turn != 0.0 {
        mesh.rotate(rot, c);
    }
    painter.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests;
