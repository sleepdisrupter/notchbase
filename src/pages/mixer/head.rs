//! The head row: the output slider and the button that unfolds the rest of the mixer.

use egui::{Color32, Id, Rect, Response, Sense, Vec2, pos2};

use crate::audio::{DeviceKind, Mixer};
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label};
use crate::ui::controls::{self, Thickness};
use crate::ui::glass::{self, Glass};
use crate::ui::spring::Spring;
use crate::ui::{glyph, motion};

use super::devices::ChannelArea;
use super::layout::{FOLD_BUTTON, FOLD_ROW, FOLD_TRACK, FOLD_TRACK_HOVER, Fold};
use super::{MixerPage, channel};

const CHEVRON: f32 = 9.0;
const TRACK_GAP: f32 = 8.0;
const TRACK: Thickness = Thickness { rest: FOLD_TRACK, hover: FOLD_TRACK_HOVER };

pub(super) struct Disclosure {
    open: bool,
    reveal: Spring,
}

impl Default for Disclosure {
    fn default() -> Self {
        Self { open: false, reveal: Spring::new(0.0, motion::SMOOTH, 0.001) }
    }
}

impl Disclosure {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn reveal(&self) -> f32 {
        self.reveal.value
    }

    pub(super) fn toggle(&mut self) {
        self.open = !self.open;
        self.reveal.animate_to(self.open as u8 as f32, motion::SMOOTH);
    }

    pub(super) fn open_now(&mut self) {
        self.open = true;
        self.reveal.snap_to(1.0);
    }

    pub(super) fn step(&mut self, dt: f32) -> bool {
        self.reveal.step(dt);
        !self.reveal.is_settled()
    }
}

struct HeadRow {
    mute: Rect,
    slider: Rect,
    button: Rect,
}

impl HeadRow {
    fn within(left: f32, right: f32, top: f32) -> Self {
        let cy = top + FOLD_ROW * 0.5;
        let well = |x: f32| Rect::from_center_size(pos2(x, cy), Vec2::splat(FOLD_BUTTON));
        let mute = well(left + FOLD_BUTTON * 0.5);
        let button = well(right - FOLD_BUTTON * 0.5);
        let slider =
            Rect::from_min_max(pos2(mute.right() + TRACK_GAP, top), pos2(button.left() - TRACK_GAP, top + FOLD_ROW));
        Self { mute, slider, button }
    }
}

impl MixerPage {
    pub(super) fn head_row(&mut self, ctx: &mut PageCtx<'_>, mixer: &Mixer, fold: Fold, actions: &mut Vec<Action>) {
        let row = HeadRow::within(ctx.rect.left(), ctx.rect.right(), ctx.rect.top() + fold.top);
        let area = ChannelArea { slider: row.slider, mute: row.mute, well: true, thickness: Some(TRACK) };
        self.channel_slider(ctx, DeviceKind::Output, channel(mixer, DeviceKind::Output), area, actions);
        self.fold_button(ctx, row.button, fold.reveal);
    }

    fn fold_button(&mut self, ctx: &mut PageCtx<'_>, rect: Rect, turn: f32) {
        let (response, tint) = round_button(ctx.ui, ctx.canvas, rect, self.scope.with("fold"));
        glyph::chevron(ctx.canvas, rect.center(), CHEVRON, turn, tint);
        if response.clicked() && ctx.alpha >= 0.5 {
            self.toggle_disclosure();
        }
    }

    fn toggle_disclosure(&mut self) {
        let Some(disclosure) = self.disclosure.as_mut() else { return };
        disclosure.toggle();
        if !disclosure.is_open() {
            self.picker.close();
        }
    }
}

pub(super) fn round_button(ui: &egui::Ui, canvas: &mut Canvas, rect: Rect, id: Id) -> (Response, Color32) {
    let response = ui.interact(rect, id, Sense::click());
    let hover = controls::animate(ui, id.with("hover"), response.hovered());
    glass::paint(canvas, rect, rect.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    (response, color::mix(secondary_label(), label(), hover))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclosure_springs_open_and_shut() {
        let mut d = Disclosure::default();
        assert!(!d.is_open());
        assert!(!d.step(0.016));
        d.toggle();
        assert!(d.is_open());
        assert!(d.step(0.016));
        assert!(d.reveal() > 0.0 && d.reveal() < 1.0);
        for _ in 0..240 {
            d.step(0.016);
        }
        assert_eq!(d.reveal(), 1.0);
        d.toggle();
        assert!(!d.is_open());
        d.open_now();
        assert_eq!((d.is_open(), d.reveal(), d.step(0.016)), (true, 1.0, false));
    }

    #[test]
    fn head_row_track_between_wells() {
        let row = HeadRow::within(10.0, 410.0, 50.0);
        assert_eq!((row.mute.size(), row.button.size()), (Vec2::splat(FOLD_BUTTON), Vec2::splat(FOLD_BUTTON)));
        assert_eq!((row.mute.left(), row.button.right()), (10.0, 410.0));
        assert_eq!(row.mute.center().y, row.slider.center().y);
        assert_eq!(row.button.center().y, row.slider.center().y);
        assert_eq!((row.slider.top(), row.slider.height()), (50.0, FOLD_ROW));
        assert_eq!(row.slider.left(), 10.0 + FOLD_BUTTON + TRACK_GAP);
        assert_eq!(row.slider.right(), 410.0 - FOLD_BUTTON - TRACK_GAP);
    }
}
