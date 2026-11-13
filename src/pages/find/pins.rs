//! The pinned items under the resting bar, which can be dragged into a new order.

use std::collections::HashMap;

use egui::{Id, Pos2, Rect, Sense, Vec2, pos2};

use crate::bridge::{FindRow, PINNED_AT_MOST};
use crate::pages::PageCtx;
use crate::pages::tools::well;
use crate::platform::{Feel, haptic};
use crate::ui::canvas::{fade, scale_about};
use crate::ui::spring::{POINT_PRECISION, Spring, lerp};
use crate::ui::{color, controls, motion};

use super::icons::{self, Icons};

pub(super) const PINS_H: f32 = 66.0;
pub(super) const EMBLEM: f32 = 52.0;
const HOVERED: f32 = 1.07;
const LIFTED: f32 = 1.14;
const SHEEN: f32 = 0.3;
const PIN_ID: &str = "notchbase.find.pin";
/// How far above or below the strip a held pin goes before it leaves the strip as a file drag (into the dock,
/// Finder or any app), instead of being reordered.
const PULL_OUT: f32 = EMBLEM;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Chose {
    Open(u64, Pos2),
    Menu(u64, Pos2),
    Moved(Vec<u64>),
    /// Pulled out of the strip: hand it to the system as a file drag.
    Out(u64),
}

struct Held {
    key: u64,
    grip: f32,
    at: f32,
}

#[derive(Default)]
pub(super) struct Pins {
    order: Vec<u64>,
    places: HashMap<u64, Spring>,
    held: Option<Held>,
}

pub(super) fn id_of(key: u64) -> Id {
    Id::new((PIN_ID, key))
}

fn slot_of(strip: f32) -> f32 {
    strip / PINNED_AT_MOST as f32
}

fn centre(left: f32, slot: f32, index: usize) -> f32 {
    left + slot * (index as f32 + 0.5)
}

fn slot_at(x: f32, left: f32, slot: f32, count: usize) -> usize {
    (((x - left) / slot).floor().max(0.0) as usize).min(count.saturating_sub(1))
}

fn pin(at: f32, area: Rect) -> Rect {
    Rect::from_center_size(pos2(at, area.center().y), Vec2::splat(EMBLEM))
}

fn pulled_out(pointer: Pos2, area: Rect) -> bool {
    (pointer.y - area.center().y).abs() > PULL_OUT
}

fn moved(order: &mut Vec<u64>, key: u64, to: usize) {
    let Some(from) = order.iter().position(|held| *held == key) else { return };
    let key = order.remove(from);
    order.insert(to.min(order.len()), key);
}

fn following(order: &mut Vec<u64>, saved: &[u64], dragging: bool) {
    if !dragging {
        saved.clone_into(order);
        return;
    }
    order.retain(|key| saved.contains(key));
    order.extend(saved.iter().filter(|key| !order.contains(key)).copied().collect::<Vec<_>>());
}

impl Pins {
    pub(super) fn is_animating(&self) -> bool {
        self.held.is_some() || self.places.values().any(|place| !place.is_settled())
    }

    pub(super) fn let_go(&mut self) {
        self.held = None;
    }

    pub(super) fn paint(
        &mut self,
        ctx: &mut PageCtx<'_>,
        area: Rect,
        rows: &[FindRow],
        icons: &mut Icons,
        live: bool,
    ) -> Option<Chose> {
        let saved: Vec<u64> = rows.iter().map(|row| row.key).collect();
        following(&mut self.order, &saved, self.held.is_some());
        self.places.retain(|key, _| saved.contains(key));
        let slot = slot_of(area.width());
        let (first, last) =
            (centre(area.left(), slot, 0), centre(area.left(), slot, self.order.len().saturating_sub(1)));
        let pointer = ctx.ui.input(|input| input.pointer.interact_pos());
        let mut chose = None;
        let mut hovered = None;
        for (index, key) in self.order.clone().into_iter().enumerate() {
            let home = centre(area.left(), slot, index);
            let place = self.places.entry(key).or_insert_with(|| Spring::new(home, motion::SMOOTH, POINT_PRECISION));
            place.animate_to(home, motion::SMOOTH);
            place.step(ctx.dt);
            let at = self.held.as_ref().filter(|held| held.key == key).map_or(place.value, |held| held.at);
            let plate = pin(at, area);
            let touched =
                ctx.ui.interact(plate, id_of(key), if live { Sense::click_and_drag() } else { Sense::hover() });
            if !live {
                continue;
            }
            if touched.hovered() {
                hovered = Some(key);
            }
            if touched.drag_started() {
                let grip = pointer.map_or(0.0, |pointer| pointer.x - at);
                self.held = Some(Held { key, grip, at });
                haptic(Feel::Tick);
            }
            if let (true, Some(held), Some(pointer)) = (touched.dragged(), self.held.as_ref(), pointer)
                && held.key == key
                && pulled_out(pointer, area)
            {
                // Out of the strip: it goes back to its place here, and the system drag carries it on.
                self.held = None;
                chose = Some(Chose::Out(key));
                continue;
            }
            if let (true, Some(held), Some(pointer)) = (touched.dragged(), self.held.as_mut(), pointer) {
                held.at = (pointer.x - held.grip).clamp(first, last);
                let before = self.order.clone();
                moved(&mut self.order, key, slot_at(held.at, area.left(), slot, saved.len()));
                if self.order != before {
                    haptic(Feel::Tick);
                }
            }
            if touched.drag_stopped() {
                // Nothing held: it was pulled out and handed over, and the strip kept its saved order.
                let Some(held) = self.held.take() else { continue };
                self.places.entry(key).and_modify(|place| place.snap_to(held.at));
                chose = (self.order != saved).then(|| Chose::Moved(self.order.clone()));
                if chose.is_some() {
                    haptic(Feel::Snap);
                }
            } else if touched.clicked() {
                chose = Some(Chose::Open(key, pos2(at, area.center().y)));
            } else if touched.secondary_clicked() {
                chose = Some(Chose::Menu(key, pointer.unwrap_or(plate.center())));
            }
        }
        let held = self.held.as_ref().map(|held| (held.key, held.at));
        let resting = self.order.iter().filter(|key| held.is_none_or(|(lifted, _)| lifted != **key));
        for key in resting.copied().collect::<Vec<_>>() {
            let at = self.places.get(&key).map_or(first, |place| place.value);
            tile(ctx, area, rows, icons, (key, at), (hovered == Some(key) && held.is_none(), false));
        }
        if let Some(lifted) = held {
            tile(ctx, area, rows, icons, lifted, (true, true));
        }
        chose
    }
}

fn tile(
    ctx: &mut PageCtx<'_>,
    area: Rect,
    rows: &[FindRow],
    icons: &mut Icons,
    (key, at): (u64, f32),
    (lit, lifted): (bool, bool),
) {
    let Some(row) = rows.iter().find(|row| row.key == key) else { return };
    let mark = ctx.canvas.mark();
    let center = pos2(at, area.center().y);
    let hover = controls::animate(ctx.ui, id_of(key).with("hover"), lit);
    let lift = controls::animate(ctx.ui, id_of(key).with("lift"), lifted);
    let landed = icons.shown(key);
    if landed < 1.0 {
        let waiting = ctx.canvas.mark();
        well::paint(ctx.canvas, center, EMBLEM, hover);
        well::glyph(ctx.canvas, icons::fallback(row), center, EMBLEM, color::PEARL);
        ctx.canvas.since(waiting).iter_mut().for_each(|shape| fade(shape, 1.0 - landed));
    }
    icons.paint(ctx.ui.ctx(), ctx.canvas, row, center, EMBLEM);
    icons.shine(ctx.canvas, key, center, EMBLEM, SHEEN * hover.max(lift));
    let scale = lerp(lerp(1.0, HOVERED, hover), LIFTED, lift);
    if scale != 1.0 {
        let grow = scale_about(center, scale);
        ctx.canvas.since(mark).iter_mut().for_each(|shape| shape.transform(grow));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragged_pin_takes_slot() {
        let left = 10.0;
        let slot = slot_of(312.0);
        assert_eq!(slot, 78.0, "four discs share the strip evenly");
        assert_eq!((centre(left, slot, 0), centre(left, slot, 3)), (49.0, 283.0));
        assert_eq!(slot_at(49.0, left, slot, 4), 0);
        assert_eq!(slot_at(87.9, left, slot, 4), 0, "until its centre has crossed into the next slot");
        assert_eq!(slot_at(88.1, left, slot, 4), 1);
        assert_eq!(slot_at(-500.0, left, slot, 4), 0);
        assert_eq!(slot_at(5_000.0, left, slot, 3), 2, "never past the last pin");
        assert_eq!(slot_at(100.0, left, slot, 0), 0);
        let area = Rect::from_min_size(pos2(left, 50.0), egui::vec2(312.0, PINS_H));
        assert_eq!(pin(49.0, area), Rect::from_center_size(pos2(49.0, 83.0), Vec2::splat(EMBLEM)));
    }

    #[test]
    fn a_pin_leaves_the_strip_once_pulled_well_above_or_below_it() {
        let area = Rect::from_min_size(pos2(10.0, 50.0), egui::vec2(312.0, PINS_H));
        let y = area.center().y;
        assert!(!pulled_out(pos2(100.0, y), area));
        assert!(!pulled_out(pos2(100.0, y + EMBLEM - 1.0), area), "a wobble while reordering stays");
        assert!(pulled_out(pos2(100.0, y + EMBLEM + 1.0), area));
        assert!(pulled_out(pos2(100.0, y - EMBLEM - 1.0), area));
    }

    #[test]
    fn moving_pin_shifts_others() {
        let mut order = vec![1, 2, 3, 4];
        moved(&mut order, 1, 2);
        assert_eq!(order, [2, 3, 1, 4]);
        moved(&mut order, 4, 0);
        assert_eq!(order, [4, 2, 3, 1]);
        moved(&mut order, 3, 99);
        assert_eq!(order, [4, 2, 1, 3], "past the end is the end");
        moved(&mut order, 7, 0);
        assert_eq!(order, [4, 2, 1, 3], "a pin that is not there moves nothing");
    }

    #[test]
    fn strip_follows_saved_pins() {
        let mut order = vec![1, 2, 3];
        following(&mut order, &[3, 1, 2], false);
        assert_eq!(order, [3, 1, 2]);
        moved(&mut order, 3, 2);
        following(&mut order, &[3, 1, 2], true);
        assert_eq!(order, [1, 2, 3], "the order being dragged is kept");
        following(&mut order, &[3, 1, 9], true);
        assert_eq!(order, [1, 3, 9], "a pin that went is dropped and a new one joins at the end");
    }
}
