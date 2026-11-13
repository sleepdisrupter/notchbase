//! The Find page: the search field, the results under it and the pins. It keeps the keyboard while it shows, and
//! every edit asks the search service again.

mod cut;
mod field;
mod hold;
mod icons;
mod keys;
mod launch;
mod list;
mod menu;
mod pins;
mod row;
pub(crate) mod samples;
mod select;
mod states;
#[cfg(test)]
mod tests;
mod words;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Rect, pos2, vec2};

use self::field::{FIELD_H, Look};
use self::hold::{Hold, Shown, Taken};
use self::icons::Icons;
use self::keys::Keyboard;
use self::launch::Launch;
use self::list::List;
use self::pins::{PINS_H, Pins};
use self::row::ROW_H;
use self::select::Select;
use super::PageCtx;
use crate::bridge::{Action, FindRow, FindState, FindView, How, ONE_SCREEN, PINNED_AT_MOST, PanelData, Pick, Pinned};
use crate::platform::{Feel, haptic};
use crate::ui::canvas::{Mark, fade};
use crate::ui::field::{self as text_field, Focus};
use crate::ui::menu::Menu;
use crate::ui::rows;
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION};

const GAP: f32 = 8.0;
const COMPACT: f32 = 150.0;
const RULE: f32 = 0.09;
const RULE_INSET: f32 = 10.0;
const LIVE_FROM: f32 = 0.5;
const SHELVED_FOR: Duration = Duration::from_millis(1200);
const SETTLING: f32 = 0.5;
const OFF_FIELD: f32 = 0.45;
const FOLD: SpringConfig = SpringConfig::new(0.2, 1.0);

struct Fold(Spring);

impl Default for Fold {
    fn default() -> Self {
        Self(Spring::new(1.0, FOLD, UNIT_PRECISION))
    }
}

impl Fold {
    fn follow(&mut self, open: bool, snap: bool, dt: f32) {
        let to = if open { 1.0 } else { 0.0 };
        if snap {
            self.0.snap_to(to);
        } else {
            self.0.animate_to(to, FOLD);
            self.0.step(dt);
        }
    }

    fn value(&self) -> f32 {
        self.0.value.clamp(0.0, 1.0)
    }

    fn is_moving(&self) -> bool {
        !self.0.is_settled()
    }
}

#[derive(Default)]
pub struct FindPage {
    focus: Option<Focus>,
    draft: String,
    armed: bool,
    visiting: bool,
    shown: Shown,
    select: Select,
    hold: Hold,
    list: List,
    look: Look,
    icons: Icons,
    keyboard: Keyboard,
    shelved: HashMap<u64, Instant>,
    pinned: Option<FindView>,
    age: f32,
    entered: Option<Instant>,
    launch: Option<Launch>,
    browsing: bool,
    held: usize,
    pins: Pins,
    pinned_now: std::sync::Arc<[FindRow]>,
    menu: Menu<menu::Chose>,
    offered: Option<FindRow>,
    room: f32,
    access_fold: Fold,
    strip_fold: Fold,
}

impl FindPage {
    pub fn height(&self, data: &PanelData) -> f32 {
        self.height_for(self.pinned.as_ref().unwrap_or(data.find)).max(self.room)
    }

    fn height_for(&self, view: &FindView) -> f32 {
        if view.state != FindState::Ready || view.entries == 0 {
            return COMPACT;
        }
        let nudged = if view.nudge { self.access_fold.value() } else { 0.0 };
        let strip = if view.pins.is_empty() { 0.0 } else { PINS_H * self.strip_fold.value() };
        let content = ROW_H * (self.held as f32 + nudged) + strip;
        if content <= 0.0 {
            return FIELD_H;
        }
        FIELD_H + GAP * (content / ROW_H).min(1.0) + content
    }

    #[cfg(test)]
    pub fn browse(&mut self) {
        self.browsing = true;
    }

    fn resting(&self) -> bool {
        self.draft.is_empty() && !self.browsing
    }

    fn hold_its_height(&mut self) {
        if self.resting() {
            self.held = 0;
        } else if self.shown.answers(&self.draft) {
            self.held = self.shown.rows.len().clamp(1, ONE_SCREEN);
        }
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, data: &PanelData) -> Vec<Action> {
        let view = self.pinned.clone().unwrap_or_else(|| data.find.clone());
        self.show(ctx, &view, data.typing)
    }

    pub fn is_animating(&self) -> bool {
        self.launch.is_some_and(|launch| !launch.over())
            || self.pins.is_animating()
            || self.menu.is_animating()
            || self.select.is_animating()
            || self.list.is_animating()
            || self.access_fold.is_moving()
            || self.strip_fold.is_moving()
            || self.icons.is_animating()
            || self.look.is_animating()
            || !self.shelved.is_empty()
    }

    pub fn arm(&mut self) {
        self.armed = true;
    }

    pub fn armed(&self) -> bool {
        self.armed
    }

    pub fn visiting(&self) -> bool {
        self.visiting
    }

    pub fn entered_for(&self) -> Option<f32> {
        self.entered.filter(|_| self.visiting).map(|at| at.elapsed().as_secs_f32())
    }

    pub fn settling(&self) -> bool {
        let lately = self.entered.is_none_or(|at| at.elapsed().as_secs_f32() < SETTLING);
        self.visiting && self.age < SETTLING && lately
    }

    pub fn present(&mut self, current: bool, actions: &mut Vec<Action>) {
        if !current && (self.armed || self.focus.is_some() || self.visiting) {
            self.rest(actions);
        }
    }

    pub fn rest(&mut self, actions: &mut Vec<Action>) {
        if self.pinned.is_some() {
            return;
        }
        self.armed = false;
        (self.menu, self.room) = (Menu::default(), 0.0);
        self.pins.let_go();
        let caret = text_field::release(&mut self.focus, actions).is_some();
        if std::mem::take(&mut self.visiting) {
            if !caret {
                actions.push(Action::Typing(false));
            }
            actions.push(Action::FindShown(false));
        }
        self.drop_quietly();
    }

    pub fn drop_quietly(&mut self) {
        self.armed = false;
        self.visiting = false;
        self.focus = None;
        self.draft.clear();
        self.hold.drop_it();
    }

    pub fn pin(&mut self, view: FindView, selected: Option<usize>) {
        text_field::begin(&mut self.focus, field::id(), &view.query, Rect::NOTHING, &mut Vec::new());
        self.draft = view.query.to_string();
        self.browsing = true;
        self.shown.reset(0);
        let taken = self.shown.take(&view, 0);
        self.held = self.shown.rows.len().clamp(1, ONE_SCREEN);
        self.list.arrive(&self.shown.rows, taken);
        self.list.settle();
        self.select.answer(&self.shown.rows, taken);
        if let Some(index) = selected {
            self.select.pin(index);
        }
        self.pinned = Some(view);
    }

    pub(super) fn show(&mut self, ctx: &mut PageCtx<'_>, view: &FindView, typing: bool) -> Vec<Action> {
        let mut actions = Vec::new();
        let page = ctx.canvas.mark();
        if let Some(launch) = &mut self.launch {
            launch.step(ctx.dt);
        }
        self.launch = self.launch.filter(|launch| !launch.over());
        self.pinned_now = view.pins.clone();
        let awake = ctx.alpha >= LIVE_FROM && self.launch.is_none();
        let live = awake && !self.menu.is_open();
        if std::mem::take(&mut self.armed) {
            self.enter(view, field::rect_in(ctx.rect), &mut actions);
            self.entered = Some(ctx.now);
        }
        self.age += ctx.dt;
        if self.keyboard.lost(self.focus.is_some(), typing) {
            self.focus = None;
        }
        if typing && self.visiting && self.focus.is_none() && self.launch.is_none() {
            text_field::begin(&mut self.focus, field::id(), &self.draft, field::rect_in(ctx.rect), &mut Vec::new());
        }
        self.shelved.retain(|_, until| *until > ctx.now);
        self.hold.expire(ctx.now);
        self.take(view, ctx.dt);
        if let Some(how) = self.hold.due(&self.shown, &self.draft) {
            actions.extend(self.pick(how, self.select.index, ctx.now));
        }
        self.hold_its_height();
        let snap = ctx.alpha < LIVE_FROM;
        self.access_fold.follow(self.draft.is_empty(), snap, ctx.dt);
        self.strip_fold.follow(self.resting(), snap, ctx.dt);
        let mark = ctx.canvas.mark();
        let answered = !self.draft.is_empty() && self.shown.answers(&self.draft);
        let hit = self.shown.rows.get(self.select.index).filter(|_| answered);
        let typed =
            field::paint(ctx, &mut self.look, &mut self.focus, (&self.draft, view), (hit, &mut self.icons), live);
        if view.state == FindState::Off {
            ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, OFF_FIELD));
        }
        if typed.pressed {
            text_field::begin(&mut self.focus, field::id(), &self.draft, field::rect_in(ctx.rect), &mut actions);
        }
        if typed.cleared {
            self.clear(&mut actions);
        }
        if self.menu.is_open() && !typed.edits.is_empty() {
            self.menu.close();
        } else if self.launch.is_none() {
            self.keys(ctx, &typed.edits, &mut actions);
        }
        let top = ctx.rect.top() + FIELD_H + GAP;
        let bottom = (ctx.rect.top() + self.height_for(view)).max(top);
        let area = Rect::from_min_max(pos2(ctx.rect.left(), top), pos2(ctx.rect.right(), bottom));
        if area.height() > 0.0 && view.state == FindState::Ready {
            let rule = Rect::from_min_size(
                pos2(area.left() + RULE_INSET, top - GAP * 0.5),
                vec2(area.width() - 2.0 * RULE_INSET, 1.0),
            );
            ctx.canvas.rect(rule, 0.0, crate::ui::color::white(RULE));
        }
        self.body(ctx, view, area, live, &mut actions);
        let nudged = if view.nudge { ROW_H * self.access_fold.value() } else { 0.0 };
        self.lift(ctx, page, area.top() + nudged);
        self.offer(ctx, bottom, awake, &mut actions);
        actions
    }

    pub(super) fn offer_menu(&mut self, ctx: &PageCtx<'_>, index: usize) {
        let Some(row) = self.shown.rows.get(index).cloned() else { return };
        let at = ctx.ui.input(|input| input.pointer.interact_pos()).unwrap_or(ctx.rect.center());
        self.menu_for(row, at);
    }

    fn menu_for(&mut self, row: FindRow, at: egui::Pos2) {
        let pinned = self.pinned_now.iter().any(|pin| pin.path == row.path);
        self.menu.open(at, menu::items(&row, pinned, self.pinned_now.len() >= PINNED_AT_MOST));
        self.offered = Some(row);
    }

    fn offer(&mut self, ctx: &mut PageCtx<'_>, bottom: f32, awake: bool, actions: &mut Vec<Action>) {
        let reach = self.menu.reach();
        self.room = reach.map_or(0.0, |reach| reach - ctx.rect.top());
        let within =
            Rect::from_min_max(ctx.rect.min, pos2(ctx.rect.right(), reach.map_or(bottom, |reach| reach.max(bottom))));
        let Some(chose) = self.menu.show(ctx.ui, ctx.canvas, within, ctx.dt, awake) else { return };
        let Some(row) = self.offered.take() else { return };
        let row = &row;
        match chose {
            menu::Chose::Pin => actions.push(Action::FindPin(Pinned::of(row))),
            menu::Chose::Unpin => actions.push(Action::FindUnpin(row.path.to_path_buf())),
            menu::Chose::Reveal | menu::Chose::AsAdmin => {
                haptic(Feel::Snap);
                let how = if chose == menu::Chose::Reveal { How::Reveal } else { How::AsAdmin };
                let pick = Pick {
                    path: row.path.to_path_buf(),
                    typed: self.draft.clone(),
                    rank: u8::MAX,
                    how,
                    predicted: false,
                };
                actions.push(Action::FindPick(pick));
            }
        }
    }

    fn strip(&mut self, ctx: &mut PageCtx<'_>, area: Rect, live: bool, actions: &mut Vec<Action>) {
        let pins = self.pinned_now.clone();
        let Some(chose) = self.pins.paint(ctx, area, &pins, &mut self.icons, live) else { return };
        let of = |key: u64| pins.iter().find(|pin| pin.key == key);
        match chose {
            pins::Chose::Open(key, from) => {
                let Some(pin) = of(key) else { return };
                haptic(Feel::Snap);
                if pin.app {
                    self.launch = Some(Launch::from(pin.key, (from, pins::EMBLEM / row::ICON), ctx.now));
                }
                let rank = pins.iter().position(|held| held.key == key).and_then(|at| u8::try_from(at).ok());
                let pick = Pick {
                    path: pin.path.to_path_buf(),
                    typed: String::new(),
                    rank: rank.unwrap_or(u8::MAX),
                    how: How::Open,
                    predicted: false,
                };
                actions.push(Action::FindPick(pick));
            }
            pins::Chose::Menu(key, at) => {
                if let Some(pin) = of(key).cloned() {
                    self.menu_for(pin, at);
                }
            }
            pins::Chose::Moved(order) => {
                let paths = order.iter().filter_map(|key| of(*key)).map(|pin| pin.path.to_path_buf());
                actions.push(Action::FindPinsOrder(paths.collect()));
            }
            pins::Chose::Out(key) => {
                // Pulled out of the strip: the same file drag a result row starts, so it can land in the dock,
                // Finder or any app.
                let Some(pin) = of(key) else { return };
                haptic(Feel::Tick);
                let rank = pins.iter().position(|held| held.key == key).and_then(|at| u8::try_from(at).ok());
                let pick = Pick {
                    path: pin.path.to_path_buf(),
                    typed: String::new(),
                    rank: rank.unwrap_or(u8::MAX),
                    how: How::Drag,
                    predicted: false,
                };
                actions.push(Action::FindPick(pick));
            }
        }
    }

    fn lift(&mut self, ctx: &mut PageCtx<'_>, page: Mark, rows_from: f32) {
        let Some(launch) = &mut self.launch else { return };
        let left = launch.left_of_the_rest();
        ctx.canvas.since(page).iter_mut().for_each(|shape| fade(shape, left));
        let listed = self.shown.rows.iter().position(|row| row.key == launch.key);
        if let (None, Some(index)) = (launch.home, listed) {
            let top = rows_from + index as f32 * ROW_H - self.list.scrolled();
            let rect = Rect::from_min_size(pos2(ctx.rect.left(), top), vec2(ctx.rect.width(), ROW_H));
            launch.home = Some(row::emblem_center(rect));
        }
        let pins = self.pinned_now.clone();
        let row = listed.map(|index| &self.shown.rows[index]).or_else(|| pins.iter().find(|pin| pin.key == launch.key));
        let (Some(home), Some(row)) = (launch.home, row) else { return };
        let pose = launch.pose(home, pos2(ctx.rect.center().x, ctx.rect.top()));
        let most = if launch.grown > 1.0 { launch.grown * launch::PIN_SWELLS_TO } else { f32::MAX };
        let scale = (pose.scale * launch.grown).min(most);
        let mark = ctx.canvas.mark();
        for (reach, light) in launch::HALO {
            let radius = row::ICON * scale * reach;
            ctx.canvas.circle(pose.at, radius, egui::Color32::WHITE.gamma_multiply(light * pose.glow));
        }
        row::emblem(ctx, row, &mut self.icons, pose.at, scale);
        ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, pose.alpha));
    }

    fn enter(&mut self, view: &FindView, rect: Rect, actions: &mut Vec<Action>) {
        self.visiting = true;
        self.launch = None;
        (self.menu, self.room) = (Menu::default(), 0.0);
        (self.browsing, self.held) = (false, 0);
        self.age = 0.0;
        self.draft.clear();
        self.shown.reset(view.answered);
        self.select.reset();
        self.select.unplace();
        self.list.clear();
        self.hold.drop_it();
        actions.push(Action::FindShown(true));
        text_field::begin(&mut self.focus, field::id(), "", rect, actions);
        actions.push(Action::FindQuery(String::new()));
    }

    fn take(&mut self, view: &FindView, dt: f32) {
        let taken = self.shown.take(view, self.select.index);
        if taken != Taken::Nothing {
            self.list.arrive(&self.shown.rows, taken);
            self.select.answer(&self.shown.rows, taken);
            if taken == Taken::Fresh {
                self.list.reveal(self.select.index, self.shown.rows.len());
            }
        }
        self.icons.step(&self.shown.rows, &self.pinned_now, dt);
    }

    fn body(&mut self, ctx: &mut PageCtx<'_>, view: &FindView, area: Rect, live: bool, actions: &mut Vec<Action>) {
        match view.state {
            FindState::Off => actions.extend(states::off(ctx, area, live)),
            FindState::Ready if view.entries > 0 => {
                let mut list = area;
                let access_shown = self.access_fold.value();
                if view.nudge && access_shown > 0.0 {
                    let access = Rect::from_min_size(area.min, vec2(area.width(), ROW_H));
                    let mark = ctx.canvas.mark();
                    actions.extend(states::access(ctx, access, live && self.draft.is_empty()));
                    list.min.y += ROW_H * access_shown;
                    rows::reveal(
                        ctx.canvas,
                        mark,
                        Rect::from_min_max(area.min, pos2(area.right(), list.min.y)),
                        access_shown,
                    );
                }
                let strip_shown = self.strip_fold.value();
                if strip_shown > 0.0 && !self.pinned_now.is_empty() {
                    let strip = Rect::from_min_size(list.min, vec2(list.width(), PINS_H));
                    let mark = ctx.canvas.mark();
                    self.strip(ctx, strip, live && self.resting(), actions);
                    let top = list.min;
                    list.min.y += PINS_H * strip_shown;
                    rows::reveal(
                        ctx.canvas,
                        mark,
                        Rect::from_min_max(top, pos2(list.right(), list.min.y)),
                        strip_shown,
                    );
                }
                if self.resting() {
                    self.select.unplace();
                } else if self.shown.rows.is_empty() && self.shown.answers(&self.draft) && !self.draft.is_empty() {
                    states::none(ctx, list, view);
                } else {
                    self.paint_rows(ctx, list, live, actions);
                }
            }
            FindState::Starting | FindState::Ready => states::starting(ctx, area, view.entries),
        }
    }

    fn pick(&mut self, how: How, index: usize, now: Instant) -> Option<Action> {
        let row = self.shown.rows.get(index)?;
        // Only an app runs as administrator: for a file it is the same as opening it.
        let how = if how == How::AsAdmin && !row.app { How::Open } else { how };
        match how {
            How::Open | How::AsAdmin | How::Reveal => haptic(Feel::Snap),
            How::Drag => haptic(Feel::Tick),
            How::Shelve => {
                haptic(Feel::Tick);
                self.shelved.insert(row.key, now + SHELVED_FOR);
            }
        }
        if matches!(how, How::Open | How::AsAdmin) && row.app {
            self.launch = Some(Launch::of(row.key, now));
        }
        let rank = u8::try_from(index).unwrap_or(u8::MAX);
        let pick =
            Pick { path: row.path.to_path_buf(), typed: self.draft.clone(), rank, how, predicted: row.predicted };
        Some(Action::FindPick(pick))
    }
}
