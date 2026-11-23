//! One notch window per display: placing them, picking the active one and following the pointer between them.

pub(super) mod window;

use std::fmt::Display;
use std::time::{Duration, Instant};

use egui::{Pos2, Rect};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::bridge::{Action, Activity, Page, PanelData};
use crate::media::NowPlaying;
use crate::notch::Notch;
use crate::platform::{self, Placement};

use super::App;
use window::NotchWindow;

const PLACEMENT_REFRESH: Duration = Duration::from_secs(1);
const NOT_SAVED: &str = "Couldn't save the display setting";

#[derive(Default)]
pub(super) struct Notches {
    windows: Vec<NotchWindow>,
    active: usize,
    placed_at: Option<Instant>,
}

impl Notches {
    pub(super) fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &NotchWindow> {
        self.windows.iter()
    }

    pub(super) fn active(&self) -> Option<&NotchWindow> {
        self.windows.get(self.active)
    }

    pub(super) fn active_mut(&mut self) -> Option<&mut NotchWindow> {
        self.windows.get_mut(self.active)
    }

    pub(super) fn is_active(&self, index: usize) -> bool {
        index == self.active
    }

    pub(super) fn windows_mut(&mut self) -> impl Iterator<Item = (usize, &mut NotchWindow)> {
        self.windows.iter_mut().enumerate()
    }

    pub(super) fn window_mut(&mut self, id: WindowId) -> Option<&mut NotchWindow> {
        self.windows.iter_mut().find(|nw| nw.window().id() == id)
    }

    pub(super) fn owns(&self, id: WindowId) -> bool {
        self.windows.iter().any(|nw| nw.window().id() == id)
    }

    pub(super) fn focus(&mut self, id: WindowId) {
        if let Some(index) = self.windows.iter().position(|nw| nw.window().id() == id) {
            self.active = index;
        }
    }

    pub(super) fn follow_cursor(&mut self) {
        if let Some(index) = self.under_cursor() {
            self.active = index;
        }
    }

    pub(super) fn under_cursor(&self) -> Option<usize> {
        let seen = |nw: &NotchWindow| {
            let cursor = nw.cursor()?;
            let on_its_display = nw.display().is_some_and(|display| display.contains(cursor));
            Some(Seen { on_its_display, away: distance(nw.notch.canvas_rect(), cursor) })
        };
        let seen: Vec<Option<Seen>> = self.windows.iter().map(seen).collect();
        pointed_at(&seen)
    }

    pub(super) fn nth_mut(&mut self, index: usize) -> Option<&mut NotchWindow> {
        self.windows.get_mut(index)
    }

    pub(super) fn visible_page(&self) -> Option<Page> {
        self.active()
            .and_then(|nw| nw.notch.visible_page())
            .or_else(|| self.windows.iter().find_map(|nw| nw.notch.visible_page()))
    }

    pub(super) fn any_expanded(&self) -> bool {
        self.windows.iter().any(|nw| nw.notch.expanded())
    }

    pub(super) fn all_collapsed(&self) -> bool {
        self.windows.iter().all(|nw| nw.notch.is_collapsed())
    }

    pub(super) fn redraw_all(&self) {
        for nw in &self.windows {
            nw.redraw();
        }
    }

    pub(super) fn update_all(&mut self, mut change: impl FnMut(&mut Notch)) {
        for nw in &mut self.windows {
            change(&mut nw.notch);
            nw.redraw();
        }
    }

    fn placement_due(&mut self, now: Instant) -> bool {
        let due = self.placed_at.is_none_or(|at| now.duration_since(at) >= PLACEMENT_REFRESH);
        if due {
            self.placed_at = Some(now);
        }
        due
    }

    fn retire_spent(&mut self) -> Vec<NotchWindow> {
        let (spent, kept) = self.windows.drain(..).partition(NotchWindow::spent);
        self.windows = kept;
        self.active = self.active.min(self.windows.len().saturating_sub(1));
        spent
    }

    fn arrange(
        &mut self,
        wanted: Vec<Placement>,
        mut open: impl FnMut(Placement) -> Option<NotchWindow>,
    ) -> (bool, Vec<NotchWindow>) {
        let mut kept: Vec<Option<NotchWindow>> = self.windows.drain(..).map(Some).collect();
        let mut unmatched = Vec::new();
        let mut arranged: Vec<Option<NotchWindow>> = Vec::with_capacity(wanted.len());
        for placement in wanted {
            let same_screen =
                kept.iter().position(|nw| nw.as_ref().is_some_and(|nw| nw.placement().monitor == placement.monitor));
            match same_screen.and_then(|i| kept[i].take()) {
                Some(mut nw) => {
                    nw.place(placement);
                    arranged.push(Some(nw));
                }
                None => {
                    unmatched.push((arranged.len(), placement));
                    arranged.push(None);
                }
            }
        }
        let mut spare = kept.into_iter().flatten();
        let mut changed = !unmatched.is_empty();
        for (slot, placement) in unmatched {
            arranged[slot] = match spare.next() {
                Some(mut nw) => {
                    nw.place(placement);
                    Some(nw)
                }
                None => open(placement),
            };
        }
        let gone: Vec<NotchWindow> = spare.collect();
        changed |= !gone.is_empty();
        self.windows = arranged.into_iter().flatten().collect();
        self.active = self.active.min(self.windows.len().saturating_sub(1));
        (changed, gone)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    on_its_display: bool,
    away: f32,
}

fn pointed_at(seen: &[Option<Seen>]) -> Option<usize> {
    let nearest = |only_its_display: bool| {
        let counted = |(index, seen): (usize, &Option<Seen>)| {
            seen.filter(|seen| seen.on_its_display || !only_its_display).map(|seen| (index, seen.away))
        };
        seen.iter()
            .enumerate()
            .filter_map(counted)
            .min_by(|one, other| one.1.total_cmp(&other.1))
            .map(|(index, _)| index)
    };
    nearest(true).or_else(|| nearest(false))
}

fn distance(canvas: Rect, cursor: Pos2) -> f32 {
    let dx = (canvas.left() - cursor.x).max(cursor.x - canvas.right()).max(0.0);
    let dy = (canvas.top() - cursor.y).max(cursor.y - canvas.bottom()).max(0.0);
    dx.hypot(dy)
}

impl App {
    pub(super) fn open_notches(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        self.arrange_notches(event_loop);
        if self.notches.is_empty() { Err("no monitor found".into()) } else { Ok(()) }
    }

    pub(super) fn set_all_displays(&mut self, event_loop: &ActiveEventLoop, on: bool) {
        if let Err(err) = self.config.update(|config| config.displays.all = on) {
            self.report(NOT_SAVED, err);
        }
        self.arrange_notches(event_loop);
        self.redraw_notch();
    }

    pub(super) fn set_wake_at_edge(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.notch.wake_at_edge = on) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }

    pub(super) fn set_haptics(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.notch.haptics = on) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }

    pub(super) fn set_lock_screen(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.notch.lock_screen = on) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }

    pub(super) fn set_snap_windows(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.snap.enabled = on) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }

    pub(super) fn set_dodge_menus(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.notch.dodge_menus = on) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }

    pub(super) fn set_flee_from_keys(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.notch.flee_from_keys = on) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }

    pub(super) fn keep_notches_placed(&mut self, event_loop: &ActiveEventLoop, now: Instant) -> bool {
        if !self.notches.placement_due(now) {
            return false;
        }
        if !self.capturing() {
            for nw in self.notches.iter() {
                platform::keep_on_top(nw.window());
            }
        }
        self.arrange_notches(event_loop)
    }

    pub(super) fn replace_spent_notches(&mut self, event_loop: &ActiveEventLoop) {
        let spent = self.notches.retire_spent();
        if !spent.is_empty() {
            self.arrange_notches_after(event_loop, &spent);
            self.retire_notches(event_loop, spent);
            self.redraw_notch();
        }
    }

    pub(super) fn arrange_notches(&mut self, event_loop: &ActiveEventLoop) -> bool {
        self.arrange_notches_after(event_loop, &[])
    }

    fn arrange_notches_after(&mut self, event_loop: &ActiveEventLoop, spent: &[NotchWindow]) -> bool {
        let wanted = platform::placements(event_loop, self.config.config().displays.all);
        let shortcuts = self.shortcut_views();
        let app = self.app_view();
        let data = self.panel.data(&shortcuts, &self.tabs, &self.tools, &self.locations, self.config.config(), app);
        let shown = self.shown();
        let inbox = &self.drop_inbox;
        let (changed, gone) = self.notches.arrange(wanted, |placement| {
            let replaced = spent.iter().find(|old| old.placement().monitor == placement.monitor);
            let prime = |notch: &mut Notch| {
                if let Some(old) = replaced {
                    shown.apply(notch);
                    notch.take_over(&old.notch);
                }
            };
            match NotchWindow::open(event_loop, placement, &data, inbox, prime) {
                Ok(nw) => Some(nw),
                Err(err) => {
                    eprintln!("notchbase: couldn't open a notch: {err}");
                    None
                }
            }
        });
        self.retire_notches(event_loop, gone);
        if changed {
            self.sync_notch_state();
        }
        changed
    }

    fn retire_notches(&mut self, event_loop: &ActiveEventLoop, gone: Vec<NotchWindow>) {
        let left: Vec<Action> = gone
            .into_iter()
            .flat_map(|mut nw| {
                nw.follow_lock(false);
                nw.notch.retire()
            })
            .collect();
        for action in left {
            self.handle(event_loop, action);
        }
    }

    pub(super) fn paint_notch(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        let now = Instant::now();
        let shortcuts = self.shortcut_views();
        let app = self.app_view();
        let Some(nw) = self.notches.window_mut(id) else { return };
        if matches!(nw.notch.visible_page(), Some(Page::Sound | Page::Media)) {
            self.panel.mixer.update_levels();
        }
        let data = self.panel.data(&shortcuts, &self.tabs, &self.tools, &self.locations, self.config.config(), app);
        let active = self.notches.active().is_some_and(|active| active.window().id() == id);
        let Some(nw) = self.notches.window_mut(id) else { return };
        let data = PanelData { typing: self.typing && active, ..data };
        let actions = nw.paint(now, &data);
        for action in actions {
            match action {
                Action::FindShown(true) => self.find_opened_on(id),
                Action::ViewChanges => self.view_changes(Some(id)),
                Action::AnswerDockIntro(on) => self.answer_dock_intro(on, Some(id)),
                Action::OpenDockIntro => {
                    if let Some(nw) = self.notches.window_mut(id) {
                        nw.notch.toggle_dock_intro();
                        nw.redraw();
                    }
                }
                Action::ShowPermissions => self.show_permissions(Some(id)),
                action => self.handle(event_loop, action),
            }
        }
    }

    pub(super) fn owns_notch(&self, id: WindowId) -> bool {
        self.notches.owns(id)
    }

    pub(super) fn visible_page(&self) -> Option<Page> {
        self.notches.visible_page()
    }

    pub(super) fn redraw_notch(&self) {
        self.notches.redraw_all();
    }

    pub(super) fn update_notch(&mut self, change: impl FnMut(&mut Notch)) {
        self.notches.update_all(change);
    }

    pub(super) fn notify(&mut self, message: &str) {
        eprintln!("notchbase: {message}");
        self.update_notch(|notch| notch.show_message(message));
    }

    pub(super) fn report(&mut self, message: &str, err: impl Display) {
        eprintln!("notchbase: {message}: {err}");
        self.update_notch(|notch| notch.show_message(message));
    }

    fn sync_notch_state(&mut self) {
        let shown = self.shown();
        self.update_notch(|notch| shown.apply(notch));
        self.prompts_seen = 0;
        self.sync_prompts();
    }

    fn shown(&self) -> Shown {
        Shown {
            locked: self.screen_locked(),
            in_mission_control: self.in_mission_control(),
            activities: self.activities(),
            now_playing: self.panel.now_playing.clone(),
        }
    }
}

struct Shown {
    locked: bool,
    in_mission_control: bool,
    activities: Vec<Activity>,
    now_playing: Option<NowPlaying>,
}

impl Shown {
    fn apply(&self, notch: &mut Notch) {
        notch.set_locked(self.locked);
        notch.set_mission_control(self.in_mission_control);
        notch.set_activities(self.activities.clone());
        notch.set_now_playing(self.now_playing.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    #[test]
    fn pointer_display_wins() {
        let seen = |on_its_display: bool, away: f32| Some(Seen { on_its_display, away });
        assert_eq!(pointed_at(&[seen(false, 300.0), seen(true, 1900.0)]), Some(1), "far along an ultrawide");
        assert_eq!(pointed_at(&[seen(true, 0.0), seen(false, 2500.0)]), Some(0));
        assert_eq!(pointed_at(&[seen(false, 900.0), seen(false, 400.0)]), Some(1), "between displays, the nearer");
        assert_eq!(pointed_at(&[None, seen(false, 400.0)]), Some(1), "a window that cannot tell is left out");
        assert_eq!(pointed_at(&[seen(true, 500.0), seen(true, 200.0)]), Some(1), "mirrored displays, the nearer");
        assert_eq!((pointed_at(&[None, None]), pointed_at(&[])), (None, None));
    }

    #[test]
    fn cursor_goes_to_nearest_canvas() {
        let canvas = Rect::from_min_size(Pos2::ZERO, egui::vec2(680.0, 600.0));
        assert_eq!(distance(canvas, pos2(300.0, 20.0)), 0.0);
        assert_eq!(distance(canvas, pos2(300.0, 700.0)), 100.0);
        assert!(distance(canvas, pos2(-1500.0, 20.0)) > distance(canvas, pos2(300.0, 900.0)));
    }
}
