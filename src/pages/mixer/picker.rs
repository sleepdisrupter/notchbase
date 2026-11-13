//! The device list that drops open from a device button.

use egui::Rect;

use crate::audio::{AudioDevice, DeviceKind};
use crate::ui::motion;
use crate::ui::spring::Spring;

use super::layout::{Content, LIST_ROWS};
use super::{KINDS, kind_index};

pub(super) struct Picker {
    open: Option<DeviceKind>,
    openness: [Spring; 2],
    first: [usize; 2],
    wheel: f32,
    area: Option<Rect>,
}

impl Default for Picker {
    fn default() -> Self {
        Self {
            open: None,
            openness: [Spring::new(0.0, motion::SMOOTH, 0.001), Spring::new(0.0, motion::SMOOTH, 0.001)],
            first: [0, 0],
            wheel: 0.0,
            area: None,
        }
    }
}

impl Picker {
    pub(super) fn is_open(&self, kind: DeviceKind) -> bool {
        self.open == Some(kind)
    }

    pub(super) fn openness(&self) -> [f32; 2] {
        self.openness.each_ref().map(|s| s.value)
    }

    pub(super) fn close(&mut self) {
        self.open = None;
    }

    pub(super) fn close_if_empty(&mut self, content: &Content) {
        if let Some(kind) = self.open
            && content.sections[kind_index(kind)].is_none_or(|n| n == 0)
        {
            self.open = None;
        }
    }

    pub(super) fn toggle(&mut self, kind: DeviceKind, devices: &[AudioDevice]) {
        if self.is_open(kind) {
            self.open = None;
            return;
        }
        self.open = Some(kind);
        self.first[kind_index(kind)] = first_row_showing(devices.iter().position(|d| d.is_default), devices.len());
        self.wheel = 0.0;
    }

    pub(super) fn first_row(&mut self, kind: DeviceKind, max_first: usize) -> usize {
        let first = &mut self.first[kind_index(kind)];
        *first = (*first).min(max_first);
        *first
    }

    pub(super) fn claim(&mut self, area: Rect) {
        self.area = Some(area);
    }

    pub(super) fn scroll(&mut self, kind: DeviceKind, notches: f32, max_first: usize) {
        let first = &mut self.first[kind_index(kind)];
        self.wheel += notches;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            *first = first.saturating_sub(1);
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            *first = (*first + 1).min(max_first);
        }
    }

    pub(super) fn close_on_press_outside(&mut self, ui: &egui::Ui) {
        let Some(area) = self.area.take() else { return };
        let outside =
            ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| !area.contains(p)));
        if outside {
            self.open = None;
        }
    }

    pub(super) fn animate(&mut self) {
        for kind in KINDS {
            let target = self.is_open(kind) as u8 as f32;
            self.openness[kind_index(kind)].animate_to(target, motion::SMOOTH);
        }
    }

    pub(super) fn step(&mut self, dt: f32) -> bool {
        let mut animating = false;
        for spring in &mut self.openness {
            spring.step(dt);
            animating |= !spring.is_settled();
        }
        animating
    }
}

fn first_row_showing(selected: Option<usize>, count: usize) -> usize {
    let max_first = count.saturating_sub(LIST_ROWS);
    selected.map_or(0, |i| (i + 1).saturating_sub(LIST_ROWS)).min(max_first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_device_scrolls_into_view() {
        assert_eq!(first_row_showing(None, 9), 0);
        assert_eq!(first_row_showing(Some(4), 9), 0);
        assert_eq!(first_row_showing(Some(5), 9), 1);
        assert_eq!(first_row_showing(Some(8), 9), 4);
        assert_eq!(first_row_showing(Some(3), 3), 0);
    }

    #[test]
    fn wheel_scrolls_whole_rows() {
        let mut picker = Picker::default();
        picker.scroll(DeviceKind::Output, -2.5, 3);
        assert_eq!(picker.first_row(DeviceKind::Output, 3), 2);
        picker.scroll(DeviceKind::Output, -9.0, 3);
        assert_eq!(picker.first_row(DeviceKind::Output, 3), 3);
        picker.scroll(DeviceKind::Output, 1.5, 3);
        assert_eq!(picker.first_row(DeviceKind::Output, 3), 2);
        assert_eq!(picker.first_row(DeviceKind::Output, 1), 1);
        assert_eq!(picker.first_row(DeviceKind::Input, 3), 0);
    }
}
