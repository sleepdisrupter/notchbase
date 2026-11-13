//! The drop zone: the well that opens under the notch while files or a video are dragged over it.

pub(super) mod layouts;

use egui::{Align2, CornerRadius, Painter, Rect, Shadow, Vec2, pos2};

use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label};
use crate::ui::font;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{lerp, smoothstep};
use crate::webvideo::Kind;

use super::geometry::Geometry;
use super::live::video;
use super::moves::transition::Motion;
use super::{Mode, Notch};

fn video_tile(well: Rect, kind: Kind) -> Rect {
    let half = (well.width() - 3.0 * CHOICE_INSET) * 0.5;
    let left = match kind {
        Kind::Video => well.left() + CHOICE_INSET,
        Kind::Audio => well.center().x + CHOICE_INSET * 0.5,
    };
    Rect::from_min_size(pos2(left, well.top() + CHOICE_INSET), egui::vec2(half, CHOICE_H))
}

const WELL_H: f32 = 84.0;
const WELL_INSET: f32 = 10.0;
const GLOW_PERIOD: f32 = 1.8;
const BOB_PERIOD: f32 = 1.1;
const PROMPT: &str = "Drop to add to Shelf";
const VIDEO_PROMPT: &str = "Drop to download";
const VIDEO_WORD: &str = "Video";
const AUDIO_WORD: &str = "Audio";
const CHOICE_INSET: f32 = 6.0;
const CHOICE_H: f32 = 58.0;
const CHOICE_RADIUS: f32 = 14.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct WellFace {
    picking: bool,
    video: Option<crate::webvideo::Site>,
    aim: Option<Kind>,
    files: usize,
    hot: Option<super::Pick>,
    usual: Option<super::Pick>,
    away: bool,
}

impl Notch {
    fn well_face(&self) -> WellFace {
        WellFace {
            picking: self.picking,
            video: self.drop_video,
            aim: self.video_aim,
            files: self.drop_files,
            hot: self.hot_pick,
            usual: self.usual_pick,
            away: self.away,
        }
    }

    fn wear(&mut self, face: WellFace) -> WellFace {
        let worn = self.well_face();
        self.picking = face.picking;
        self.drop_video = face.video;
        self.video_aim = face.aim;
        self.drop_files = face.files;
        self.hot_pick = face.hot;
        self.usual_pick = face.usual;
        self.away = face.away;
        worn
    }

    pub(super) fn drop_geometry(&self) -> Geometry {
        let wide = if self.picking { self.layouts_width() } else { 300.0 };
        match self.style {
            Style::Notch => Geometry {
                w: (self.notch.x + 2.0 * 72.0).max(wide),
                h: self.notch.y + WELL_H + 12.0,
                r: 22.0,
                ear: 10.0,
            },
            Style::Island => Geometry { w: wide, h: WELL_H + 20.0, r: 30.0, ear: 0.0 },
        }
    }

    pub(super) fn drop_ui(&mut self, painter: &Painter, g: Geometry, body: Rect) {
        if self.mode == Mode::Drop {
            self.drop_face = self.well_face();
        }
        let alpha = self.drop_alpha(g);
        if alpha <= 0.001 {
            return;
        }
        let live = (self.mode != Mode::Drop).then(|| self.wear(self.drop_face));
        self.paint_drop(painter, g, body, alpha);
        if let Some(live) = live {
            self.wear(live);
        }
    }

    fn paint_drop(&self, painter: &Painter, g: Geometry, body: Rect, alpha: f32) {
        let well = self.drop_well(body);
        let mut canvas = Canvas::new(painter);
        self.paint_well(&mut canvas, well, g.r);
        match self.drop_video {
            _ if self.picking => self.layouts_ui(&mut canvas, well),
            Some(site) => self.paint_video_prompt(&mut canvas, well, site),
            None => self.paint_prompt(&mut canvas, well),
        }
        let clip = painter.with_clip_rect(body.expand(1.0));
        canvas.paint(&clip, pos2(body.center().x, body.top()), lerp(0.94, 1.0, alpha), Vec2::ZERO, alpha);
    }

    fn drop_alpha(&self, g: Geometry) -> f32 {
        let (target, idle) = (self.drop_geometry(), self.idle_geometry());
        let grown = ((g.h - idle.h) / (target.h - idle.h).max(1.0)).clamp(0.0, 1.0);
        self.drop_fade.value.clamp(0.0, 1.0)
            * smoothstep(0.55, 0.95, grown)
            * smoothstep(0.8, 1.0, self.attract.value)
            * self.clear_of_the_panel()
    }

    pub(super) fn drop_well(&self, body: Rect) -> Rect {
        let top = match self.style {
            Style::Notch => body.top() + self.notch.y + 2.0,
            Style::Island => body.top() + WELL_INSET,
        };
        let bottom_right = pos2(body.right() - WELL_INSET, body.bottom() - WELL_INSET);
        Rect::from_min_max(pos2(body.left() + WELL_INSET, top), bottom_right)
    }

    fn paint_well(&self, canvas: &mut Canvas, well: Rect, body_radius: f32) {
        let pulse = self.clock.pulse(GLOW_PERIOD);
        let tint = self.drop_video.filter(|_| !self.picking).map_or(color::ACCENT, video::tint);
        let glow = Shadow { offset: [0, 0], blur: 18, spread: 0, color: tint.gamma_multiply(0.16 + 0.10 * pulse) };
        let radius = (body_radius - WELL_INSET).max(12.0);
        canvas.add(glow.as_shape(well, CornerRadius::same(radius.round() as u8)));
        glass::paint(canvas, well, radius, Glass::tinted_by(tint));
    }

    fn paint_video_prompt(&self, canvas: &mut Canvas, well: Rect, site: crate::webvideo::Site) {
        let tint = video::tint(site);
        let bob = 2.0 * self.clock.wave(BOB_PERIOD);
        for (kind, glyph, word) in [(Kind::Video, Glyph::Film, VIDEO_WORD), (Kind::Audio, Glyph::Waveform, AUDIO_WORD)]
        {
            let tile = video_tile(well, kind);
            let aimed = self.video_aim == Some(kind);
            if aimed {
                glass::paint(canvas, tile, CHOICE_RADIUS, Glass::tinted_by(tint));
                canvas.rect(tile, CHOICE_RADIUS, tint.gamma_multiply(0.16));
            }
            let ink = if aimed { label() } else { secondary_label() };
            let lift = if aimed { bob * 0.5 } else { 0.0 };
            glyph::paint(canvas, glyph, pos2(tile.center().x, tile.top() + 21.0 + lift), 20.0, ink);
            canvas.text(pos2(tile.center().x, tile.top() + 45.0), Align2::CENTER_CENTER, word, font::headline(), ink);
        }
        let from = match (self.video_aim, site) {
            (_, crate::webvideo::Site::Other) => VIDEO_PROMPT.to_owned(),
            _ => format!("{VIDEO_PROMPT} from {}", site.name()),
        };
        let at = pos2(well.center().x, well.bottom() - 11.0);
        canvas.text(at, Align2::CENTER_CENTER, from, font::subheadline(), secondary_label());
    }

    pub fn set_drop_video(&mut self, site: Option<crate::webvideo::Site>) -> bool {
        if self.preview.is_some() || (self.locked && site.is_some()) || self.drop_video == site {
            return false;
        }
        self.drop_video = site;
        match site {
            Some(_) if self.mode != Mode::Drop => {
                self.drop_files = 0;
                self.set_mode(Mode::Drop, Motion::Open);
            }
            Some(_) => self.follow_target(),
            None => {
                self.video_aim = None;
                if self.mode == Mode::Drop && !self.picking && self.drop_files == 0 {
                    self.return_to_rest();
                } else if self.mode == Mode::Drop {
                    self.follow_target();
                }
            }
        }
        true
    }

    pub fn takes_video_at(&self, cursor: egui::Pos2) -> Option<Kind> {
        if self.drop_video.is_none() || self.mode != Mode::Drop {
            return None;
        }
        if self.picking {
            return self.video_cell_at(cursor);
        }
        let well = self.drop_well(self.body_rect(self.drop_geometry()));
        Some(if cursor.x < well.center().x { Kind::Video } else { Kind::Audio })
    }

    pub fn aim_video(&mut self, aim: Option<Kind>) -> bool {
        let aim = aim.filter(|_| self.drop_video.is_some());
        let changed = self.video_aim != aim;
        if changed && aim.is_some() && self.preview.is_none() {
            crate::platform::haptic(crate::platform::Feel::Tick);
        }
        self.video_aim = aim;
        changed
    }

    fn paint_prompt(&self, canvas: &mut Canvas, well: Rect) {
        let cx = well.center().x;
        let bob = 2.0 * self.clock.wave(BOB_PERIOD);
        glyph::paint(canvas, Glyph::Tray, pos2(cx, well.top() + 26.0 + bob * 0.5), 24.0, label());
        canvas.text(pos2(cx, well.top() + 55.0), Align2::CENTER_CENTER, PROMPT, font::headline(), label());
        let count = match self.drop_files {
            0 | 1 => "1 item".to_owned(),
            n => format!("{n} items"),
        };
        canvas.text(pos2(cx, well.top() + 71.0), Align2::CENTER_CENTER, count, font::subheadline(), secondary_label());
    }
}

#[cfg(test)]
mod tests {
    use super::video_tile;
    use crate::bridge::Page;
    use crate::notch::Mode;
    use crate::notch::tests::fixtures::{island, settle};
    use crate::webvideo::Kind;

    #[test]
    fn drop_opens_shelf_in_place() {
        let mut n = island();
        n.set_drop_hover(None);
        assert_eq!(n.mode, Mode::Idle);
        n.set_drop_hover(Some(2));
        assert!(n.is_drop_target() && n.is_animating());
        n.set_drop_hover(Some(3));
        assert_eq!((n.mode, n.drop_files), (Mode::Drop, 3));
        n.set_drop_hover(None);
        n.open_page(Page::Shelf);
        assert_eq!((n.mode, n.pager.page, n.pager.from), (Mode::Expanded, Page::Shelf, Page::Shelf));
        settle(&mut n);
        assert!(!n.is_animating());
    }

    #[test]
    fn video_tab_opens_and_closes() {
        use crate::webvideo::Site;
        let mut n = island();
        assert!(n.set_drop_video(Some(Site::YouTube)));
        assert!(!n.set_drop_video(Some(Site::YouTube)), "nothing changes while it keeps hovering");
        assert!(n.is_drop_target());
        let well = n.drop_well(n.body_rect(n.drop_geometry()));
        let (left, right) = (well.center() - egui::vec2(40.0, 0.0), well.center() + egui::vec2(40.0, 0.0));
        assert_eq!(n.takes_video_at(left), Some(Kind::Video));
        assert_eq!(n.takes_video_at(right), Some(Kind::Audio), "the right half keeps only the sound");
        assert!(well.contains_rect(video_tile(well, Kind::Video)) && well.contains_rect(video_tile(well, Kind::Audio)));
        assert!(n.aim_video(Some(Kind::Audio)) && !n.aim_video(Some(Kind::Audio)));
        assert!(n.set_drop_video(None));
        assert_eq!(n.mode, Mode::Idle);
        assert!(n.takes_video_at(left).is_none() && n.video_aim.is_none());
        settle(&mut n);
        assert!(!n.is_animating());
    }

    #[test]
    fn link_keeps_notch_open() {
        use crate::webvideo::Site;
        let mut n = island();
        n.set_drop_hover(Some(1));
        n.set_drop_video(Some(Site::Instagram));
        n.set_drop_video(None);
        assert_eq!(n.mode, Mode::Drop, "the drag is still over the notch");
        n.set_drop_hover(None);
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn download_cell_takes_video() {
        use crate::webvideo::Site;
        let mut n = island();
        n.set_window_drag(true);
        let narrow = n.drop_geometry().w;
        n.set_drop_video(Some(Site::YouTube));
        assert!(n.is_picking_layout(), "the picker stays");
        assert!(n.drop_geometry().w > narrow, "and makes room for one more cell");
        settle(&mut n);
        let well = n.drop_well(n.body_rect(n.drop_geometry()));
        for kind in [Kind::Video, Kind::Audio] {
            let cell = n.video_cell(well, kind);
            assert!(well.contains_rect(cell.expand(2.0)), "{cell:?} spills out of {well:?}");
            assert_eq!(n.takes_video_at(cell.center()), Some(kind));
            assert!(n.pick_at(cell.center()).is_none(), "a download cell is not a layout");
        }
        let layout = egui::pos2(well.center().x, n.video_cell(well, Kind::Video).center().y);
        assert!(n.takes_video_at(layout).is_none() && n.pick_at(layout).is_some());
        n.set_window_drag(false);
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn closing_well_keeps_face() {
        use crate::webvideo::Site;
        let rig = crate::notch::tests::fixtures::Rig::new();
        let mut n = island();
        n.set_window_drag(true);
        n.hover_pick(Some(crate::notch::Pick::Fill));
        for _ in 0..40 {
            rig.frame(&mut n, &[], Vec::new());
        }
        let picked = n.drop_face;
        n.set_window_drag(false);
        rig.frame(&mut n, &[], Vec::new());
        assert_eq!(n.drop_face, picked, "the layout grid fades out as it was");
        assert!(!n.picking && n.hot_pick.is_none(), "while the notch itself has moved on");

        n.set_drop_video(Some(Site::YouTube));
        for _ in 0..40 {
            rig.frame(&mut n, &[], Vec::new());
        }
        let video = n.drop_face;
        n.set_drop_video(None);
        rig.frame(&mut n, &[], Vec::new());
        assert_eq!(n.drop_face, video);
        assert!(n.drop_video.is_none());
    }

    #[test]
    fn passing_drag_leaves_notch() {
        let mut n = island();
        n.set_drop_hover(Some(1));
        n.set_drop_hover(None);
        assert_eq!(n.mode, Mode::Idle);
        settle(&mut n);
        assert!(!n.is_animating());

        n.set_pinned(true);
        n.set_drop_hover(Some(1));
        assert!(n.is_drop_target());
        n.set_drop_hover(None);
        assert_eq!(n.mode, Mode::Expanded);
    }

    #[test]
    fn well_waits_for_page_fade() {
        let mut n = island();
        n.open_page(Page::Tools);
        settle(&mut n);
        n.set_drop_hover(Some(2));
        let mut overlap = 0.0f32;
        for _ in 0..60 {
            n.step(1.0 / 60.0);
            overlap = overlap.max(n.drop_alpha(n.morph.current()).min(n.panel_fade.value));
        }
        assert!(overlap < 0.1, "the page and the well are never both half there: {overlap}");
        assert!(n.drop_alpha(n.morph.current()) > 0.99);
    }
}
