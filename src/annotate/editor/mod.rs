//! The editor that fills the annotate window. Each frame it reads the input, changes the sheet and paints the picture,
//! the toolbar and the line or words in progress, then reports what changed.

mod act;
#[cfg(test)]
mod bench;
mod command;
mod keys;
mod layout;
mod paint;
mod pen;
mod picture;
mod sense;
#[cfg(test)]
mod snapshot;
mod status;
#[cfg(test)]
mod tests;
mod tool;
mod toolbar;
mod touch;
mod typing;

use std::sync::Arc;

use egui::{Event, Pos2, Rect};

use crate::capture::Screenshot;
use crate::platform;
use crate::ui::spring::{Spring, UNIT_PRECISION};
use crate::ui::{glass, motion};

use self::command::Command;
use self::keys::Busy;
use self::layout::Layout;
use self::pen::Pen;
use self::picture::Picture;
use self::status::Status;
use self::tool::Tool;
use self::toolbar::Toolbar;
use self::typing::{Page, Setter, Typing};
use super::{Brush, Fit, Job, PixelRect, Sheet};

pub(super) use self::keys::forget_repeated_strikes;
pub use self::layout::CHROME;

/// What the window should do after a frame: copy a new revision, remember the brush, or close.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Changed(u64),
    Remember(Brush),
    Close,
}

pub struct Editor {
    sheet: Sheet,
    brush: Brush,
    remembered: Brush,
    picture: Picture,
    tool: Tool,
    pen: Option<Pen>,
    typing: Option<Typing>,
    setter: Option<Setter>,
    page: Option<Page>,
    status: Status,
    toolbar: Toolbar,
    ring: Spring,
    handle: Rect,
    frosted: bool,
    title_bar: f32,
}

#[derive(Default)]
struct Turn {
    fit: Option<Fit>,
    outcomes: Vec<Outcome>,
    dirty: PixelRect,
}

impl Editor {
    pub fn new(shot: Arc<Screenshot>, brush: Brush) -> Option<Self> {
        Some(Self {
            sheet: Sheet::new(shot)?,
            brush,
            remembered: brush,
            picture: Picture::Waiting,
            tool: Tool::default(),
            pen: None,
            typing: None,
            setter: None,
            page: None,
            status: Status::default(),
            toolbar: Toolbar::new(brush, Tool::default()),
            ring: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            handle: Rect::NOTHING,
            frosted: platform::BLURS_BEHIND,
            title_bar: 0.0,
        })
    }

    pub fn sits_under_a_title_bar(&mut self, height: f32) {
        self.title_bar = height;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, dt: f32) -> Vec<Outcome> {
        let layout = Layout::of(ui.ctx().content_rect());
        self.handle = layout.title;
        self.step(dt);
        glass::set_seen_through(ui.ctx(), if self.frosted { 1.0 } else { 0.0 });
        self.picture.load(ui.ctx(), &self.sheet);
        let fit = Fit::new(self.sheet.size(), layout.stage, ui.ctx().pixels_per_point());
        let (before, mut turn) = (self.sheet.revision(), Turn { fit: Some(fit), ..Turn::default() });
        let over_picture = self.sense(ui, &layout, fit, &mut turn);
        let outcomes = self.conclude(before, turn);
        self.ring.animate_to(if over_picture.is_some() { 1.0 } else { 0.0 }, motion::HOVER);
        self.paint(ui, &layout, fit, over_picture);
        outcomes
    }

    pub fn close(&mut self) -> Vec<Outcome> {
        let (before, mut turn) = (self.sheet.revision(), Turn::default());
        self.perform(Command::Close, &mut turn);
        self.conclude(before, turn)
    }

    pub fn finish_what_is_in_progress(&mut self, unread: &mut Vec<Event>) -> Vec<Outcome> {
        let (before, mut turn) = (self.sheet.revision(), Turn::default());
        self.finish_the_line(&mut turn);
        self.read_what_no_frame_has(unread, &mut turn);
        self.finish_the_words(&mut turn);
        self.conclude(before, turn)
    }

    pub fn copied(&mut self, revision: u64) {
        self.status.holds(revision, self.sheet.revision());
    }

    pub fn not_copied(&mut self, revision: u64) {
        self.status.refused(revision);
    }

    pub fn revision(&self) -> u64 {
        self.sheet.revision()
    }

    pub fn is_animating(&self) -> bool {
        !(self.toolbar.is_settled() && self.status.is_settled() && self.ring.is_settled())
    }

    pub fn job(&self) -> Job {
        Job::of(&self.sheet)
    }

    pub fn image_size(&self) -> [usize; 2] {
        self.sheet.size()
    }

    pub fn shows(&self, shot: &Arc<Screenshot>) -> bool {
        Arc::ptr_eq(self.sheet.base(), shot)
    }

    pub fn moves_the_window_from(&self, point: Pos2) -> bool {
        self.handle.contains(point)
    }

    fn conclude(&mut self, before: u64, mut turn: Turn) -> Vec<Outcome> {
        self.picture.refresh(&self.sheet, turn.dirty);
        if self.sheet.revision() != before {
            turn.outcomes.insert(0, Outcome::Changed(self.sheet.revision()));
        }
        turn.outcomes
    }

    fn step(&mut self, dt: f32) {
        self.toolbar.step(self.brush, self.tool, dt);
        self.status.step(dt);
        self.ring.step(dt);
    }

    fn busy(&self) -> Busy {
        match (&self.typing, &self.pen) {
            (Some(_), _) => Busy::Typing,
            (None, Some(_)) => Busy::Drawing,
            (None, None) => Busy::Idle,
        }
    }

    fn is_writing(&self) -> bool {
        self.typing.as_ref().is_some_and(|typing| typing.would_change(&self.sheet))
    }
}
