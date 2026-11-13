//! The What's New page: this version's changes, a picture when the release has one, and Done.

mod layout;
#[cfg(test)]
mod tests;

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Shape, pos2, vec2};

use self::layout::{Bullet, EMPTY_H, GLYPH, Plan, VERSION_GAP, baseline, text_indent};
use super::PageCtx;
use crate::bridge::Action;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label};
use crate::ui::metrics::CARD_RADIUS;
use crate::ui::picture::Album;
use crate::ui::{controls, font, glyph};
use crate::whats_new::Release;

const LIVE_FROM: f32 = 0.5;
const DONE_SLOP: f32 = 6.0;
const TITLE: &str = "What's New";
const DONE: &str = "Done";
const NOTHING: &str = "Nothing new in this version";
const DONE_ID: &str = "notchbase.whats-new.done";

#[derive(Default)]
pub struct WhatsNewPage {
    album: Album,
    done: bool,
}

impl WhatsNewPage {
    pub fn height(&self, release: Option<&Release>, width: f32, beside_camera: bool, canvas: &Canvas) -> f32 {
        layout::plan(release, width, beside_camera, canvas).height
    }

    pub fn is_animating(&self) -> bool {
        false
    }

    pub fn take_done(&mut self) -> bool {
        std::mem::take(&mut self.done)
    }

    pub fn rest(&mut self) {
        self.album.forget();
        self.done = false;
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, release: Option<&Release>, beside_camera: bool) -> Vec<Action> {
        let live = ctx.alpha >= LIVE_FROM;
        let plan = layout::plan(release, ctx.rect.width(), beside_camera, ctx.canvas);
        let pressed = header(ctx, &plan, live);
        if let Some((picture, frame)) = plan.picture {
            let frame = frame.translate(ctx.rect.min.to_vec2());
            self.album.paint(ctx.ui.ctx(), ctx.canvas, picture, frame, CARD_RADIUS);
        }
        if plan.bullets.is_empty() {
            nothing(ctx, &plan);
        }
        for bullet in &plan.bullets {
            point(ctx, &plan, bullet);
        }
        if !pressed {
            return Vec::new();
        }
        self.done = true;
        vec![Action::ChangesSeen]
    }
}

fn point(ctx: &mut PageCtx<'_>, plan: &Plan, bullet: &Bullet) {
    let (left, top) = (ctx.rect.left() + plan.column.min, ctx.rect.top() + bullet.top);
    let mark = pos2(left + GLYPH * 0.5, top + bullet.line * 0.5);
    glyph::paint(ctx.canvas, bullet.change.glyph, mark, GLYPH, label());
    let text = left + text_indent();
    let room = (ctx.rect.left() + plan.column.max - text).max(0.0);
    let title = controls::truncate(ctx.canvas, bullet.change.title, font::headline(), room);
    ctx.canvas.text(pos2(text, top), Align2::LEFT_TOP, title, font::headline(), label());
    let caption = pos2(text, ctx.rect.top() + bullet.caption_top());
    ctx.canvas.add(Shape::galley(caption, bullet.caption.clone(), secondary_label()));
}

fn header(ctx: &mut PageCtx<'_>, plan: &Plan, live: bool) -> bool {
    let header = &plan.header;
    let line = ctx.rect.top() + baseline(&header.title);
    let left = ctx.rect.left() + plan.column.min;
    ctx.canvas.add(Shape::galley(pos2(left, ctx.rect.top()), header.title.clone(), label()));
    let beside = pos2(left + header.title.size().x + VERSION_GAP, line - baseline(&header.version));
    ctx.canvas.add(Shape::galley(beside, header.version.clone(), secondary_label()));
    done(ctx, pos2(ctx.rect.left() + plan.column.max, line), live)
}

fn done(ctx: &mut PageCtx<'_>, at: Pos2, live: bool) -> bool {
    let id = Id::new(DONE_ID);
    let words = ctx.canvas.wrapped(DONE, font::headline(), label(), f32::INFINITY);
    let min = pos2(at.x - words.size().x, at.y - baseline(&words));
    let rect = Rect::from_min_size(min, words.size()).expand(DONE_SLOP);
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    ctx.canvas.text(min, Align2::LEFT_TOP, DONE, font::headline(), color::mix(label(), Color32::WHITE, hover));
    live && response.clicked()
}

fn nothing(ctx: &mut PageCtx<'_>, plan: &Plan) {
    let top = ctx.rect.top() + plan.list_top;
    let area = Rect::from_min_size(pos2(ctx.rect.left(), top), vec2(ctx.rect.width(), EMPTY_H));
    ctx.canvas.text(area.center(), Align2::CENTER_CENTER, NOTHING, font::subheadline(), secondary_label());
}
