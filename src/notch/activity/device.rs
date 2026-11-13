//! Bluetooth device notices: the device's name and glyph, and a ring for its battery.

use std::time::Instant;

use egui::{Align2, Color32, Pos2, Rect, Shape, Stroke, pos2};

use crate::notch::Notch;
use crate::notch::geometry::Geometry;
use crate::platform::Style;
use crate::system::bluetooth::{Change, Device, DeviceKind};
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{controls, font};

use super::ActivityKind;

const NOTCH_WING: f32 = 76.0;
const NOTCH_DROP: f32 = 24.0;
const ISLAND_W: f32 = 264.0;
const ISLAND_H: f32 = 37.0;
const INSET: f32 = 14.0;
const GLYPH: f32 = 17.0;
const GAP: f32 = 9.0;
const RING: f32 = 6.5;
const RING_W: f32 = 2.0;
const LOW: u8 = 20;
const CRITICAL: u8 = 10;
const CONNECTED: &str = "Connected";
const DISCONNECTED: &str = "Disconnected";

#[derive(Clone, Debug, PartialEq)]
pub(in crate::notch) struct DeviceNotice {
    pub(in crate::notch) device: Device,
    pub(in crate::notch) connected: bool,
}

impl Notch {
    pub fn show_device(&mut self, change: &Change) {
        if self.preview.is_some() || !crate::config::current().bluetooth.indicator {
            return;
        }
        let (device, connected) = match change {
            Change::Connected(device) => (device, true),
            Change::Disconnected(device) => (device, false),
            Change::Updated(device) => {
                self.refresh_device(device);
                return;
            }
        };
        let mut device = device.clone();
        if !crate::config::current().bluetooth.battery {
            device.battery = None;
        }
        let notice = DeviceNotice { device, connected };
        self.start_activity(ActivityKind::Device(notice), Instant::now());
    }

    fn refresh_device(&mut self, device: &Device) {
        if !self.activity.is_live() {
            return;
        }
        if let Some(ActivityKind::Device(notice)) = &mut self.activity.kind
            && notice.device.id == device.id
        {
            notice.device.battery = device.battery;
        }
    }

    pub(super) fn device_geometry(&self) -> Geometry {
        match self.style {
            Style::Notch => {
                Geometry { w: self.notch.x + 2.0 * NOTCH_WING, h: self.notch.y + NOTCH_DROP, r: 14.0, ear: 6.0 }
            }
            Style::Island => Geometry { w: ISLAND_W, h: ISLAND_H, r: ISLAND_H * 0.5, ear: 0.0 },
        }
    }

    pub(super) fn paint_device(&self, canvas: &mut Canvas, body: Rect, notice: &DeviceNotice, pop: f32) {
        let tone = if notice.connected { label() } else { secondary_label() };
        let pop = if notice.connected { pop } else { 1.0 };
        let lane_y = match self.style {
            Style::Notch => body.top() + self.notch.y * 0.5,
            Style::Island => body.center().y,
        };
        let icon = pos2(self.lead(body.left() + INSET) + GLYPH * 0.5, lane_y);
        glyph::paint(canvas, device_glyph(notice.device.kind), icon, GLYPH * pop, tone);
        let trailing = pos2(body.right() - INSET, lane_y);
        match self.style {
            Style::Island => {
                let status = paint_status(canvas, trailing, notice, true);
                let left = icon.x + GLYPH * 0.5 + GAP;
                let name = controls::truncate(canvas, &notice.device.name, font::callout(), status.left() - GAP - left);
                canvas.text(pos2(left, lane_y), Align2::LEFT_CENTER, name, font::callout(), tone);
            }
            Style::Notch => {
                paint_status(canvas, trailing, notice, false);
                let y = body.top() + self.notch.y + NOTCH_DROP * 0.5 - 2.0;
                let name = controls::truncate(canvas, &notice.device.name, font::callout(), body.width() - 2.0 * INSET);
                canvas.text(pos2(body.center().x, y), Align2::CENTER_CENTER, name, font::callout(), tone);
            }
        }
    }
}

fn paint_status(canvas: &mut Canvas, trailing: Pos2, notice: &DeviceNotice, worded: bool) -> Rect {
    match (notice.connected, notice.device.battery) {
        (true, Some(level)) => paint_battery(canvas, trailing, level),
        (connected, _) if worded => {
            let word = if connected { CONNECTED } else { DISCONNECTED };
            canvas.text(trailing, Align2::RIGHT_CENTER, word, font::callout(), secondary_label())
        }
        (connected, _) => {
            let at = trailing - egui::vec2(GLYPH * 0.5, 0.0);
            let tint = if connected { color::BLUE } else { secondary_label() };
            glyph::paint(canvas, Glyph::Bluetooth, at, GLYPH * 0.9, tint);
            Rect::from_center_size(at, egui::Vec2::splat(GLYPH))
        }
    }
}

fn paint_battery(canvas: &mut Canvas, trailing: Pos2, level: u8) -> Rect {
    let center = trailing - egui::vec2(RING, 0.0);
    canvas.ring(center, RING, Stroke::new(RING_W, white(0.18)));
    let share = f32::from(level.min(100)) / 100.0;
    let start = -std::f32::consts::FRAC_PI_2;
    let steps = (share * 48.0).ceil().max(1.0) as usize;
    let arc: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let angle = start + std::f32::consts::TAU * share * i as f32 / steps as f32;
            center + egui::vec2(angle.cos(), angle.sin()) * RING
        })
        .collect();
    canvas.add(Shape::line(arc, Stroke::new(RING_W, battery_color(level))));
    let percent = format!("{level}%");
    let text_at = pos2(center.x - RING - 6.0, center.y);
    let text = canvas.text(text_at, Align2::RIGHT_CENTER, percent, font::mono(12.0), secondary_label());
    text.union(Rect::from_center_size(center, egui::Vec2::splat(RING * 2.0)))
}

fn battery_color(level: u8) -> Color32 {
    match level {
        level if level <= CRITICAL => color::RED,
        level if level <= LOW => color::ORANGE,
        _ => color::GREEN,
    }
}

fn device_glyph(kind: DeviceKind) -> Glyph {
    match kind {
        DeviceKind::Headphones => Glyph::Headphones,
        DeviceKind::Speaker => Glyph::Speaker,
        DeviceKind::Keyboard => Glyph::Keyboard,
        DeviceKind::Mouse => Glyph::Mouse,
        DeviceKind::Controller => Glyph::Gamepad,
        DeviceKind::Phone => Glyph::Phone,
        DeviceKind::Computer => Glyph::Display,
        DeviceKind::Watch => Glyph::Watch,
        DeviceKind::Other => Glyph::Bluetooth,
    }
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use crate::notch::tests::fixtures::{island, settle};
    use crate::notch::{Mode, Notch};

    use super::*;

    fn airpods(battery: Option<u8>) -> Device {
        Device { id: "a".into(), name: "AirPods Pro".into(), kind: DeviceKind::Headphones, battery }
    }

    #[test]
    fn new_device_pops_battery_follows() {
        let mut n = island();
        n.show_device(&Change::Connected(airpods(None)));
        assert_eq!(n.mode, Mode::Activity);
        settle(&mut n);
        assert_eq!(n.silhouette().rect(), n.body_rect(n.device_geometry()));
        n.show_device(&Change::Updated(airpods(Some(64))));
        let Some(ActivityKind::Device(notice)) = &n.activity.kind else { panic!("a device notice") };
        assert_eq!(notice.device.battery, Some(64));
        let other = Device { id: "b".into(), ..airpods(Some(10)) };
        n.show_device(&Change::Updated(other));
        let Some(ActivityKind::Device(notice)) = &n.activity.kind else { panic!("a device notice") };
        assert_eq!(notice.device.battery, Some(64), "another device's battery leaves this one alone");
    }

    #[test]
    fn macbook_shows_wings_and_name() {
        for size in [vec2(185.0, 32.0), vec2(200.0, 38.0)] {
            let mut n = Notch::with_preview(Style::Notch, Some(size), None);
            n.show_device(&Change::Disconnected(airpods(None)));
            settle(&mut n);
            let body = n.silhouette().rect();
            assert_eq!(body.width(), size.x + 2.0 * NOTCH_WING);
            assert_eq!(body.height(), size.y + NOTCH_DROP);
            assert_eq!(body.top(), 0.0);
        }
    }

    #[test]
    fn battery_color_warns_low() {
        assert_eq!(battery_color(82), color::GREEN);
        assert_eq!(battery_color(20), color::ORANGE);
        assert_eq!(battery_color(7), color::RED);
    }
}
