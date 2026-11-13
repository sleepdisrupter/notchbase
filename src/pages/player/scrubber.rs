//! The progress bar between the time played and the time left.

use std::time::Duration;

use egui::{Align2, Id, Rect, Sense, pos2, vec2};
use notchbase_sdk::clock;

use crate::pages::PageCtx;
use crate::ui::color::{secondary_label, white};
use crate::ui::spring::lerp;
use crate::ui::{controls, font};

pub(super) const HEIGHT: f32 = 16.0;
const THIN: f32 = 4.0;
const THICK: f32 = 7.0;
const TRACK: f32 = 0.16;
const DIGIT: f32 = 6.4;
const TIME_GAP: f32 = 10.0;

#[derive(Default)]
pub(super) struct Scrubber {
    dragging: Option<f64>,
}

impl Scrubber {
    pub(super) fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    pub(super) fn ui(
        &mut self,
        ctx: &mut PageCtx<'_>,
        area: Rect,
        position: f64,
        duration: f64,
        interactive: bool,
    ) -> Option<f64> {
        let sense = if interactive { Sense::click_and_drag() } else { Sense::hover() };
        let times = area;
        let area = bar(area, duration);
        let response = ctx.ui.interact(area, Id::new("notchbase.media.scrubber"), sense);
        let grip = controls::animate(
            ctx.ui,
            Id::new("notchbase.media.scrubber.grip"),
            response.hovered() || response.dragged(),
        );
        let at_pointer = |x: f32| f64::from(((x - area.left()) / area.width()).clamp(0.0, 1.0)) * duration;
        let held = interactive && response.is_pointer_button_down_on();
        let mut released = None;
        match response.interact_pointer_pos() {
            Some(pointer) if held => self.dragging = Some(at_pointer(pointer.x)),
            _ if !held => released = self.dragging.take(),
            _ => {}
        }
        let shown = self.dragging.or(released).unwrap_or(position).clamp(0.0, duration);
        paint(ctx, area, shown, duration, grip);
        paint_times(ctx, times, shown, duration);
        released
    }
}

fn paint(ctx: &mut PageCtx<'_>, area: Rect, position: f64, duration: f64, grip: f32) {
    let h = lerp(THIN, THICK, grip);
    let track = Rect::from_center_size(area.center(), vec2(area.width(), h));
    ctx.canvas.rect(track, h * 0.5, white(TRACK));
    let fraction = if duration > 0.0 { (position / duration) as f32 } else { 0.0 };
    let fill = Rect::from_min_size(track.min, vec2((track.width() * fraction).max(h), h));
    ctx.canvas.rect(fill, h * 0.5, white(lerp(0.8, 1.0, grip)));
}

fn paint_times(ctx: &mut PageCtx<'_>, area: Rect, position: f64, duration: f64) {
    let y = area.center().y;
    let tint = secondary_label();
    let elapsed = time_label(position);
    let remaining = format!("-{}", time_label(duration - position));
    ctx.canvas.text(pos2(area.left(), y), Align2::LEFT_CENTER, elapsed, font::subheadline(), tint);
    ctx.canvas.text(pos2(area.right(), y), Align2::RIGHT_CENTER, remaining, font::subheadline(), tint);
}

fn time_room(duration: f64) -> f32 {
    (time_label(duration).len() + 1) as f32 * DIGIT + TIME_GAP
}

fn bar(area: Rect, duration: f64) -> Rect {
    let room = time_room(duration).min(area.width() * 0.25);
    Rect::from_min_max(pos2(area.left() + room, area.top()), pos2(area.right() - room, area.bottom()))
}

fn time_label(seconds: f64) -> String {
    clock(Duration::from_secs_f64(seconds.max(0.0).floor()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_sits_between_times() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(340.0, HEIGHT));
        let short = bar(area, 200.0);
        assert_eq!(short.left(), 340.0 - short.right());
        assert_eq!(short.left(), 5.0 * DIGIT + TIME_GAP);
        assert!(bar(area, 4000.0).width() < short.width());
        assert_eq!(bar(area, 199.0), short, "the bar never moves as the seconds tick");
    }

    #[test]
    fn times_are_whole_seconds() {
        assert_eq!(time_label(62.9), "1:02");
        assert_eq!(time_label(-1.0), "0:00");
    }
}
