//! One app's row: its icon, its name, a volume slider and mute.

use std::time::Instant;

use egui::{Align2, Id, Rect, Response, Sense, Vec2, pos2};

use crate::audio::{AppId, AudioApp};
use crate::bridge::Action;
use crate::ui::canvas::Canvas;
use crate::ui::meter::Reading;
use crate::ui::{color, controls, font, motion};

use super::MixerPage;
use super::thin_slider::{KNOB_R, KNOB_R_DRAG, ThinSlider};

const ICON: f32 = 20.0;
const ICON_GAP: f32 = 10.0;
const ICON_HIT_SLOP: f32 = 4.0;
const ICON_PRESS_SHRINK: f32 = 0.08;
const NAME_W: f32 = 118.0;
const NAME_GAP: f32 = 10.0;
const WHEEL_STEP: f32 = 0.02;

pub(super) struct RowFrame {
    row: Rect,
    icon: Rect,
    name_x: f32,
    slider: ThinSlider,
    now: Instant,
    live: bool,
    toggle: Response,
    drag: Response,
    down: bool,
    pressed: f32,
    hover: f32,
    icon_hover: f32,
    wheel: f32,
}

impl RowFrame {
    pub(super) fn read(ui: &egui::Ui, scope: Id, id: AppId, row: Rect, live: bool, now: Instant) -> Self {
        let icon = Rect::from_center_size(pos2(row.left() + ICON * 0.5, row.center().y), Vec2::splat(ICON));
        let name_x = icon.right() + ICON_GAP;
        let slider = Rect::from_min_max(pos2(name_x + NAME_W + NAME_GAP, row.top()), pos2(row.right(), row.bottom()));

        let toggle = ui.interact(icon.expand(ICON_HIT_SLOP), scope.with((id, "icon")), Sense::click());
        let sense = if live { Sense::click_and_drag() } else { Sense::hover() };
        let drag = ui.interact(slider, scope.with((id, "slider")), sense);
        let over_row = live && ui.rect_contains_pointer(row);
        let hover = controls::animate(ui, scope.with((id, "hover")), over_row);
        let icon_hover = controls::animate(ui, scope.with((id, "icon-hover")), live && toggle.hovered());
        let down = live && drag.is_pointer_button_down_on();
        let pressed = (toggle.is_pointer_button_down_on() && live) as u8 as f32;
        let wheel = if over_row { crate::ui::scroll::across(ui) } else { 0.0 };
        Self {
            row,
            icon,
            name_x,
            slider: ThinSlider::within(slider),
            now,
            live,
            toggle,
            drag,
            down,
            pressed,
            hover,
            icon_hover,
            wheel,
        }
    }
}

struct RowLook {
    name: String,
    mute: f32,
    volume: f32,
    level: Reading,
    knob_r: f32,
}

impl MixerPage {
    pub(super) fn app_row(
        &mut self,
        ui_ctx: &egui::Context,
        c: &mut Canvas,
        app: &AudioApp,
        frame: &RowFrame,
        actions: &mut Vec<Action>,
    ) {
        let look = self.drive_row(c, app, frame, actions);
        self.track_drag(app.id, frame.down);
        let icon =
            Rect::from_center_size(frame.icon.center(), frame.icon.size() * (1.0 - ICON_PRESS_SHRINK * frame.pressed));
        self.icons.paint(ui_ctx, c, icon, app, look.mute, frame.icon_hover);
        let name_color = color::mix(color::label(), color::secondary_label(), look.mute);
        c.text(pos2(frame.name_x, frame.row.center().y), Align2::LEFT_CENTER, look.name, font::body(), name_color);
        frame.slider.paint(c, look.volume, look.level, look.mute, look.knob_r);
    }

    fn drive_row(&mut self, c: &Canvas, app: &AudioApp, frame: &RowFrame, actions: &mut Vec<Action>) -> RowLook {
        let (id, now) = (app.id, frame.now);
        let state = self.row_state(id, Some(app), 0.0);
        let volume = state.shown_volume(app.volume, now);
        let muted = state.shown_muted(app.muted, now);
        if frame.toggle.clicked() && frame.live {
            state.send_muted(id, !muted, now, actions);
        }
        if frame.down
            && let Some(p) = frame.drag.interact_pointer_pos()
        {
            let v = frame.slider.value_at(p.x);
            state.volume.snap_to(v);
            state.send_volume(id, v, muted, now, actions);
        } else {
            state.last_sent = None;
            if frame.wheel != 0.0 {
                let v = (volume + frame.wheel * WHEEL_STEP).clamp(0.0, 1.0);
                state.send_volume(id, v, muted, now, actions);
                state.last_sent = None;
                state.volume.animate_to(v, motion::SMOOTH);
            } else {
                state.volume.animate_to(volume, motion::FOLLOW);
            }
        }
        let muted_now = state.shown_muted(app.muted, now);
        state.meter.listen(if muted_now { 0.0 } else { app.peak });
        state.mute.animate_to(muted_now as u8 as f32, motion::SMOOTH);
        state.knob.animate_to(if frame.down { KNOB_R_DRAG } else { KNOB_R + 0.5 * frame.hover }, motion::PRESS);
        RowLook {
            name: state.fitted_name(c, &app.name, NAME_W),
            mute: state.mute.value.clamp(0.0, 1.0),
            volume: state.volume.value.clamp(0.0, 1.0),
            level: state.meter.reading(),
            knob_r: state.knob.value,
        }
    }
}
