//! The search field: what was typed, the top hit's completion after it, and a spinner while Find works.

use std::f32::consts::{FRAC_PI_2, TAU};

use egui::{Align2, Id, Pos2, Rect, Sense, Shape, Stroke, Vec2, pos2, vec2};

use crate::bridge::{FindRow, FindView};
use crate::pages::PageCtx;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::field::bare::{Bare, bare};
use crate::ui::field::{Edit, Focus};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{Spring, UNIT_PRECISION};
use crate::ui::{font, motion};

use super::icons::Icons;
use super::row;
use super::states::{self, DISC};

pub(super) const FIELD_H: f32 = 46.0;
pub(super) const FIELD_ID: &str = "notchbase.find.field";
const MAGNIFIER: f32 = 19.0;
const MAGNIFIER_X: f32 = 19.0;
const TEXT_X: f32 = 42.0;
const TEXT_SIZE: f32 = 21.0;
const HIT: f32 = 28.0;
const GHOST: f32 = 0.62;
const DASH: &str = "  \u{2014}  ";
const EDGE: f32 = (FIELD_H - DISC) * 0.5;
const ITEM_GAP: f32 = 8.0;
const READING_LEAVES: f32 = 24.0;
const RING: f32 = 14.0;
const RING_W: f32 = 1.75;
const RING_TRACK: f32 = 0.14;
const RING_ARC: f32 = 0.28;
const RING_STEPS: usize = 16;
const TURN_SECONDS: f32 = 0.9;
#[cfg(target_os = "macos")]
const PLACEHOLDER: &str = "Search this Mac";
#[cfg(not(target_os = "macos"))]
const PLACEHOLDER: &str = "Search this PC";
const SHOWN: f32 = 0.01;
const CLEAR_LIVE: f32 = 0.5;

pub(super) struct Look {
    clear: Spring,
    lit: Spring,
    spin: f32,
    spinning: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            clear: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            lit: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            spin: 0.0,
            spinning: false,
        }
    }
}

impl Look {
    pub(super) fn is_animating(&self) -> bool {
        self.spinning || !self.clear.is_settled() || !self.lit.is_settled()
    }
}

pub(super) struct Typed {
    pub(super) edits: Vec<Edit>,
    pub(super) cleared: bool,
    pub(super) pressed: bool,
}

pub(super) fn id() -> Id {
    Id::new(FIELD_ID)
}

pub(super) fn rect_in(page: Rect) -> Rect {
    Rect::from_min_size(page.min, vec2(page.width(), FIELD_H))
}

pub(super) fn completes<'a>(draft: &str, name: &'a str) -> Option<&'a str> {
    let mut rest = name.char_indices();
    for typed in draft.chars() {
        let (_, shown) = rest.next()?;
        if !typed.to_lowercase().eq(shown.to_lowercase()) {
            return None;
        }
    }
    Some(rest.next().map_or("", |(at, _)| &name[at..]))
}

pub(super) fn paint(
    ctx: &mut PageCtx<'_>,
    look: &mut Look,
    focus: &mut Option<Focus>,
    (draft, view): (&str, &FindView),
    (hit, icons): (Option<&FindRow>, &mut Icons),
    live: bool,
) -> Typed {
    let rect = rect_in(ctx.rect);
    let typed = !draft.is_empty();
    let hit = hit.filter(|_| typed);
    let response = ctx.ui.interact(rect, id(), if live { Sense::click() } else { Sense::hover() });
    let pressed = live && response.clicked() && focus.is_none();
    for (spring, on) in [(&mut look.clear, typed && hit.is_none()), (&mut look.lit, typed)] {
        spring.animate_to(f32::from(u8::from(on)), motion::HOVER);
        spring.step(ctx.dt);
    }
    let tint = color::mix(secondary_label(), color::PEARL, look.lit.value.clamp(0.0, 1.0));
    glyph::paint(ctx.canvas, Glyph::Search, pos2(rect.left() + MAGNIFIER_X, rect.center().y), MAGNIFIER, tint);
    let mut right = rect.right() - EDGE;
    let clear = look.clear.value.clamp(0.0, 1.0);
    let mut cleared = false;
    if clear > SHOWN {
        let disc = Rect::from_center_size(pos2(right - DISC * 0.5, rect.center().y), Vec2::splat(DISC));
        cleared = states::disc(ctx, disc, id().with("clear"), live && clear >= CLEAR_LIVE, clear);
        right -= (DISC + ITEM_GAP) * clear;
    }
    if let Some(hit) = hit {
        let center = pos2(right - HIT * 0.5, rect.center().y);
        row::emblem(ctx, hit, icons, center, HIT / row::ICON);
        right -= HIT + ITEM_GAP;
    }
    let text_left = rect.left() + TEXT_X;
    let font = font::regular(TEXT_SIZE);
    if let Some(reading) = view.reading.as_deref().filter(|_| typed) {
        let width = ctx.canvas.text_width(reading, font::subheadline());
        let typed_width = ctx.canvas.text_width(draft, font.clone());
        if right - width - ITEM_GAP - text_left >= typed_width + READING_LEAVES {
            let at = pos2(right, rect.center().y);
            ctx.canvas.text(at, Align2::RIGHT_CENTER, reading, font::subheadline(), secondary_label());
            right -= width + ITEM_GAP;
        }
    }
    look.spinning = view.indexing.is_some();
    if look.spinning {
        look.spin = (look.spin + ctx.dt / TURN_SECONDS).fract();
        spinner(ctx.canvas, pos2(right - RING * 0.5, rect.center().y), look.spin);
        right -= RING + ITEM_GAP;
    }
    if let Some(hit) = hit {
        let from = text_left + ctx.canvas.text_width(draft, font.clone());
        let said = match completes(draft, &hit.name) {
            Some(rest) => rest.to_owned(),
            None => format!("{DASH}{}", hit.name),
        };
        let said = crate::ui::controls::truncate(ctx.canvas, &said, font.clone(), (right - from).max(0.0));
        let dim = secondary_label().gamma_multiply(GHOST);
        ctx.canvas.text(pos2(from, rect.center().y), Align2::LEFT_CENTER, said, font.clone(), dim);
    }
    let placeholder = if typed { "" } else { PLACEHOLDER };
    let field = Bare { id: id(), rect, text_left, text_right: right.max(text_left), placeholder, font: font.clone() };
    if focus.is_none() && typed {
        ctx.canvas.text(pos2(text_left, rect.center().y), Align2::LEFT_CENTER, draft, font, label());
    }
    let edits = bare(ctx.ui, ctx.canvas, &field, focus);
    Typed { edits, cleared, pressed }
}

fn spinner(canvas: &mut Canvas, center: Pos2, phase: f32) {
    let radius = (RING - RING_W) * 0.5;
    canvas.ring(center, radius, Stroke::new(RING_W, white(RING_TRACK)));
    let start = -FRAC_PI_2 + phase * TAU;
    let arc = (0..=RING_STEPS).map(|step| {
        let angle = start + RING_ARC * TAU * step as f32 / RING_STEPS as f32;
        center + vec2(angle.cos(), angle.sin()) * radius
    });
    canvas.add(Shape::line(arc.collect(), Stroke::new(RING_W, label())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_hit_completes_from_start() {
        assert_eq!(completes("ze", "Zen"), Some("n"));
        assert_eq!(completes("ZEN", "Zen"), Some(""), "typed in full, nothing is left to add");
        assert_eq!(completes("win", "Windows 11"), Some("dows 11"));
        assert_eq!(completes("wi-f", "Wi-Fi"), Some("i"));
        assert_eq!(completes("\u{e9}t", "\u{c9}t\u{e9}.pdf"), Some("\u{e9}.pdf"), "letters, not bytes");
        assert_eq!(completes("code", "Visual Studio Code"), None, "a name that starts otherwise is not completed");
        assert_eq!(completes("zenith", "Zen"), None);
        assert_eq!(completes("", "Zen"), Some("Zen"));
    }
}
