use std::time::{Duration, Instant};

use egui::{Event, Id, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};

use super::PageCtx;
use crate::ui::canvas::Canvas;
use crate::ui::font;

pub(crate) const PAGE: Rect = Rect::from_min_max(pos2(16.0, 60.0), pos2(356.0, 640.0));
const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(400.0, 700.0));
pub(crate) const DT: f32 = 1.0 / 60.0;

pub(crate) struct Harness {
    ctx: egui::Context,
    pub(crate) now: Instant,
}

impl Harness {
    pub(crate) fn new() -> Self {
        let ctx = egui::Context::default();
        font::install(&ctx);
        Self { ctx, now: Instant::now() }
    }

    pub(crate) fn frame<T>(&mut self, events: Vec<Event>, mut body: impl FnMut(&mut PageCtx<'_>) -> T) -> T {
        self.now += Duration::from_secs_f32(DT);
        let now = self.now;
        let input = egui::RawInput { screen_rect: Some(SCREEN), events, ..Default::default() };
        let mut result = None;
        let mut output = self.ctx.run_ui(input, |ui| {
            let mut canvas = Canvas::new(ui.painter());
            let mut ctx = PageCtx { ui, canvas: &mut canvas, rect: PAGE, alpha: 1.0, dt: DT, now };
            result = Some(body(&mut ctx));
        });
        output.textures_delta.clear();
        result.expect("the frame ran")
    }

    pub(crate) fn rect_of(&self, id: Id) -> Rect {
        self.ctx.read_response(id).map(|response| response.rect).unwrap_or_else(|| panic!("{id:?} was drawn"))
    }

    pub(crate) fn drawn(&self, id: Id) -> bool {
        self.ctx.read_response(id).is_some()
    }
}

pub(crate) fn press(at: Pos2) -> Vec<Event> {
    vec![Event::PointerMoved(at), button(at, true)]
}

pub(crate) fn release(at: Pos2) -> Vec<Event> {
    vec![button(at, false)]
}

pub(crate) fn typed(text: &str) -> Event {
    Event::Text(text.to_owned())
}

pub(crate) fn key(key: egui::Key) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

pub(crate) fn away() -> Pos2 {
    SCREEN.right_bottom() - vec2(4.0, 4.0)
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}
