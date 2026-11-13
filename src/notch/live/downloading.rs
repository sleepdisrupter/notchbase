//! The rows of video downloads under the notch, with their covers and progress bars.

use std::f32::consts::TAU;
use std::sync::Arc;

use egui::epaint::RectShape;
use egui::{
    Align2, Color32, ColorImage, CornerRadius, Id, ImageData, Painter, Pos2, Rect, Stroke, TextureHandle, TextureId,
    TextureOptions, Vec2, pos2, vec2,
};

use crate::bridge::{Action, Activity, VideoLive, VideoStage};
use crate::notch::{Mode, Notch};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::glyph::{self, Glyph};
use crate::ui::metrics::PAD;
use crate::ui::rows::{self, Rows};
use crate::ui::spring::{Spring, UNIT_PRECISION, lerp};
use crate::ui::{controls, font, motion};

use super::video::{self, Look};

const MOST: usize = 3;
const ROW_H: f32 = 48.0;
const GAP: f32 = 8.0;
const COVER: f32 = 40.0;
const COVER_RADIUS: f32 = 9.0;
const TEXT_GAP: f32 = 12.0;
const TITLE_RISE: f32 = 11.0;
const BAR_DROP: f32 = 2.5;
const BAR_H: f32 = 3.0;
const STATUS_DROP: f32 = 13.0;
const BUTTON: f32 = 20.0;
const BUTTON_GAP: f32 = 12.0;
const MARK: f32 = 9.0;
const SEEKING: f32 = 0.3;
const SEEK_PERIOD: f32 = 1.6;
const BREATH_PERIOD: f32 = 1.3;
const REPLAN: f32 = 0.01;
const TEXTURE: &str = "notchbase.download.cover";
const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));

struct Cover {
    id: u64,
    image: Arc<ColorImage>,
    texture: TextureHandle,
    shown: Spring,
}

#[derive(Default)]
pub(in crate::notch) struct Downloading {
    rows: Rows<u64, VideoLive>,
    covers: Vec<Cover>,
    planned: f32,
}

impl Downloading {
    pub(in crate::notch) fn follow(&mut self, activities: &[Activity], animate: bool) {
        let lives = activities.iter().filter_map(|activity| match activity {
            Activity::Video(live) => Some((live.id, live.clone())),
            _ => None,
        });
        self.rows.follow(lives.take(MOST), animate);
    }

    fn room(&self) -> f32 {
        let shown: f32 = self.rows.iter().map(|(_, _, shown)| shown).sum();
        if shown <= 0.0 { 0.0 } else { ROW_H * shown + GAP * shown.min(1.0) }
    }

    pub(in crate::notch) fn plan_room(&mut self) {
        self.planned = self.room();
    }

    fn step(&mut self, dt: f32) {
        self.rows.step(dt);
        let rows = &self.rows;
        self.covers.retain(|cover| {
            rows.iter().any(|(id, live, _)| {
                *id == cover.id && live.cover.as_ref().is_some_and(|image| Arc::ptr_eq(image, &cover.image))
            })
        });
        for cover in &mut self.covers {
            cover.shown.step(dt);
        }
    }

    pub(in crate::notch) fn snap(&mut self) {
        self.rows.snap();
        for cover in &mut self.covers {
            cover.shown.snap_to(cover.shown.target);
        }
    }

    pub(in crate::notch) fn is_animating(&self) -> bool {
        self.rows.is_animating() || self.covers.iter().any(|cover| !cover.shown.is_settled())
    }

    fn running(&self) -> bool {
        self.rows.iter().any(|(_, live, _)| video::continuous(live))
    }

    fn cover(&mut self, ctx: &egui::Context, live: &VideoLive) -> Option<(TextureId, f32)> {
        let image = live.cover.as_ref()?;
        let fresh = |cover: &Cover| cover.id == live.id && Arc::ptr_eq(&cover.image, image);
        if !self.covers.iter().any(fresh) {
            let texture = ctx.load_texture(TEXTURE, ImageData::Color(Arc::clone(image)), TextureOptions::LINEAR);
            let mut shown = Spring::new(0.0, motion::HOVER, UNIT_PRECISION);
            shown.animate_to(1.0, motion::HOVER);
            self.covers.retain(|cover| cover.id != live.id);
            self.covers.push(Cover { id: live.id, image: Arc::clone(image), texture, shown });
        }
        let cover = self.covers.iter().find(|cover| fresh(cover))?;
        Some((cover.texture.id(), cover.shown.value.clamp(0.0, 1.0)))
    }
}

impl Notch {
    pub(in crate::notch) fn download_room(&self) -> f32 {
        self.downloading.room()
    }

    pub(in crate::notch) fn downloads_move(&self) -> bool {
        self.downloading.is_animating() || (self.panel_alpha > 0.0 && self.downloading.running())
    }

    pub(in crate::notch) fn step_downloading(&mut self, dt: f32) {
        self.downloading.step(dt);
        let room = self.downloading.room();
        let grown = room - self.downloading.planned;
        if grown.abs() <= REPLAN {
            return;
        }
        self.downloading.planned = room;
        if self.mode == Mode::Expanded {
            self.morph.h.value += grown;
            self.morph.h.target += grown;
        }
    }

    pub(in crate::notch) fn downloading_ui(
        &mut self,
        ui: &egui::Ui,
        painter: &Painter,
        shape: Rect,
        input: bool,
    ) -> (Canvas, Vec<Action>) {
        let mut canvas = Canvas::new(painter);
        let mut actions = Vec::new();
        let left = self.body_rect(self.expanded_geometry()).left() + PAD;
        let width = self.panel_width() - 2.0 * PAD;
        let seconds = self.clock.seconds();
        let shown: Vec<(VideoLive, f32, Look)> = (self.downloading.rows.iter())
            .map(|(_, live, shown)| (live.clone(), shown, self.video_look(live)))
            .collect();
        let tall: f32 = shown.iter().map(|(_, shown, _)| ROW_H * shown).sum();
        let mut top = shape.bottom() - PAD - tall;
        for (live, shown, look) in shown {
            let row = Rect::from_min_size(pos2(left, top), vec2(width, ROW_H));
            top += ROW_H * shown;
            if rows::alpha(shown) <= 0.001 {
                continue;
            }
            let mark = canvas.mark();
            let cover = self.downloading.cover(ui.ctx(), &live);
            let text_right = paint_row(&mut canvas, &live, look, row, cover, seconds);
            let center = pos2(text_right + BUTTON_GAP + BUTTON * 0.5, row.center().y);
            match live.stage {
                _ if video::cancellable(&live) => {
                    let id = Id::new(("notchbase.download.cancel", live.id));
                    if video::cancel(ui, &mut canvas, center, id, input && shown > 0.9) {
                        actions.push(Action::CancelVideo(live.id));
                    }
                }
                VideoStage::Saved => glyph::paint(&mut canvas, Glyph::Checkmark, center, MARK, color::GREEN),
                _ => glyph::paint(&mut canvas, Glyph::Exclamation, center, MARK + 4.0, color::RED),
            }
            rows::reveal(&mut canvas, mark, Rect::from_min_size(row.min, vec2(width, ROW_H * shown)), shown);
        }
        (canvas, actions)
    }
}

fn paint_row(
    canvas: &mut Canvas,
    live: &VideoLive,
    look: Look,
    row: Rect,
    cover: Option<(TextureId, f32)>,
    seconds: f32,
) -> f32 {
    let art = Rect::from_min_size(pos2(row.left(), row.center().y - COVER * 0.5), Vec2::splat(COVER));
    paint_cover(canvas, live, look, art, cover, seconds);
    let left = art.right() + TEXT_GAP;
    let right = row.right() - BUTTON - BUTTON_GAP;
    let room = (right - left).max(0.0);
    let title = if live.title.is_empty() { live.site.name() } else { live.title.as_str() };
    let title = controls::truncate(canvas, title, font::headline(), room);
    canvas.text(pos2(left, row.center().y - TITLE_RISE), Align2::LEFT_CENTER, title, font::headline(), label());
    let bar = Rect::from_min_size(pos2(left, row.center().y + BAR_DROP - BAR_H * 0.5), vec2(room, BAR_H));
    paint_bar(canvas, live, look, bar, seconds);
    let status = controls::truncate(canvas, &status(live, look), font::subheadline(), room);
    let at = pos2(left, row.center().y + STATUS_DROP);
    canvas.text(at, Align2::LEFT_CENTER, status, font::subheadline(), secondary_label());
    right
}

fn paint_cover(
    canvas: &mut Canvas,
    live: &VideoLive,
    look: Look,
    art: Rect,
    cover: Option<(TextureId, f32)>,
    seconds: f32,
) {
    let shown = cover.map_or(0.0, |(_, shown)| shown);
    canvas.rect(art, COVER_RADIUS, white(0.08 * (1.0 - shown)));
    if shown < 1.0 {
        let mark = canvas.mark();
        video::paint_leading(canvas, live, look, art.center(), seconds);
        for shape in canvas.since(mark) {
            fade(shape, 1.0 - shown);
        }
    }
    if let Some((texture, shown)) = cover {
        let lit = Color32::WHITE.gamma_multiply(shown);
        canvas.add(RectShape::filled(art, CornerRadius::same(COVER_RADIUS as u8), lit).with_texture(texture, FULL_UV));
        canvas.rect_stroke(art, COVER_RADIUS, Stroke::new(1.0, white(0.1 * shown)));
    }
}

fn paint_bar(canvas: &mut Canvas, live: &VideoLive, look: Look, bar: Rect, seconds: f32) {
    let radius = BAR_H * 0.5;
    canvas.rect(bar, radius, white(0.14));
    let tint = video::tint(live.site);
    let breath = 0.5 + 0.5 * (seconds * TAU / BREATH_PERIOD).sin();
    let (from, to, ink) = match live.stage {
        VideoStage::Preparing => (0.0, look.share, color::PEARL),
        VideoStage::Downloading => (0.0, look.share, tint),
        VideoStage::Joining => (0.0, 1.0, tint.gamma_multiply(lerp(0.55, 1.0, breath))),
        VideoStage::Saved => (0.0, 1.0, color::mix(tint, color::GREEN, look.done.clamp(0.0, 1.0))),
        VideoStage::Finding => {
            let middle = 0.5 + 0.5 * (seconds * TAU / SEEK_PERIOD).sin();
            let start = middle * (1.0 - SEEKING);
            (start, start + SEEKING, tint)
        }
        VideoStage::Failed => return,
    };
    if to <= from {
        return;
    }
    let (start, end) = (lerp(bar.left(), bar.right(), from.max(0.0)), lerp(bar.left(), bar.right(), to.min(1.0)));
    canvas.rect(Rect::from_min_max(pos2(start, bar.top()), pos2(end.max(start + BAR_H), bar.bottom())), radius, ink);
}

fn status(live: &VideoLive, look: Look) -> String {
    let percent = format!("{}%", (look.share * 100.0).floor() as u32);
    match live.stage {
        VideoStage::Preparing | VideoStage::Failed => live.detail.clone(),
        VideoStage::Finding => format!("Finding the {}", if live.audio { "sound" } else { "video" }),
        VideoStage::Downloading if live.detail.is_empty() => percent,
        VideoStage::Downloading => format!("{percent} \u{b7} {}", live.detail),
        VideoStage::Joining => "Saving".to_owned(),
        VideoStage::Saved => "Saved in Downloads".to_owned(),
    }
}
