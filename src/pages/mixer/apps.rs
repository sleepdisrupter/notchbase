//! The apps section, or why it is empty.

use std::borrow::Cow;

use egui::{Align2, Rect, Shape, pos2, vec2};

use crate::audio::Mixer;
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::ui::canvas::Canvas;
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::smoothstep;
use crate::ui::{color, controls, font, motion};

use super::app_row::RowFrame;
use super::layout::{APP_ROWS, APPS_GAP, APPS_TITLE_H, AppsBody, EMPTY_H, Layout, ROW};
use super::{MixerPage, fade_into};

const REASON_PAD: f32 = 6.0;
const EMPTY_GLYPH: f32 = 16.0;
const EMPTY_GLYPH_GAP: f32 = 6.0;
const UNSUPPORTED: &str = "Per-app volume isn't available";
const NOTHING_PLAYING: &str = "Nothing is playing";

struct Body {
    page: Rect,
    top: f32,
    bottom: f32,
}

impl Body {
    fn reveal(&self, height: f32) -> f32 {
        ((self.bottom - self.top) / height).clamp(0.0, 1.0)
    }
}

impl MixerPage {
    pub(super) fn apps_section(
        &mut self,
        ctx: &mut PageCtx<'_>,
        mixer: &Mixer,
        body: AppsBody,
        layout: &Layout,
        actions: &mut Vec<Action>,
    ) {
        let rect = ctx.rect;
        let top = rect.top() + layout.apps_top;
        controls::section_header(ctx.canvas, pos2(rect.left(), top + APPS_TITLE_H * 0.5), "Apps");
        let body_top = top + APPS_TITLE_H + APPS_GAP;
        let area = Body { page: rect, top: body_top, bottom: body_top + layout.apps_h };
        match body {
            AppsBody::Reason => self.paint_reason(ctx, mixer.unsupported_reason().unwrap_or(UNSUPPORTED), &area),
            AppsBody::Empty => paint_empty(ctx, &area, 1.0),
            AppsBody::Rows(_) => {
                let shown = self.app_rows(ctx, mixer, &area, actions);
                paint_empty(ctx, &area, 1.0 - shown);
            }
        }
    }

    fn paint_reason(&mut self, ctx: &mut PageCtx<'_>, reason: &str, area: &Body) {
        let text_color = color::secondary_label();
        let galley = ctx.ui.painter().layout(reason.to_owned(), font::callout(), text_color, area.page.width());
        self.reason_h = galley.size().y + REASON_PAD;
        let reveal = area.reveal(self.reason_h);
        let mut c = Canvas::new(ctx.ui.painter());
        c.add(Shape::galley(pos2(area.page.left(), area.top + 2.0), galley, text_color));
        fade_into(ctx.canvas, c, smoothstep(0.3, 1.0, reveal));
    }

    fn app_rows(&mut self, ctx: &mut PageCtx<'_>, mixer: &Mixer, area: &Body, actions: &mut Vec<Action>) -> f32 {
        let interactive = ctx.alpha >= 0.5;
        let mut shown = 0.0f32;
        for i in 0..self.slots.len().min(APP_ROWS) {
            let slot = self.slots[i];
            let target_y = i as f32 * ROW;
            let live = mixer.apps().iter().find(|a| a.id == slot.id);
            let state = self.row_state(slot.id, live, target_y);
            let app = match (live, &state.last) {
                (Some(app), _) => {
                    state.last = Some(app.clone());
                    Cow::Borrowed(app)
                }
                (None, Some(last)) => Cow::Owned(last.clone()),
                (None, None) => continue,
            };
            state.y.animate_to(target_y, motion::SMOOTH);
            state.appear.animate_to(if slot.leaving { 0.0 } else { 1.0 }, motion::SMOOTH);
            shown = shown.max(state.appear.value.clamp(0.0, 1.0));
            let y = area.top + state.y.value;
            let reveal = ((area.bottom - y) / ROW).clamp(0.0, 1.0);
            let alpha = state.appear.value.clamp(0.0, 1.0) * smoothstep(0.0, 1.0, reveal);
            if alpha <= 0.002 {
                continue;
            }
            let row = Rect::from_min_size(pos2(area.page.left(), y), vec2(area.page.width(), ROW));
            let mut c = Canvas::new(ctx.ui.painter());
            let live_row = interactive && !slot.leaving && alpha > 0.5;
            let frame = RowFrame::read(ctx.ui, self.scope, app.id, row, live_row, ctx.now);
            self.app_row(ctx.ui.ctx(), &mut c, &app, &frame, actions);
            fade_into(ctx.canvas, c, alpha);
        }
        shown
    }
}

fn paint_empty(ctx: &mut PageCtx<'_>, area: &Body, shown: f32) {
    if shown <= 0.01 {
        return;
    }
    let mut c = Canvas::new(ctx.ui.painter());
    let cy = area.top + EMPTY_H * 0.5;
    let text_w = c.text_width(NOTHING_PLAYING, font::callout());
    let glyph_w = EMPTY_GLYPH + EMPTY_GLYPH_GAP;
    let x = area.page.center().x - (glyph_w + text_w) * 0.5;
    glyph::paint(&mut c, Glyph::Speaker, pos2(x + EMPTY_GLYPH * 0.5, cy), EMPTY_GLYPH, color::tertiary_label());
    c.text(pos2(x + glyph_w, cy), Align2::LEFT_CENTER, NOTHING_PLAYING, font::callout(), color::secondary_label());
    fade_into(ctx.canvas, c, shown * smoothstep(0.3, 1.0, area.reveal(EMPTY_H)));
}
