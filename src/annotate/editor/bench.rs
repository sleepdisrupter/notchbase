use std::sync::Arc;

use egui::epaint::ClippedPrimitive;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2};

use super::keys::forget_repeated_strikes;
use super::layout::Layout;
use super::{Editor, Outcome};
use crate::annotate::{Brush, Fit};
use crate::capture::Screenshot;
use crate::ui::snapshot::Camera;

pub(super) const SCALE: f32 = 2.0;
const DT: f32 = 1.0 / 60.0;
const SETTLING_FRAMES: usize = 120;

pub(super) struct Bench {
    pub(super) camera: Camera,
    pub(super) editor: Editor,
    size: Vec2,
}

impl Bench {
    pub(super) fn new(shot: Screenshot, brush: Brush, size: Vec2) -> Self {
        let editor = Editor::new(Arc::new(shot), brush).expect("a drawable screenshot");
        let mut bench = Self { camera: Camera::new(size, SCALE), editor, size };
        bench.frost(crate::platform::BLURS_BEHIND);
        bench.settle();
        bench
    }

    pub(super) fn frost(&mut self, frosted: bool) {
        self.editor.frosted = frosted;
        self.camera.sits_on_frost(frosted);
    }

    pub(super) fn frame(&mut self, events: Vec<Event>) -> Vec<Outcome> {
        let mut outcomes = Vec::new();
        self.camera.pass(events, |ui| outcomes = self.editor.ui(ui, DT));
        outcomes
    }

    pub(super) fn tessellated(&mut self) -> Vec<ClippedPrimitive> {
        self.camera.pass(Vec::new(), |ui| drop(self.editor.ui(ui, DT)))
    }

    pub(super) fn settle(&mut self) -> Vec<Outcome> {
        (0..SETTLING_FRAMES).flat_map(|_| self.frame(Vec::new())).collect()
    }

    pub(super) fn layout(&self) -> Layout {
        Layout::of(Rect::from_min_size(Pos2::ZERO, self.size))
    }

    pub(super) fn fit(&self) -> Fit {
        Fit::new(self.editor.image_size(), self.layout().stage, SCALE)
    }

    pub(super) fn hover(&mut self, at: Pos2) -> Vec<Outcome> {
        self.frame(vec![Event::PointerMoved(at)])
    }

    pub(super) fn press(&mut self, at: Pos2) -> Vec<Outcome> {
        let mut outcomes = self.hover(at);
        outcomes.extend(self.frame(vec![button(at, true)]));
        outcomes
    }

    pub(super) fn release(&mut self, at: Pos2) -> Vec<Outcome> {
        let mut outcomes = self.frame(vec![button(at, false)]);
        outcomes.extend(self.frame(Vec::new()));
        outcomes
    }

    pub(super) fn click(&mut self, at: Pos2) -> Vec<Outcome> {
        let mut outcomes = self.press(at);
        outcomes.extend(self.release(at));
        outcomes
    }

    pub(super) fn drag(&mut self, path: &[Pos2]) -> Vec<Outcome> {
        let (Some(first), Some(last)) = (path.first(), path.last()) else { return Vec::new() };
        let mut outcomes = self.press(*first);
        for step in path {
            outcomes.extend(self.frame(vec![Event::PointerMoved(*step)]));
        }
        outcomes.extend(self.release(*last));
        outcomes
    }

    pub(super) fn key(&mut self, key: Key, modifiers: Modifiers) -> Vec<Outcome> {
        let mut outcomes = self.hold(key, modifiers);
        outcomes.extend(self.let_go(key, modifiers));
        outcomes
    }

    pub(super) fn hold(&mut self, key: Key, modifiers: Modifiers) -> Vec<Outcome> {
        self.frame(vec![key_event(key, modifiers, true)])
    }

    pub(super) fn let_go(&mut self, key: Key, modifiers: Modifiers) -> Vec<Outcome> {
        self.frame(vec![key_event(key, modifiers, false)])
    }

    pub(super) fn type_text(&mut self, text: &str) -> Vec<Outcome> {
        self.frame(vec![Event::Text(text.to_owned())])
    }

    pub(super) fn strike(&mut self, key: Key, text: &str) -> Vec<Outcome> {
        let mut outcomes = self.frame(vec![key_event(key, Modifiers::NONE, true), Event::Text(text.to_owned())]);
        outcomes.extend(self.let_go(key, Modifiers::NONE));
        outcomes
    }

    pub(super) fn paste(&mut self, text: &str) -> Vec<Outcome> {
        self.frame(vec![Event::Paste(text.to_owned())])
    }

    pub(super) fn repeat_from_the_window(&mut self, key: Key, modifiers: Modifiers) -> Vec<Outcome> {
        let mut heard = Vec::new();
        let already_heard = heard.len();
        heard.push(key_event(key, modifiers, true));
        forget_repeated_strikes(&mut heard, already_heard);
        self.frame(heard)
    }
}

pub(super) fn key_event(key: Key, modifiers: Modifiers, pressed: bool) -> Event {
    Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }
}

pub(super) fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}
