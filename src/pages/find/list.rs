//! The scrolling list of results: which rows are in view and how they fade in and out at the edges.

use std::collections::HashMap;
use std::ops::Range;

use egui::{Rect, Vec2, pos2, vec2};

use crate::bridge::{Action, FindRow, How};
use crate::pages::PageCtx;
use crate::platform::{Feel, haptic};
use crate::ui::canvas::fade;
use crate::ui::glass::{self, Glass};
use crate::ui::scroll::{self, POINTS_PER_NOTCH};
use crate::ui::spring::{POINT_PRECISION, Spring, UNIT_PRECISION, smoothstep};
use crate::ui::{font, motion};

use super::FindPage;
use super::cut::{self, Cut};
use super::hold::Taken;
use super::row::{self, Face, HIGHLIGHT_INSET, Hit, ROW_H};
use super::words::{self, Clock};

const EDGE_FADE: f32 = 12.0;
const SELECTED_RADIUS: f32 = 16.0;
const SHOWN: f32 = 1.0;

pub(super) struct List {
    offset: Spring,
    appear: HashMap<u64, Spring>,
    titles: HashMap<u64, Cut>,
    pub(super) height: f32,
}

impl Default for List {
    fn default() -> Self {
        Self {
            offset: Spring::new(0.0, motion::SMOOTH, POINT_PRECISION),
            appear: HashMap::new(),
            titles: HashMap::new(),
            height: 0.0,
        }
    }
}

impl List {
    pub(super) fn arrive(&mut self, rows: &[FindRow], taken: Taken) {
        self.titles.clear();
        if taken == Taken::Fresh {
            self.offset.snap_to(0.0);
        }
        self.appear.retain(|key, _| rows.iter().any(|row| row.key == *key));
        for row in rows {
            self.appear.entry(row.key).or_insert_with(|| {
                let mut spring = Spring::new(0.0, motion::HOVER, UNIT_PRECISION);
                spring.animate_to(SHOWN, motion::HOVER);
                spring
            });
        }
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn scrolled(&self) -> f32 {
        self.offset.value
    }

    pub(super) fn settle(&mut self) {
        self.appear.values_mut().for_each(|spring| spring.snap_to(SHOWN));
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.offset.is_settled() || self.appear.values().any(|spring| !spring.is_settled())
    }

    pub(super) fn scroll(&mut self, notches: f32, count: usize, height: f32) {
        let target = self.offset.target - notches * POINTS_PER_NOTCH;
        self.offset.animate_to(target.clamp(0.0, spare(count, height)), motion::SMOOTH);
    }

    pub(super) fn follow(&mut self, index: usize, count: usize, height: f32) {
        self.offset.animate_to(into_view(self.offset.target, index, count, height), motion::SMOOTH);
    }

    pub(super) fn reveal(&mut self, index: usize, count: usize) {
        self.offset.snap_to(into_view(self.offset.target, index, count, self.height));
    }

    fn step(&mut self, dt: f32) {
        self.offset.step(dt);
        self.appear.values_mut().for_each(|spring| spring.step(dt));
    }

    fn alpha(&self, key: u64) -> f32 {
        self.appear.get(&key).map_or(SHOWN, |spring| spring.value.clamp(0.0, 1.0))
    }

    fn cut(&mut self, ctx: &PageCtx<'_>, row: &FindRow, room: f32) -> Cut {
        *self.titles.entry(row.key).or_insert_with(|| {
            cut::middle_showing(&row.name, row.marks.first(), |text| {
                ctx.canvas.text_width(text, font::headline()) <= room
            })
        })
    }
}

pub(super) fn visible(offset: f32, count: usize, height: f32) -> Range<usize> {
    let first = (offset / ROW_H).floor().max(0.0) as usize;
    let last = ((offset + height) / ROW_H).ceil().max(0.0) as usize;
    first.min(count)..last.min(count)
}

pub(super) fn edge(row: Rect, area: Rect) -> f32 {
    let out = (area.top() - row.top()).max(row.bottom() - area.bottom()).max(0.0);
    1.0 - smoothstep(0.0, ROW_H - EDGE_FADE, out)
}

fn into_view(offset: f32, index: usize, count: usize, height: f32) -> f32 {
    let height = height.max(ROW_H);
    let top = index as f32 * ROW_H;
    offset.clamp(top + ROW_H - height, top).clamp(0.0, spare(count, height))
}

fn spare(count: usize, height: f32) -> f32 {
    (count as f32 * ROW_H - height).max(0.0)
}

impl FindPage {
    pub(super) fn paint_rows(&mut self, ctx: &mut PageCtx<'_>, area: Rect, live: bool, actions: &mut Vec<Action>) {
        let count = self.shown.rows.len();
        self.list.height = area.height();
        if count == 0 {
            self.select.unplace();
            return;
        }
        if live && ctx.ui.rect_contains_pointer(area) {
            self.list.scroll(scroll::down(ctx.ui), count, area.height());
        }
        self.list.step(ctx.dt);
        let offset = self.list.offset.value;
        let mark = ctx.canvas.mark();
        let glide = self.select.glide(ROW_H, ctx.dt) - offset;
        let lit = Rect::from_min_size(pos2(area.left(), area.top() + glide), vec2(area.width(), ROW_H));
        let band = lit.shrink2(vec2(0.0, HIGHLIGHT_INSET));
        glass::paint(ctx.canvas, band, SELECTED_RADIUS, Glass::TILE_HOVER);
        let shown = edge(lit, area);
        ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, shown));
        let moved = ctx.ui.input(|input| input.pointer.delta() != Vec2::ZERO);
        let clock = Clock::current();
        let suggesting = self.draft.is_empty();
        let mut hits = Vec::new();
        for index in visible(offset, count, area.height()) {
            let row = &self.shown.rows[index];
            let top = area.top() + index as f32 * ROW_H - offset;
            let rect = Rect::from_min_size(pos2(area.left(), top), vec2(area.width(), ROW_H));
            let detail = words::told(row, suggesting, &clock);
            let shared = self.shown.rows.iter().filter(|other| other.name == row.name).count() > 1;
            let face = Face {
                row,
                detail: &detail,
                hint: if shared { words::beside(row, &self.shown.rows) } else { "" },
                cut: self.list.cut(ctx, row, row::text_room(rect, row)),
                two_tone: !suggesting,
                place_first: !suggesting || row.why.is_none(),
                selected: index == self.select.index,
                shelved: self.shelved.contains_key(&row.key),
                alpha: self.list.alpha(row.key) * edge(rect, area),
                live: live && area.contains(rect.center()),
            };
            let out = row::paint(ctx, rect, &face, &mut self.icons);
            if out.hovered && self.select.point(index, moved, &self.shown.rows) {
                haptic(Feel::Tick);
            }
            hits.extend(out.hit.map(|hit| (hit, index)));
        }
        ctx.canvas.clip(mark, area);
        for (hit, index) in hits {
            let how = match hit {
                Hit::Open => How::Open,
                Hit::AsAdmin => How::AsAdmin,
                Hit::Reveal => How::Reveal,
                Hit::Shelve => How::Shelve,
                Hit::Drag => How::Drag,
                Hit::Menu => {
                    self.offer_menu(ctx, index);
                    continue;
                }
            };
            actions.extend(self.pick(how, index, ctx.now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIX: f32 = 6.0 * ROW_H;

    #[test]
    fn paints_only_visible_rows() {
        assert_eq!(visible(0.0, 20, SIX), 0..6);
        assert_eq!(visible(ROW_H * 0.5, 20, SIX), 0..7);
        assert_eq!(visible(0.0, 3, SIX), 0..3);
        assert_eq!(visible(14.0 * ROW_H, 20, SIX), 14..20);
    }

    #[test]
    fn scroll_stays_within_rows() {
        let mut list = List::default();
        list.scroll(-100.0, 20, SIX);
        assert_eq!(list.offset.target, 14.0 * ROW_H, "never past the last row");
        list.scroll(100.0, 20, SIX);
        assert_eq!(list.offset.target, 0.0);
        list.follow(8, 20, SIX);
        assert_eq!(list.offset.target, 3.0 * ROW_H, "the selected row comes into view at the bottom");
        list.follow(1, 20, SIX);
        assert_eq!(list.offset.target, ROW_H, "and at the top");
        list.scroll(-1.0, 3, SIX);
        assert_eq!(list.offset.target, 0.0, "a short list does not scroll");
    }

    #[test]
    fn rows_fade_at_window_edge() {
        let area = Rect::from_min_size(pos2(0.0, 100.0), vec2(340.0, SIX));
        let inside = Rect::from_min_size(pos2(0.0, 100.0), vec2(340.0, ROW_H));
        assert_eq!(edge(inside, area), 1.0);
        assert!(edge(inside.translate(vec2(0.0, -20.0)), area) < 1.0);
        assert_eq!(edge(inside.translate(vec2(0.0, -ROW_H)), area), 0.0);
    }

    #[test]
    fn new_rows_fade_in_kept_stay() {
        let row = |key| FindRow {
            key,
            path: std::sync::Arc::from(std::path::Path::new("/x")),
            name: "x".into(),
            place: "~".into(),
            marks: Vec::new(),
            kind: crate::bridge::Kind::Other,
            app: false,
            folder: false,
            cloud: false,
            modified: 0,
            why: None,
            predicted: false,
            icon: None,
        };
        let mut list = List::default();
        list.arrive(&[row(1)], Taken::Fresh);
        list.settle();
        list.arrive(&[row(1), row(2)], Taken::Refresh);
        assert_eq!((list.alpha(1), list.alpha(2)), (1.0, 0.0));
        assert!(list.is_animating());
    }
}
