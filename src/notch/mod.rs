//! The notch itself. A `Notch` holds everything one notch window shows, from the closed shape to the open
//! panel, and `ui` draws a frame of it and returns the `Action`s that were pressed. The folders hold its parts:
//! how it moves, the panel, live activities, notices, questions and music.

mod activity;
mod ask;
mod drop_zone;
mod frame;
mod geometry;
mod keyline;
pub mod live;
mod moves;
mod music;
pub mod notice;
mod panel;
mod pointer;
mod preview;
mod state;
#[cfg(test)]
mod tests;
mod zones;

use std::time::Instant;

use egui::{Pos2, Rect, Vec2};
use notchbase_sdk::now_ms;
use notchbase_sdk::protocol::NoticeNode;

use crate::bridge::{Action, Ask, Page, PanelData, Tab, ToolSlot};
use crate::platform::Style;
use crate::ui::element::{Env, Textures};
use crate::ui::liquid::Liquid;
use crate::ui::meter::Meter;
use crate::ui::motion::{self, Clock};
use crate::ui::spring::{POINT_PRECISION, Spring, UNIT_PRECISION};

use activity::{Activity, ActivityKind};
use geometry::{CANVAS, DEFAULT_NOTCH, Morph};
use live::Live;
use moves::attract::{ATTRACT, LEAN, resting_attract};
use moves::transition::Motion;
use music::{Music, Strip};
use notice::ToolNotice;
use panel::header::LiquidPill;
use panel::pager::Pager;
use panel::pages::Pages;
use preview::Preview;

pub use drop_zone::layouts::Pick;
pub use preview::sample_thumbnail;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Idle,
    Peek,
    Expanded,
    Activity,
    Drop,
    Ask,
}

pub struct Notch {
    style: Style,
    notch: Vec2,
    preview: Option<Preview>,

    mode: Mode,
    pinned: bool,
    hover_armed: bool,
    awake: bool,
    scared: bool,
    pressed_near: Option<bool>,
    feared: Option<Pos2>,
    still_at: Option<Pos2>,
    dwell_until: Option<Instant>,
    peek_until: Option<Instant>,
    leave_until: Option<Instant>,

    activity: Activity,
    ask: Option<Ask>,
    card: bool,
    ask_layout: Option<ask::AskLayout>,
    card_shown: Option<ask::Shown>,
    answering: ask::Answering,
    drop_files: usize,
    drop_face: drop_zone::WellFace,
    drop_video: Option<crate::webvideo::Site>,
    video_aim: Option<crate::webvideo::Kind>,
    video: live::video::Reel,
    downloading: live::downloading::Downloading,
    picking: bool,
    hot_pick: Option<Pick>,
    away: bool,
    usual_pick: Option<Pick>,
    capturing_keys: bool,
    shelf_filled: bool,
    news_due: bool,
    welcome_due: bool,
    live: Option<Live>,
    live_leaving: Option<live::Leaving>,
    spin: crate::ui::agent_mark::Spin,
    said: live::agents::Said,
    app_activities: Vec<crate::bridge::Activity>,
    activities: Vec<crate::bridge::Activity>,
    sides: Vec<live::sides::Side>,
    live_second: Option<u64>,
    live_controls: Option<Rect>,
    music: Option<Music>,
    music_leaving: Option<Music>,
    strip_gone: Option<activity::volume::Indicator>,
    music_until: Option<Instant>,
    strip: Option<Strip>,
    liquid: Liquid,
    level_meter: Meter,
    panel_alpha: f32,
    clock: Clock,
    textures: Textures,

    tabs: Vec<Tab>,
    pager: Pager,
    pages: Pages,

    morph: Morph,
    panel_fade: Spring,
    activity_fade: Spring,
    drop_fade: Spring,
    ask_fade: Spring,
    live_fade: Spring,
    live_hover: Spring,
    music_fade: Spring,
    dance: Spring,
    attract: Spring,
    lean: Spring,
    hug: Spring,
    dock: Spring,
    jolt: Spring,
    gulp: Spring,
    swallowing: Option<Instant>,
    flee: Spring,
    fear: Spring,
    surge: Spring,
    compact: Spring,
    dodge: Spring,
    split_shown: bool,
    menu_edge: Option<f32>,
    locked: bool,
    /// Mission Control or App Exposé is showing: the notch stays closed and lets the pointer through to it.
    in_mission_control: bool,
    unlocked_at: Option<Instant>,
    lock_leaving: Option<live::lock::Leaving>,
    taking_over: bool,
    tab_pill: LiquidPill,
    tab_rows: crate::ui::rows::Rows<String, Tab>,
    switched_from: Page,
    switched_at: Option<Pos2>,
}

impl Notch {
    pub fn new(style: Style, notch_size: Option<Vec2>) -> Self {
        Self::with_preview(style, notch_size, Preview::from_env())
    }

    fn with_preview(style: Style, notch_size: Option<Vec2>, preview: Option<Preview>) -> Self {
        let mut notch = Self::at_rest(style, notch_size.unwrap_or(DEFAULT_NOTCH));
        if let Some(preview) = preview {
            notch.pin_preview(preview);
        }
        notch
    }

    fn at_rest(style: Style, notch: Vec2) -> Self {
        let fade = || Spring::new(0.0, motion::HOVER, UNIT_PRECISION);
        let tabs = Tab::defaults();
        let mut at_rest = Self {
            style,
            notch,
            preview: None,
            mode: Mode::Idle,
            pinned: false,
            hover_armed: true,
            awake: false,
            scared: false,
            pressed_near: None,
            feared: None,
            still_at: None,
            dwell_until: None,
            peek_until: None,
            leave_until: None,
            activity: Activity::default(),
            ask: None,
            card: false,
            ask_layout: None,
            card_shown: None,
            answering: ask::Answering::default(),
            drop_files: 0,
            drop_face: drop_zone::WellFace::default(),
            drop_video: None,
            video_aim: None,
            video: live::video::Reel::default(),
            downloading: live::downloading::Downloading::default(),
            picking: false,
            hot_pick: None,
            away: false,
            usual_pick: None,
            capturing_keys: false,
            shelf_filled: false,
            news_due: false,
            welcome_due: false,
            live: None,
            live_leaving: None,
            spin: crate::ui::agent_mark::Spin::default(),
            said: live::agents::Said::default(),
            app_activities: Vec::new(),
            activities: Vec::new(),
            sides: Vec::new(),
            live_second: None,
            live_controls: None,
            music: None,
            music_leaving: None,
            strip_gone: None,
            music_until: None,
            strip: None,
            liquid: Liquid::default(),
            level_meter: Meter::default(),
            panel_alpha: 0.0,
            clock: Clock::default(),
            textures: Textures::default(),
            pager: Pager::new(panel::pager::start_page(&tabs), &tabs),
            tabs,
            pages: Pages::default(),
            morph: Morph::default(),
            panel_fade: fade(),
            activity_fade: fade(),
            drop_fade: fade(),
            ask_fade: fade(),
            live_fade: fade(),
            live_hover: Spring::new(0.0, live::HOVER, UNIT_PRECISION),
            music_fade: fade(),
            dance: fade(),
            attract: Spring::new(resting_attract(style), ATTRACT, UNIT_PRECISION),
            lean: Spring::new(0.0, LEAN, POINT_PRECISION),
            hug: Spring::new(0.0, ATTRACT, UNIT_PRECISION),
            dock: Spring::new(0.0, moves::hug::GLIDE, POINT_PRECISION),
            jolt: Spring::new(0.0, motion::HOVER, POINT_PRECISION),
            gulp: Spring::new(0.0, motion::HOVER, POINT_PRECISION),
            swallowing: None,
            flee: Spring::new(0.0, moves::flee::FLEE, POINT_PRECISION),
            fear: Spring::new(0.0, moves::flee::FEAR, UNIT_PRECISION),
            surge: Spring::new(0.0, moves::surge::SURGE_OUT, UNIT_PRECISION),
            compact: fade(),
            dodge: Spring::new(0.0, moves::dodge::SLIDE, POINT_PRECISION),
            split_shown: false,
            menu_edge: None,
            locked: false,
            in_mission_control: false,
            unlocked_at: None,
            lock_leaving: None,
            taking_over: false,
            tab_pill: LiquidPill::default(),
            tab_rows: crate::ui::rows::Rows::default(),
            switched_from: Page::Tools,
            switched_at: None,
        };
        at_rest.pager.remeasure(at_rest.pages.estimated_heights());
        at_rest.morph.snap_to(at_rest.idle_geometry());
        at_rest
    }

    pub fn canvas_size(&self) -> Vec2 {
        CANVAS
    }

    pub fn canvas_rect(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, CANVAS)
    }

    pub fn tick(&mut self, now: Instant, cursor: Option<Pos2>) -> bool {
        if self.preview.is_some() {
            return false;
        }
        let cursor = cursor.filter(|_| !self.locked && !self.in_mission_control);
        self.hold_hovered_notice(now, cursor);
        let expired = self.expire_activity(now) | self.expire_music(now) | self.expire_strip(now) | self.lock_tick(now);
        let calm = cursor.filter(|_| !self.shies());
        let sided = self.hover_sides(calm);
        let lifted = self.hover_live(calm);
        let moved = self.follow_pointer(now, calm) || sided || lifted;
        let attracted = self.attract_toward(calm) | self.flee_from(cursor);
        let summoned = self.summon_music(calm, now);
        let ticked = self.live_tick() | self.music_tick(now);
        attracted || summoned || expired || moved || ticked
    }

    pub fn wants_input(&self, cursor: Pos2) -> bool {
        if self.locked || self.in_mission_control {
            return false;
        }
        (self.mode != Mode::Idle && self.input_zone().contains(cursor))
            || self.awaits_click(cursor)
            || self.side_takes_input(cursor)
            || self.hovers_live_controls(Some(cursor))
    }

    pub fn is_animating(&self) -> bool {
        self.mode == Mode::Drop
            || self.springs().any(|spring| !spring.is_settled())
            || self.pages.is_animating()
            || (self.capturing_keys && self.panel_alpha > 0.0)
            || self.music_is_moving()
            || self.indicator_is_moving()
            || self.pulse_is_moving()
            || self.sides_moving()
            || self.live_moves()
            || self.activity.leaving.is_some()
            || self.tab_rows.is_animating()
            || self.surging()
            || self.flowing()
            || self.video_moving()
            || self.downloads_move()
            || self.lock_opening()
            || self.launch_moves()
    }

    pub fn next_wakeup(&self) -> Option<Instant> {
        if self.preview.is_some() {
            return None;
        }
        let now = Instant::now();
        let live = self.live_wakeup(now);
        let music = self.music_wakeup(now);
        let hides = [self.music_until, self.strip_wakeup()];
        let waits = [self.dwell_until, self.peek_until, self.leave_until, self.activity.until];
        let lock = self.lock_wakeup();
        waits.into_iter().chain([live, music, lock]).chain(hides).flatten().min()
    }

    pub fn is_collapsed(&self) -> bool {
        self.mode == Mode::Idle
            && !self.activity.is_live()
            && self
                .morph
                .springs()
                .into_iter()
                .chain([&self.panel_fade, &self.activity_fade, &self.attract])
                .all(Spring::is_settled)
    }

    pub fn set_mission_control(&mut self, showing: bool) {
        if self.preview.is_some() || self.in_mission_control == showing {
            return;
        }
        self.in_mission_control = showing;
        self.hover_armed = false;
        if showing {
            self.pinned = false;
            if matches!(self.mode, Mode::Expanded | Mode::Peek) {
                self.collapse();
            }
        }
    }

    pub fn collapse(&mut self) {
        if self.preview.is_some() {
            return;
        }
        self.activity.until = None;
        self.hover_armed = false;
        self.set_mode(Mode::Idle, Motion::Dismiss);
    }

    pub fn show_notice(&mut self, slot: ToolSlot, notice: NoticeNode) {
        self.start_activity(ActivityKind::Tool(ToolNotice::new(slot, notice)), Instant::now());
    }

    pub fn show_message(&mut self, text: &str) {
        self.start_activity(ActivityKind::Message(text.to_owned()), Instant::now());
    }

    pub fn set_drop_hover(&mut self, files: Option<usize>) {
        if self.preview.is_some() || (self.locked && files.is_some()) {
            return;
        }
        match files {
            Some(count) => {
                self.drop_files = count;
                self.picking = false;
                self.hot_pick = None;
                if self.mode != Mode::Drop {
                    self.set_mode(Mode::Drop, Motion::Open);
                }
            }
            None if self.mode == Mode::Drop => {
                self.picking = false;
                self.hot_pick = None;
                self.return_to_rest();
            }
            None => {}
        }
    }

    pub fn is_drop_target(&self) -> bool {
        self.mode == Mode::Drop
    }

    pub fn drop_catch_zone(&self) -> Rect {
        self.catch_zone()
    }

    pub fn open_page(&mut self, page: Page) {
        if self.preview.is_some() || self.locked {
            return;
        }
        if self.mode == Mode::Expanded {
            self.switch_to(page);
        } else {
            self.pager.show(page);
            self.hover_armed = true;
            self.set_mode(Mode::Expanded, Motion::Open);
        }
    }

    pub fn open_tool_settings(&mut self, id: &str) {
        if self.preview.is_some() {
            return;
        }
        self.pages.show_tool_settings(id);
        self.open_page(Page::Settings);
    }

    pub fn pinned(&self) -> bool {
        self.pinned
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned && !self.locked && !self.in_mission_control;
        self.leave_until = None;
        if self.pinned && self.mode != Mode::Expanded && self.preview.is_none() {
            self.set_mode(Mode::Expanded, Motion::Open);
        }
    }

    pub fn expanded(&self) -> bool {
        self.mode == Mode::Expanded
    }

    pub fn visible_page(&self) -> Option<Page> {
        (self.mode == Mode::Expanded && self.panel_alpha > 0.3).then_some(self.pager.page)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, data: &PanelData, dt: f32) -> Vec<Action> {
        let painter = ui.painter().clone();
        let env = Env {
            style: self.style,
            images: data.images,
            shortcuts: data.shortcuts,
            prompt: self.pages.capture.prompt(),
            now_ms: now_ms(),
        };
        self.advance(ui.ctx(), &painter, data, &env, dt);
        crate::ui::glass::set_seen_through(ui.ctx(), self.glass());
        crate::ui::scroll::track(ui);
        let g = self.morph.current();
        let silhouette = self.silhouette();
        self.paint_sides_keyline(&painter, &silhouette);
        self.paint_body(&painter, &silhouette, self.openness(g));
        self.paint_sides_shell(&painter, &silhouette);
        self.paint_video_rim(&painter, &silhouette);
        self.open_on_click(ui);
        let body = silhouette.rect();
        let mut actions = self.panel_ui(ui, &painter, data, &env, body, dt);
        let noticed = self.activity_ui(ui, &env, g, body);
        let lived = self.live_ui(ui, &env, g, body);
        self.lock_leaving_ui(ui, g, body);
        let pressed = self.sides_ui(ui, &env, &silhouette, data.tools);
        self.music_ui(&painter, g, body);
        self.drop_ui(&painter, g, body);
        actions.extend(self.ask_ui(ui, &painter, g, body));
        actions.extend(noticed);
        actions.extend(pressed);
        actions.extend(lived);
        actions
    }
}
