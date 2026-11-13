//! The icon row: how many icons fit, and how it scrolls and fades at its sides.

use egui::{Align2, Id, Pos2, Rect, Sense, Stroke, pos2, vec2};
use notchbase_sdk::protocol::TileNode;

use super::tile::TileState;
use super::well::{self, ICON_GROW, WELL};
use crate::ui::canvas::{fade, scale_about};
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::element::Scene;
use crate::ui::element::style::tone;
use crate::ui::glass::{self, Glass};
use crate::ui::spring::{lerp, smoothstep};
use crate::ui::{controls, font};

const FITTING: usize = 4;
const LONE_ROW: usize = 5;
const PEEKING: f32 = 5.125;
const FADE_W: f32 = 36.0;
const FADE_FROM: f32 = 24.0;
const SUNK: f32 = 0.88;
const NUDGE: f32 = 26.0;
const NUDGE_SPEED: f32 = 320.0;
const LIFT_ROOM: f32 = 8.0;
pub(super) const CELL_H: f32 = 70.0;
const WELL_TOP: f32 = 2.0;
const RING_GAP: f32 = 3.0;
const RING_W: f32 = 2.0;
const FOCUS_RING: f32 = 0.22;
const LABEL_GAP: f32 = 13.0;
const LABEL_MARGIN: f32 = 2.0;
const DISABLED: f32 = 0.4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Touch {
    pub(super) hovered: bool,
    pub(super) clicked: bool,
    pub(super) grabbed: Option<Pos2>,
    pub(super) dragged: Option<Pos2>,
    pub(super) dropped: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Cell {
    pub(super) focused: bool,
    pub(super) pressable: bool,
    pub(super) movable: bool,
    pub(super) lift: Option<f32>,
    pub(super) surfaced: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Strip {
    pub(super) rect: Rect,
    pub(super) count: usize,
    pub(super) scroll: f32,
}

impl Strip {
    pub(super) fn cell_width(&self) -> f32 {
        let across = match self.count {
            LONE_ROW => LONE_ROW as f32,
            count if count > LONE_ROW => PEEKING,
            _ => FITTING as f32,
        };
        self.rect.width() / across
    }

    pub(super) fn reach(&self) -> f32 {
        (self.count as f32 * self.cell_width() - self.rect.width()).max(0.0)
    }

    pub(super) fn scrolls(&self) -> bool {
        self.reach() > 0.0
    }

    pub(super) fn home(&self, index: usize) -> Rect {
        let width = self.cell_width();
        Rect::from_min_size(pos2(self.rect.left() + index as f32 * width, self.rect.top()), vec2(width, CELL_H))
    }

    pub(super) fn shown(&self, home: Rect) -> Rect {
        home.translate(vec2(-self.scroll, 0.0))
    }

    pub(super) fn along(&self, pointer: Pos2) -> Pos2 {
        pointer + vec2(self.scroll, 0.0)
    }

    pub(super) fn index_at(&self, along: Pos2) -> usize {
        let index = ((along.x - self.rect.left()) / self.cell_width()).floor().max(0.0) as usize;
        index.min(self.count.saturating_sub(1))
    }

    pub(super) fn fades(&self) -> [f32; 2] {
        [self.scroll, self.reach() - self.scroll].map(|hidden| (hidden / FADE_FROM).clamp(0.0, 1.0))
    }

    pub(super) fn surfaced(&self, shown: Rect) -> f32 {
        let [left, right] = self.fades();
        let from = |edge: f32, inward: f32, fade: f32| {
            let gone_at = WELL * 0.5;
            let clear = smoothstep(gone_at, gone_at + FADE_W, (shown.center().x - edge) * inward);
            1.0 - fade * (1.0 - clear)
        };
        from(self.rect.left(), 1.0, left).min(from(self.rect.right(), -1.0, right))
    }

    pub(super) fn nudge(&self, pointer: Pos2, dt: f32) -> f32 {
        let pull = |edge: f32, inward: f32| (1.0 - (pointer.x - edge) * inward / NUDGE).clamp(0.0, 1.0);
        (pull(self.rect.right(), -1.0) - pull(self.rect.left(), 1.0)) * NUDGE_SPEED * dt
    }

    pub(super) fn window(&self) -> Rect {
        self.rect.expand2(vec2(0.0, LIFT_ROOM))
    }
}

pub(super) fn sunk(surfaced: f32) -> f32 {
    lerp(SUNK, 1.0, surfaced)
}

pub(super) fn paint(scene: &mut Scene, tile: &TileNode, rect: Rect, state: &mut TileState, cell: Cell) -> Touch {
    let Cell { focused, pressable, movable, lift, surfaced } = cell;
    let id = Id::new(("notchbase.cell", scene.slot.0));
    let sense = match (scene.live, movable) {
        (true, true) => Sense::click_and_drag(),
        (true, false) => Sense::click(),
        (false, _) => Sense::hover(),
    };
    let body = scene.ui.interact(rect, id, sense);
    let mark = scene.canvas.mark();
    let hover = state.hover().clamp(0.0, 1.0);
    let center = well_center(rect);
    match tile.well {
        Some(t) if crate::ui::color::is_light(tone(t)) => {
            let well = egui::Rect::from_center_size(center, egui::Vec2::splat(WELL));
            glass::paint(scene.canvas, well, WELL * 0.5, Glass::tinted_by(tone(t)));
        }
        Some(t) => scene.canvas.circle(center, WELL * 0.5, tone(t)),
        None => well::paint(scene.canvas, center, WELL, hover),
    }
    let ring = WELL * 0.5 + RING_GAP;
    let tint = state.tint(tile.tint);
    let focus = controls::animate(scene.ui, id.with("focus"), focused);
    let plain = 1.0 - tint.map_or(0.0, |(_, shown)| shown);
    if focus * plain > 0.01 {
        scene.canvas.ring(center, ring, Stroke::new(RING_W * 0.5, white(FOCUS_RING * focus * plain)));
    }
    if let Some((t, shown)) = tint {
        scene.canvas.ring(center, ring, Stroke::new(RING_W, tone(t).gamma_multiply(shown)));
    }
    let icon_mark = scene.canvas.mark();
    let icon = scene.measure().size(&tile.icon, WELL);
    scene.paint(&tile.icon, Rect::from_center_size(center, icon), id.with("icon"));
    let grow = scale_about(center, ICON_GROW);
    for shape in scene.canvas.since(icon_mark) {
        shape.transform(grow);
    }
    let room = (rect.width() - 2.0 * LABEL_MARGIN).max(0.0);
    let name = controls::truncate(scene.canvas, &tile.title, font::caption(), room);
    let color = color::mix(secondary_label(), label(), focus.max(hover));
    scene.canvas.text(caption_at(rect), Align2::CENTER_CENTER, name, font::caption(), color);

    let live = scene.live;
    let moving = movable && (body.dragged() || body.drag_stopped());
    state.follow(live && body.hovered(), live && pressable && !moving && body.is_pointer_button_down_on());
    let press = scale_about(center, lift.unwrap_or_else(|| state.pressed()) * sunk(surfaced));
    for shape in scene.canvas.since(mark) {
        shape.transform(press);
        fade(shape, if tile.enabled { surfaced } else { surfaced * DISABLED });
    }
    let pointer = body.interact_pointer_pos().filter(|_| live && movable);
    Touch {
        hovered: live && body.hovered(),
        clicked: live && body.clicked(),
        grabbed: pointer.filter(|_| body.drag_started()),
        dragged: pointer.filter(|_| body.dragged()),
        dropped: live && movable && body.drag_stopped(),
    }
}

pub(super) fn well_center(rect: Rect) -> Pos2 {
    pos2(rect.center().x, rect.top() + WELL_TOP + WELL * 0.5)
}

fn caption_at(rect: Rect) -> Pos2 {
    pos2(rect.center().x, well_center(rect).y + WELL * 0.5 + LABEL_GAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: Rect = Rect::from_min_max(pos2(16.0, 50.0), pos2(356.0, 112.0));

    fn strip(count: usize, scroll: f32) -> Strip {
        Strip { rect: PAGE, count, scroll }
    }

    #[test]
    fn few_icons_fit_without_scrolling() {
        assert_eq!([1, 3, 4].map(|count| strip(count, 0.0).cell_width()), [85.0; 3]);
        assert_eq!(strip(5, 0.0).cell_width(), 68.0, "five share the row instead of leaving one alone");
        assert!([0, 1, 4, 5].iter().all(|count| !strip(*count, 0.0).scrolls()));
        assert_eq!(strip(4, 0.0).home(3).right(), 356.0);
        assert_eq!(strip(4, 0.0).home(0).min, pos2(16.0, 50.0));
        assert!(strip(5, 0.0).cell_width() >= WELL + 2.0 * (RING_GAP + RING_W), "the ring fits beside its neighbours");
    }

    #[test]
    fn overflow_shows_half_an_icon() {
        let nine = strip(9, 0.0);
        assert!(nine.scrolls());
        assert_eq!(nine.cell_width() * PEEKING, 340.0);
        assert_eq!(nine.reach(), 9.0 * nine.cell_width() - 340.0);
        assert!(
            (nine.home(8).right() - nine.reach() - 356.0).abs() < 0.01,
            "at the end the last icon touches the edge"
        );
        let scrolled = strip(9, 100.0);
        assert_eq!(scrolled.shown(scrolled.home(2)).left(), nine.home(2).left() - 100.0);
    }

    #[test]
    fn point_finds_slot_when_scrolled() {
        let at = |x: f32, strip: Strip| strip.index_at(strip.along(pos2(x, 60.0)));
        assert_eq!([at(20.0, strip(3, 0.0)), at(190.0, strip(3, 0.0)), at(350.0, strip(3, 0.0))], [0, 2, 2]);
        assert_eq!(at(-90.0, strip(9, 0.0)), 0);
        assert_eq!([at(20.0, strip(9, 0.0)), at(20.0, strip(9, 140.0)), at(350.0, strip(9, 257.0))], [0, 2, 8]);
    }

    #[test]
    fn sides_fade_by_what_they_hide() {
        let reach = strip(9, 0.0).reach();
        assert_eq!(strip(4, 0.0).fades(), [0.0, 0.0], "a row that fits never fades");
        assert_eq!(strip(9, 0.0).fades(), [0.0, 1.0]);
        assert_eq!(strip(9, 12.0).fades(), [0.5, 1.0]);
        assert_eq!(strip(9, 100.0).fades(), [1.0, 1.0]);
        assert_eq!(strip(9, reach).fades(), [1.0, 0.0]);
    }

    #[test]
    fn icons_dissolve_before_edge() {
        let start = strip(9, 0.0);
        let surfaced = |strip: Strip, index: usize| strip.surfaced(strip.shown(strip.home(index)));
        assert_eq!([surfaced(start, 0), surfaced(start, 3)], [1.0, 1.0], "nothing is hidden to the left yet");
        let fifth = surfaced(start, 4);
        assert!((0.3..0.7).contains(&fifth), "the fifth icon is whole but dimmed, to say there are more: {fifth}");
        assert!(start.shown(start.home(4)).center().x + WELL * 0.5 < PAGE.right(), "and is not cut by the edge");
        assert_eq!(surfaced(start, 5), 0.0);
        let row = strip(9, 100.0);
        for step in 0..=120 {
            let center = PAGE.left() - 30.0 + step as f32;
            let seen = row.surfaced(Rect::from_center_size(pos2(center, 80.0), vec2(row.cell_width(), CELL_H)));
            assert!(seen == 0.0 || center - WELL * 0.5 * sunk(seen) >= PAGE.left(), "{center}: {seen}");
        }
        assert_eq!(surfaced(strip(9, start.reach()), 8), 1.0, "the last icon is whole at the end");
        assert_eq!(strip(4, 0.0).surfaced(PAGE.translate(vec2(400.0, 0.0))), 1.0, "a row that fits never fades");
        assert_eq!((sunk(1.0), sunk(0.0)), (1.0, SUNK));
    }

    #[test]
    fn edge_hold_pulls_row() {
        let row = strip(9, 100.0);
        assert_eq!(row.nudge(PAGE.center(), 0.1), 0.0);
        assert!(row.nudge(pos2(PAGE.right() - 4.0, 60.0), 0.1) > 20.0);
        assert!(row.nudge(pos2(PAGE.left() + 4.0, 60.0), 0.1) < -20.0);
        assert_eq!(row.nudge(pos2(PAGE.right() + 50.0, 60.0), 0.1), NUDGE_SPEED * 0.1, "past the edge is full speed");
    }

    #[test]
    fn name_sits_inside_cell() {
        assert!(WELL_TOP + WELL + LABEL_GAP + font::caption().size * 0.5 <= CELL_H);
    }
}
