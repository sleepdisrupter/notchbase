//! The list a dropdown opens beneath it: up to four rows that scroll a row at a time, with the page growing to fit.

use egui::{Id, Rect, pos2, vec2};
use notchbase_sdk::protocol::{Choice, HandlerId, UiValue};

use crate::ui::canvas::{self, Canvas};
use crate::ui::metrics::GAP;
use crate::ui::spring::{Spring, smoothstep};
use crate::ui::{controls, motion};

use super::Scene;
use super::style::glyph;

const ROW: f32 = 28.0;
const MAX_ROWS: usize = 4;
const PRECISION: f32 = 0.001;

#[derive(Clone)]
struct Anchor {
    id: Id,
    button: Rect,
    options: Vec<Choice>,
    selected: Option<usize>,
    handler: Option<HandlerId>,
}

pub struct Drawer {
    owner: Option<Id>,
    open: bool,
    openness: Spring,
    rows: usize,
    first: usize,
    wheel: f32,
    area: Option<Rect>,
    anchor: Option<Anchor>,
    shown: Option<Anchor>,
    seen: bool,
}

impl Default for Drawer {
    fn default() -> Self {
        Self {
            owner: None,
            open: false,
            openness: Spring::new(0.0, motion::SMOOTH, PRECISION),
            rows: 0,
            first: 0,
            wheel: 0.0,
            area: None,
            anchor: None,
            shown: None,
            seen: false,
        }
    }
}

impl Drawer {
    /// How much taller the page is while the drawer is open.
    pub fn extra(&self) -> f32 {
        (GAP + self.rows as f32 * ROW) * self.openness.value.clamp(0.0, 1.0)
    }

    pub fn is_open_for(&self, id: Id) -> bool {
        self.open && self.owner == Some(id)
    }

    pub fn toggle(&mut self, id: Id, count: usize, selected: Option<usize>) {
        if self.is_open_for(id) {
            self.close();
            return;
        }
        self.owner = Some(id);
        self.open = true;
        self.rows = count.min(MAX_ROWS);
        self.first = first_showing(selected, count);
        self.wheel = 0.0;
        self.openness.animate_to(1.0, motion::SMOOTH);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.area = None;
        self.openness.animate_to(0.0, motion::SMOOTH);
    }

    pub(super) fn begin(&mut self) {
        self.seen = false;
        self.anchor = None;
    }

    pub(super) fn anchor(
        &mut self,
        id: Id,
        button: Rect,
        options: &[Choice],
        selected: Option<usize>,
        handler: Option<HandlerId>,
    ) {
        if self.owner != Some(id) {
            return;
        }
        self.seen = true;
        let anchor = Anchor { id, button, options: options.to_vec(), selected, handler };
        self.anchor = Some(anchor.clone());
        self.shown = Some(anchor);
        if self.open {
            self.rows = options.len().min(MAX_ROWS);
        }
    }

    pub fn anchored(&self) -> bool {
        self.anchor.is_some()
    }

    pub(super) fn end(&mut self, ui: &egui::Ui) {
        // The dropdown wasn't painted this frame, so it's gone and its list goes with it.
        if self.open && !self.seen {
            self.close();
        }
        let Some(area) = self.area else { return };
        let outside =
            ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| !area.contains(p)));
        if outside {
            self.close();
        }
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.openness.step(dt);
    }

    pub(super) fn snap(&mut self) {
        self.openness.snap_to(self.openness.target);
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.openness.is_settled()
    }

    pub fn paint(&mut self, scene: &mut Scene, top: f32, left: f32, width: f32) {
        let openness = self.openness.value.clamp(0.0, 1.0);
        let Some(shown) = self.shown.clone().filter(|_| self.rows > 0 && openness > PRECISION) else { return };
        let list_top = top + GAP;
        let list = Rect::from_min_size(pos2(left, list_top), vec2(width, self.rows as f32 * ROW));
        let max_first = shown.options.len().saturating_sub(self.rows);
        if self.open {
            self.area = Some(shown.button.union(list));
            if max_first > 0 && scene.ui.rect_contains_pointer(list) {
                self.scroll(crate::ui::scroll::down(scene.ui), max_first);
            }
        }
        self.first = self.first.min(max_first);
        let visible_bottom = list_top + list.height() * openness;
        let clickable = self.open && scene.live;
        let mut chosen = None;
        for (i, choice) in shown.options.iter().enumerate().skip(self.first).take(self.rows) {
            let row_top = list_top + (i - self.first) as f32 * ROW;
            let reveal = ((visible_bottom - row_top) / ROW).clamp(0.0, 1.0);
            if reveal <= 0.01 {
                break;
            }
            let rect = Rect::from_min_size(pos2(left, row_top), vec2(width, ROW));
            let mut row = Canvas::new(scene.ui.painter());
            let id = shown.id.with(("choice", i));
            let icon = choice.icon.map(glyph);
            let response =
                controls::device_row(scene.ui, &mut row, rect, id, icon, &choice.label, shown.selected == Some(i));
            let alpha = smoothstep(0.0, 1.0, reveal);
            for mut shape in row.shapes {
                canvas::fade(&mut shape, alpha);
                scene.canvas.add(shape);
            }
            if response.clicked() && clickable && reveal > 0.5 {
                chosen = Some(i);
            }
        }
        if let Some(index) = chosen {
            scene.emit(shown.handler, UiValue::Index(index));
            self.close();
        }
    }

    fn scroll(&mut self, notches: f32, max_first: usize) {
        self.wheel += notches;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            self.first = self.first.saturating_sub(1);
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            self.first = (self.first + 1).min(max_first);
        }
    }
}

fn first_showing(selected: Option<usize>, count: usize) -> usize {
    let max_first = count.saturating_sub(MAX_ROWS);
    selected.map_or(0, |i| (i + 1).saturating_sub(MAX_ROWS)).min(max_first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_shows_chosen_row_and_grows() {
        let mut drawer = Drawer::default();
        let id = Id::new("menu");
        assert_eq!(drawer.extra(), 0.0);
        drawer.toggle(id, 7, Some(6));
        assert!(drawer.is_open_for(id));
        assert_eq!((drawer.rows, drawer.first), (MAX_ROWS, 3));
        drawer.openness.snap_to(1.0);
        assert_eq!(drawer.extra(), GAP + MAX_ROWS as f32 * ROW);
        drawer.toggle(id, 7, Some(6));
        assert!(!drawer.open);
        assert_eq!(drawer.openness.target, 0.0);
    }

    #[test]
    fn wheel_scrolls_whole_rows() {
        let mut drawer = Drawer::default();
        drawer.scroll(-2.5, 3);
        assert_eq!(drawer.first, 2);
        drawer.scroll(-9.0, 3);
        assert_eq!(drawer.first, 3);
        drawer.scroll(1.5, 3);
        assert_eq!(drawer.first, 2);
        assert_eq!(first_showing(None, 2), 0);
    }

    #[test]
    fn vanished_menu_closes_its_drawer() {
        let ctx = egui::Context::default();
        let mut drawer = Drawer::default();
        drawer.toggle(Id::new("menu"), 3, None);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            drawer.begin();
            drawer.end(ui);
        });
        output.textures_delta.clear();
        assert!(!drawer.open);
    }
}
