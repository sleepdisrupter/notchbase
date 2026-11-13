//! The device sections: the output or input in use, its volume, and the list to pick another.

use egui::{Rect, Sense, Vec2, pos2, vec2};

use crate::audio::{AppId, AudioApp, AudioDevice, DeviceForm, DeviceKind, INPUT, MASTER, Mixer};
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::ui::canvas::Canvas;
use crate::ui::controls::{self, Slider, Thickness};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::smoothstep;
use crate::ui::{font, metrics, motion};

use super::head::round_button;
use super::layout::{LIST_GAP, LIST_ROWS, ROW, Section, TITLE_GAP, TITLE_H};
use super::{MixerPage, channel, fade_into, kind_index};

const BUTTON_CHROME: f32 = 40.0;
const BUTTON_MIN_W: f32 = 60.0;
const BUTTON_MAX_SHARE: f32 = 0.62;
const NO_DEVICE: &str = "None";
const MUTE_GLYPH: f32 = 0.52;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChannelArea {
    pub(super) slider: Rect,
    pub(super) mute: Rect,
    pub(super) well: bool,
    pub(super) thickness: Option<Thickness>,
}

impl ChannelArea {
    fn inline(slider: Rect) -> Self {
        let mute = Rect::from_min_size(slider.min, Vec2::splat(slider.height()));
        Self { slider, mute, well: false, thickness: None }
    }
}

struct List<'a> {
    kind: DeviceKind,
    devices: &'a [AudioDevice],
    open: bool,
    top: f32,
    visible_bottom: f32,
    button: Option<Rect>,
}

impl MixerPage {
    pub(super) fn device_section(
        &mut self,
        ctx: &mut PageCtx<'_>,
        mixer: &Mixer,
        kind: DeviceKind,
        section: Section,
        actions: &mut Vec<Action>,
    ) {
        let devices = mixer.devices(kind);
        let channel_row = channel(mixer, kind);
        let open = self.picker.is_open(kind);
        let top = ctx.rect.top() + section.top;
        controls::section_header(ctx.canvas, pos2(ctx.rect.left(), top + TITLE_H * 0.5), title(kind));
        let current = current_name(devices, channel_row);
        let button = (!devices.is_empty()).then(|| self.device_button(ctx, kind, devices, current, top));
        if section.slider {
            let slider = Rect::from_min_size(
                pos2(ctx.rect.left(), top + TITLE_H + TITLE_GAP),
                vec2(ctx.rect.width(), metrics::SLIDER),
            );
            self.channel_slider(ctx, kind, channel_row, ChannelArea::inline(slider), actions);
        }
        if section.list <= 0.5 || devices.is_empty() {
            return;
        }
        let head = top + section.head();
        let list = List { kind, devices, open, top: head + LIST_GAP, visible_bottom: head + section.list, button };
        self.device_list(ctx, list, actions);
    }

    fn device_button(
        &mut self,
        ctx: &mut PageCtx<'_>,
        kind: DeviceKind,
        devices: &[AudioDevice],
        current: &str,
        top: f32,
    ) -> Rect {
        let max_w = (ctx.rect.width() * BUTTON_MAX_SHARE).max(BUTTON_MIN_W);
        let fits = |text: &str| ctx.canvas.text_width(text, font::callout()) + BUTTON_CHROME <= max_w;
        let current = if fits(current) { current } else { short_device_name(current) };
        let w = (ctx.canvas.text_width(current, font::callout()) + BUTTON_CHROME).clamp(BUTTON_MIN_W, max_w);
        let right = ctx.rect.right();
        let rect = Rect::from_min_max(pos2(right - w, top), pos2(right, top + TITLE_H));
        let id = self.scope.with(("popup", kind_index(kind)));
        let response = controls::popup_button(ctx.ui, ctx.canvas, rect, id, current, self.picker.is_open(kind));
        if response.clicked() && ctx.alpha >= 0.5 {
            self.picker.toggle(kind, devices);
        }
        rect
    }

    pub(super) fn channel_slider(
        &mut self,
        ctx: &mut PageCtx<'_>,
        kind: DeviceKind,
        row: Option<&AudioApp>,
        area: ChannelArea,
        actions: &mut Vec<Action>,
    ) {
        let interactive = ctx.alpha >= 0.5;
        let id = channel_id(kind);
        let (now, scope) = (ctx.now, self.scope);
        let state = self.row_state(id, row, 0.0);
        let (volume, muted) = match row {
            Some(r) => (state.shown_volume(r.volume, now), state.shown_muted(r.muted, now)),
            None => (0.0, false),
        };
        state.mute.animate_to(muted as u8 as f32, motion::SMOOTH);
        state.meter.listen(if muted { 0.0 } else { row.map_or(0.0, |r| r.peak) });
        let reading = state.meter.reading();
        let level = level_glyph(kind, muted || volume <= 0.0);
        let result = Slider {
            value: state.volume.value.clamp(0.0, 1.0),
            level: reading.is_visible().then_some(reading),
            leading: (!area.well).then_some(level),
            dimmed: state.mute.value,
            enabled: interactive && row.is_some(),
            thickness: area.thickness,
        }
        .show(ctx.ui, ctx.canvas, area.slider, scope.with((id, "slider")));
        let down = result.response.is_pointer_button_down_on();
        match result.changed {
            Some(v) => {
                state.volume.snap_to(v);
                state.send_volume(id, v, muted, now, actions);
            }
            None => {
                if !down {
                    state.last_sent = None;
                }
                state.volume.animate_to(volume, motion::FOLLOW);
            }
        }
        let mute_id = scope.with((id, "mute"));
        let toggle = if area.well {
            let (response, tint) = round_button(ctx.ui, ctx.canvas, area.mute, mute_id);
            glyph::paint(ctx.canvas, level, area.mute.center(), area.mute.height() * MUTE_GLYPH, tint);
            response
        } else {
            ctx.ui.interact(area.mute, mute_id, Sense::click())
        };
        if toggle.clicked() && interactive && row.is_some() {
            state.send_muted(id, !muted, now, actions);
        }
        self.track_drag(id, down);
    }

    fn device_list(&mut self, ctx: &mut PageCtx<'_>, list: List<'_>, actions: &mut Vec<Action>) {
        let count = list.devices.len();
        let shown = count.min(LIST_ROWS);
        let max_first = count - shown;
        let (left, width) = (ctx.rect.left(), ctx.rect.width());
        let list_rect = Rect::from_min_size(pos2(left, list.top), vec2(width, shown as f32 * ROW));
        if list.open {
            self.picker.claim(list.button.map_or(list_rect, |b| b.union(list_rect)));
            if max_first > 0 && ctx.ui.rect_contains_pointer(list_rect) {
                self.picker.scroll(list.kind, crate::ui::scroll::down(ctx.ui), max_first);
            }
        }
        let first = self.picker.first_row(list.kind, max_first);

        let clickable = ctx.alpha >= 0.5 && list.open;
        for (i, device) in list.devices.iter().skip(first).take(shown).enumerate() {
            let row_top = list.top + i as f32 * ROW;
            let reveal = ((list.visible_bottom - row_top) / ROW).clamp(0.0, 1.0);
            if reveal <= 0.01 {
                break;
            }
            let rect = Rect::from_min_size(pos2(left, row_top), vec2(width, ROW));
            let mut c = Canvas::new(ctx.ui.painter());
            let id = self.scope.with(("device", kind_index(list.kind), device.id.as_str()));
            let response = controls::device_row(
                ctx.ui,
                &mut c,
                rect,
                id,
                Some(form_glyph(device.form)),
                &device.name,
                device.is_default,
            );
            fade_into(ctx.canvas, c, smoothstep(0.0, 1.0, reveal));
            if response.clicked() && clickable && reveal > 0.5 {
                self.select_device(list.kind, device, actions);
            }
        }
    }

    fn select_device(&mut self, kind: DeviceKind, device: &AudioDevice, actions: &mut Vec<Action>) {
        if !device.is_default {
            actions.push(Action::SetDefaultDevice(kind, device.id.clone()));
        }
        self.picker.close();
    }
}

fn current_name<'a>(devices: &'a [AudioDevice], channel: Option<&'a AudioApp>) -> &'a str {
    devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| d.name.as_str())
        .or_else(|| channel.map(|c| c.name.as_str()))
        .unwrap_or(NO_DEVICE)
}

fn title(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Output => "Output",
        DeviceKind::Input => "Input",
    }
}

fn channel_id(kind: DeviceKind) -> AppId {
    match kind {
        DeviceKind::Output => MASTER,
        DeviceKind::Input => INPUT,
    }
}

fn level_glyph(kind: DeviceKind, silent: bool) -> Glyph {
    match (kind, silent) {
        (DeviceKind::Output, false) => Glyph::Speaker,
        (DeviceKind::Output, true) => Glyph::SpeakerMuted,
        (DeviceKind::Input, false) => Glyph::Microphone,
        (DeviceKind::Input, true) => Glyph::MicrophoneMuted,
    }
}

fn form_glyph(form: DeviceForm) -> Glyph {
    match form {
        DeviceForm::Speakers => Glyph::Speaker,
        DeviceForm::Headphones => Glyph::Headphones,
        DeviceForm::Display => Glyph::Display,
        DeviceForm::Microphone => Glyph::Microphone,
        DeviceForm::Other => Glyph::Device,
    }
}

fn short_device_name(name: &str) -> &str {
    match name.find(" (") {
        Some(i) if !name[..i].trim().is_empty() => name[..i].trim_end(),
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_shorten() {
        assert_eq!(short_device_name("Headphones (Realtek(R) Audio)"), "Headphones");
        assert_eq!(short_device_name("MacBook Pro Speakers"), "MacBook Pro Speakers");
        assert_eq!(short_device_name(" (odd)"), " (odd)");
    }

    #[test]
    fn button_names_device_then_channel() {
        let device = |name: &str, is_default| AudioDevice {
            id: name.into(),
            name: name.into(),
            kind: DeviceKind::Output,
            is_default,
            form: DeviceForm::Speakers,
        };
        let channel =
            AudioApp { id: MASTER, name: "Speakers".into(), volume: 1.0, muted: false, peak: 0.0, icon: None };
        assert_eq!(current_name(&[device("A", false), device("B", true)], Some(&channel)), "B");
        assert_eq!(current_name(&[device("A", false)], Some(&channel)), "Speakers");
        assert_eq!(current_name(&[], None), NO_DEVICE);
    }

    #[test]
    fn silent_channels_show_muted_glyphs() {
        assert!(matches!(level_glyph(DeviceKind::Output, true), Glyph::SpeakerMuted));
        assert!(matches!(level_glyph(DeviceKind::Input, false), Glyph::Microphone));
    }
}
