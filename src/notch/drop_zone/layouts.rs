//! The layout picker that opens while a window is dragged to the notch, and its video download cells.

use egui::{Align2, Pos2, Rect, Stroke, pos2, vec2};

use crate::notch::Notch;
use crate::snap::Zone;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label};
use crate::ui::font;
use crate::ui::glyph::{self, Glyph};
use crate::webvideo::Kind;

const CELL: egui::Vec2 = vec2(38.0, 26.0);
const CELL_GAP: f32 = 8.0;
const CELL_TOP: f32 = 12.0;
const CELL_GROW: f32 = 2.0;
const PROMPT: &str = "Drop on a layout";
const VIDEO_GAP: f32 = 12.0;
const VIDEO_GLYPH: f32 = 14.0;
const VIDEO_LABEL: &str = "Download video";
const AUDIO_LABEL: &str = "Download audio only";
const AWAY_KEY: &str = if cfg!(target_os = "macos") { "⇧" } else { "Shift" };

pub use crate::snap::Choice as Pick;

pub const PICKS: [Pick; 9] = [
    Pick::Auto,
    Pick::Fill,
    Pick::Zone(Zone::Left),
    Pick::Zone(Zone::Right),
    Pick::Zone(Zone::Full),
    Pick::Zone(Zone::LeftThird),
    Pick::Zone(Zone::CenterThird),
    Pick::Zone(Zone::RightThird),
    Pick::Zone(Zone::Center),
];

pub(in crate::notch) fn width() -> f32 {
    PICKS.len() as f32 * CELL.x + (PICKS.len() - 1) as f32 * CELL_GAP + 36.0
}

fn cells(well: Rect) -> [Rect; PICKS.len()] {
    let total = PICKS.len() as f32 * CELL.x + (PICKS.len() - 1) as f32 * CELL_GAP;
    let left = well.center().x - total * 0.5;
    std::array::from_fn(|i| {
        Rect::from_min_size(pos2(left + i as f32 * (CELL.x + CELL_GAP), well.top() + CELL_TOP), CELL)
    })
}

impl Notch {
    pub(super) fn layouts_width(&self) -> f32 {
        let cells = 2.0 * (CELL.x + CELL_GAP);
        width() + if self.drop_video.is_some() { 2.0 * (VIDEO_GAP + cells) } else { 0.0 }
    }

    pub(super) fn video_cell(&self, well: Rect, kind: Kind) -> Rect {
        let last = cells(well)[PICKS.len() - 1];
        let nth = if kind == Kind::Video { 0.0 } else { 1.0 };
        let left = last.right() + CELL_GAP + VIDEO_GAP + nth * (CELL.x + CELL_GAP);
        Rect::from_min_size(pos2(left, last.top()), CELL)
    }

    pub(super) fn video_cell_at(&self, cursor: Pos2) -> Option<Kind> {
        let well = self.drop_well(self.body_rect(self.drop_geometry()));
        let reach = vec2(CELL_GAP * 0.5, CELL_TOP);
        [Kind::Video, Kind::Audio].into_iter().find(|kind| self.video_cell(well, *kind).expand2(reach).contains(cursor))
    }

    fn video_cells_ui(&self, canvas: &mut Canvas, well: Rect, site: crate::webvideo::Site) {
        let tint = crate::notch::live::video::tint(site);
        for (kind, glyph) in [(Kind::Video, Glyph::Film), (Kind::Audio, Glyph::Waveform)] {
            let aimed = self.video_aim == Some(kind);
            let rect = self.video_cell(well, kind);
            let rect = if aimed { rect.expand(CELL_GROW) } else { rect };
            if aimed {
                canvas.rect(rect, 6.0, tint.gamma_multiply(0.9));
            }
            canvas.rect_stroke(rect, 6.0, Stroke::new(1.2, if aimed { tint } else { tint.gamma_multiply(0.75) }));
            glyph::paint(canvas, glyph, rect.center(), VIDEO_GLYPH, if aimed { color::ink_on(tint) } else { tint });
        }
    }

    pub fn set_window_drag(&mut self, on: bool) {
        if self.preview.is_some() || (self.locked && on) {
            return;
        }
        if on {
            self.picking = true;
            self.drop_files = 0;
            if self.mode != crate::notch::Mode::Drop {
                self.set_mode(crate::notch::Mode::Drop, crate::notch::moves::transition::Motion::Open);
            }
        } else if self.picking {
            self.picking = false;
            self.hot_pick = None;
            self.usual_pick = None;
            self.away = false;
            if self.mode == crate::notch::Mode::Drop {
                self.return_to_rest();
            }
        }
    }

    pub fn is_picking_layout(&self) -> bool {
        self.picking && self.mode == crate::notch::Mode::Drop
    }

    pub fn pick_at(&self, cursor: Pos2) -> Option<Pick> {
        let well = self.drop_well(self.body_rect(self.drop_geometry()));
        if !well.contains(cursor) {
            return None;
        }
        let rects = cells(well);
        PICKS.iter().zip(rects).find_map(|(pick, rect)| {
            let column = rect.expand2(vec2(CELL_GAP * 0.5, 0.0));
            (cursor.x >= column.left() && cursor.x < column.right()).then_some(*pick)
        })
    }

    pub fn set_usual_pick(&mut self, pick: Option<Pick>) {
        self.usual_pick = pick;
    }

    pub fn send_away(&mut self, on: bool) {
        self.away = on;
    }

    pub fn hover_pick(&mut self, pick: Option<Pick>) {
        self.hot_pick = pick;
    }

    pub(super) fn layouts_ui(&self, canvas: &mut Canvas, well: Rect) {
        for (pick, rect) in PICKS.iter().zip(cells(well)) {
            let hot = self.hot_pick == Some(*pick);
            let usual = self.usual_pick == Some(*pick);
            paint_cell(canvas, *pick, if hot { rect.expand(CELL_GROW) } else { rect }, hot, usual);
            if usual {
                canvas.circle(pos2(rect.center().x, rect.bottom() + 6.0), 1.8, color::PEARL);
            }
        }
        if let Some(site) = self.drop_video {
            self.video_cells_ui(canvas, well, site);
        }
        let (text, tone) = match (self.hot_pick, self.usual_pick) {
            _ if self.video_aim == Some(Kind::Video) => (VIDEO_LABEL.to_owned(), label()),
            _ if self.video_aim == Some(Kind::Audio) => (AUDIO_LABEL.to_owned(), label()),
            (Some(pick), _) if self.away => (format!("{} · next display", pick.label()), label()),
            (Some(pick), _) => (pick.label().to_owned(), label()),
            (None, Some(usual)) => (format!("Usually {}", usual.label().to_lowercase()), secondary_label()),
            (None, None) if crate::snap::screens().len() > 1 => {
                (format!("{PROMPT} · hold {AWAY_KEY} for the next display"), secondary_label())
            }
            (None, None) => (PROMPT.to_owned(), secondary_label()),
        };
        canvas.text(pos2(well.center().x, well.bottom() - 16.0), Align2::CENTER_CENTER, text, font::headline(), tone);
    }
}

fn paint_cell(canvas: &mut Canvas, pick: Pick, rect: Rect, hot: bool, usual: bool) {
    let outline = if hot { label() } else { color::white(if usual { 0.6 } else { 0.32 }) };
    let fill = if hot { color::PEARL } else { color::white(0.5) };
    canvas.rect_stroke(rect, 6.0, Stroke::new(1.2, outline));
    let inner = rect.shrink(3.5);
    match pick {
        Pick::Auto => {
            let pane = vec2((inner.width() - 2.0) * 0.5, (inner.height() - 2.0) * 0.5);
            for (column, row) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let at = inner.min + vec2(column * (pane.x + 2.0), row * (pane.y + 2.0));
                canvas.rect(Rect::from_min_size(at, pane), 2.0, fill);
            }
        }
        Pick::Fill => {
            let taken = Rect::from_min_max(inner.min, pos2(inner.left() + inner.width() * 0.3, inner.bottom()));
            canvas.rect_stroke(taken.shrink(0.5), 2.0, Stroke::new(1.0, fill));
            let free = Rect::from_min_max(pos2(taken.right() + 2.0, inner.top()), inner.max);
            canvas.rect(free.shrink(0.5), 2.5, fill);
        }
        Pick::Zone(zone) => {
            let share = zone.share();
            let pane = Rect::from_min_max(
                inner.min + vec2(inner.width() * share.left(), inner.height() * share.top()),
                inner.min + vec2(inner.width() * share.right(), inner.height() * share.bottom()),
            );
            canvas.rect(pane.shrink(0.5), 2.5, fill);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::{island, notch, settle};

    fn picking(mut n: Notch) -> Notch {
        n.set_window_drag(true);
        settle(&mut n);
        n
    }

    #[test]
    fn window_drag_opens_picker() {
        let n = picking(island());
        assert!(n.is_picking_layout() && n.is_drop_target());
    }

    #[test]
    fn picker_closes_when_window_leaves() {
        let mut n = picking(island());
        n.set_window_drag(false);
        settle(&mut n);
        assert!(!n.is_picking_layout() && !n.is_drop_target() && !n.is_animating());
    }

    #[test]
    fn every_cell_reaches_its_layout() {
        for n in [picking(island()), picking(notch())] {
            let well = n.drop_well(n.body_rect(n.drop_geometry()));
            for (pick, rect) in PICKS.iter().zip(cells(well)) {
                assert_eq!(n.pick_at(rect.center()), Some(*pick));
            }
            assert_eq!(n.pick_at(pos2(well.left() - 40.0, well.center().y)), None);
        }
    }

    #[test]
    fn cells_fit_in_well() {
        for n in [picking(island()), picking(notch())] {
            let well = n.drop_well(n.body_rect(n.drop_geometry()));
            for rect in cells(well) {
                assert!(well.contains_rect(rect.expand(CELL_GROW)), "{rect:?} spills out of {well:?}");
            }
        }
    }

    #[test]
    fn usual_layout_named_until_hover() {
        let mut n = picking(island());
        n.set_usual_pick(Some(Pick::Zone(Zone::Left)));
        n.hover_pick(Some(Pick::Auto));
        n.hover_pick(None);
        assert_eq!(n.usual_pick, Some(Pick::Zone(Zone::Left)));
        n.set_window_drag(false);
        assert_eq!(n.usual_pick, None);
    }

    #[test]
    fn file_drop_replaces_picker() {
        let mut n = picking(island());
        n.set_drop_hover(Some(2));
        assert!(!n.is_picking_layout() && n.is_drop_target());
    }
}
