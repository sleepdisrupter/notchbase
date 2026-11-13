//! Where each section goes and how tall the page is, open or folded.

use crate::audio::{DeviceKind, Mixer};
use crate::ui::metrics;

use super::{KINDS, channel};

pub(super) const TITLE_H: f32 = 20.0;
pub(super) const TITLE_GAP: f32 = 6.0;
const SECTION_GAP: f32 = 12.0;
pub(super) const APPS_TITLE_H: f32 = 16.0;
pub(super) const APPS_GAP: f32 = 4.0;
pub(super) const ROW: f32 = 28.0;
pub(super) const LIST_GAP: f32 = 6.0;
pub(super) const LIST_ROWS: usize = 5;
pub(super) const APP_ROWS: usize = 4;
pub(super) const EMPTY_H: f32 = 40.0;
pub(super) const REASON_H: f32 = 32.0;
pub(super) const FOLD_TRACK: f32 = 8.0;
pub(super) const FOLD_TRACK_HOVER: f32 = 12.0;
pub(super) const FOLD_BUTTON: f32 = metrics::SLIDER;
pub(super) const FOLD_ROW: f32 = if FOLD_BUTTON > FOLD_TRACK_HOVER { FOLD_BUTTON } else { FOLD_TRACK_HOVER };

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Content {
    pub(super) sections: [Option<usize>; 2],
    pub(super) apps: AppsBody,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum AppsBody {
    Rows(usize),
    Empty,
    Reason,
}

impl Content {
    pub(super) fn of(mixer: &Mixer, app_rows: usize) -> Self {
        let section = |kind: DeviceKind| {
            let devices = mixer.devices(kind).len();
            (channel(mixer, kind).is_some() || devices > 0).then_some(devices)
        };
        let apps = if !mixer.is_supported() {
            AppsBody::Reason
        } else if app_rows == 0 {
            AppsBody::Empty
        } else {
            AppsBody::Rows(app_rows.min(APP_ROWS))
        };
        Self { sections: KINDS.map(section), apps }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Section {
    pub(super) top: f32,
    pub(super) slider: bool,
    pub(super) list: f32,
}

impl Section {
    pub(super) fn head(self) -> f32 {
        TITLE_H + if self.slider { TITLE_GAP + metrics::SLIDER } else { 0.0 }
    }

    fn height(self) -> f32 {
        self.head() + self.list + SECTION_GAP
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Fold {
    pub(super) top: f32,
    pub(super) reveal: f32,
}

impl Fold {
    pub(super) fn bottom(self) -> f32 {
        self.top + FOLD_ROW
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) sections: [Option<Section>; 2],
    pub(super) fold: Option<Fold>,
    pub(super) apps_top: f32,
    pub(super) apps_h: f32,
    pub(super) height: f32,
}

impl Layout {
    pub(super) fn reveal(&self) -> f32 {
        self.fold.map_or(1.0, |f| f.reveal)
    }
}

fn list_height(devices: usize) -> f32 {
    let rows = devices.min(LIST_ROWS);
    if rows == 0 { 0.0 } else { LIST_GAP + rows as f32 * ROW }
}

pub(super) fn apps_target(body: AppsBody, reason_h: f32) -> f32 {
    match body {
        AppsBody::Rows(n) => n as f32 * ROW,
        AppsBody::Empty => EMPTY_H,
        AppsBody::Reason => reason_h,
    }
}

pub(super) fn layout(content: Content, open: [f32; 2], apps_h: f32, fold: Option<f32>) -> Layout {
    let fold = fold.map(|reveal| Fold { top: 0.0, reveal: reveal.clamp(0.0, 1.0) });
    let mut y = fold.map_or(0.0, |f| f.bottom() + SECTION_GAP);
    let mut sections = [None; 2];
    for (i, slot) in sections.iter_mut().enumerate() {
        let Some(devices) = content.sections[i] else { continue };
        let slider = i == 1 || fold.is_none();
        let section = Section { top: y, slider, list: list_height(devices) * open[i].clamp(0.0, 1.0) };
        y += section.height();
        *slot = Some(section);
    }
    let apps_h = apps_h.max(0.0);
    let full = y + APPS_TITLE_H + APPS_GAP + apps_h;
    let height = fold.map_or(full, |f| f.bottom() + (full - f.bottom()) * f.reveal);
    Layout { sections, fold, apps_top: y, apps_h, height }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECTION: f32 = TITLE_H + TITLE_GAP + metrics::SLIDER + SECTION_GAP;

    #[test]
    fn stacks_sections_and_lists() {
        let content = Content { sections: [Some(3), Some(2)], apps: AppsBody::Rows(2) };
        let apps = apps_target(content.apps, REASON_H);
        let closed = layout(content, [0.0, 0.0], apps, None);
        assert_eq!(closed.sections[0].unwrap().top, 0.0);
        assert_eq!(closed.sections[1].unwrap().top, SECTION);
        assert_eq!(closed.apps_top, 2.0 * SECTION);
        assert_eq!(closed.height, 2.0 * SECTION + APPS_TITLE_H + APPS_GAP + 2.0 * ROW);

        let open = layout(content, [1.0, 0.0], apps, None);
        let list = LIST_GAP + 3.0 * ROW;
        assert_eq!(open.sections[0].unwrap().list, list);
        assert_eq!(open.sections[1].unwrap().top, SECTION + list);
        assert_eq!(open.height, closed.height + list);

        assert_eq!(layout(content, [0.5, 0.0], apps, None).height, closed.height + list * 0.5);
        assert_eq!(layout(content, [-0.1, 0.0], apps, None).height, closed.height);
        assert_eq!(layout(content, [1.2, 0.0], apps, None).height, open.height);
    }

    #[test]
    fn tallest_page_fits_the_notch() {
        let content = Content { sections: [Some(12), Some(12)], apps: AppsBody::Rows(APP_ROWS) };
        let tallest = layout(content, [1.0, 0.0], apps_target(content.apps, REASON_H), None).height;
        assert!(tallest <= 420.0, "tallest Sound page is {tallest} pt");
        let closed = layout(content, [0.0, 0.0], apps_target(content.apps, REASON_H), None).height;
        assert!(closed <= 260.0, "closed Sound page is {closed} pt");
    }

    #[test]
    fn fold_keeps_output_slider() {
        let content = Content { sections: [Some(3), Some(2)], apps: AppsBody::Rows(2) };
        let apps = apps_target(content.apps, REASON_H);
        let all = layout(content, [0.0, 0.0], apps, None);
        let closed = layout(content, [0.0, 0.0], apps, Some(0.0));
        let fold = closed.fold.unwrap();
        assert_eq!((fold.top, fold.bottom()), (0.0, FOLD_ROW));
        assert_eq!(closed.height, FOLD_ROW);
        assert_eq!(closed.reveal(), 0.0);

        let open = layout(content, [0.0, 0.0], apps, Some(1.0));
        let output = open.sections[0].unwrap();
        assert_eq!((output.top, output.slider, output.head()), (FOLD_ROW + SECTION_GAP, false, TITLE_H));
        let input = open.sections[1].unwrap();
        assert_eq!((input.top, input.slider), (output.top + TITLE_H + SECTION_GAP, true));
        assert_eq!(open.height, all.height + FOLD_ROW + SECTION_GAP - TITLE_GAP - metrics::SLIDER);
        let half = layout(content, [0.0, 0.0], apps, Some(0.5)).height;
        assert_eq!(half, (closed.height + open.height) * 0.5);
        assert_eq!(layout(content, [0.0, 0.0], apps, Some(1.4)).height, open.height);

        let picking = layout(content, [1.0, 0.0], apps, Some(1.0));
        assert_eq!(picking.sections[1].unwrap().top, input.top + LIST_GAP + 3.0 * ROW);
        assert_eq!(layout(content, [1.0, 0.0], apps, Some(0.0)).height, closed.height);
        assert_eq!(all.reveal(), 1.0);
        assert!(all.sections.iter().flatten().all(|s| s.slider));
    }

    #[test]
    fn fold_row_has_buttons_and_track() {
        const { assert!(FOLD_ROW >= FOLD_BUTTON && FOLD_ROW >= FOLD_TRACK_HOVER) };
        const { assert!(FOLD_TRACK < FOLD_TRACK_HOVER && FOLD_TRACK < metrics::SLIDER) };
    }

    #[test]
    fn lists_cap_visible_rows() {
        assert_eq!(list_height(0), 0.0);
        assert_eq!(list_height(1), LIST_GAP + ROW);
        assert_eq!(list_height(40), LIST_GAP + LIST_ROWS as f32 * ROW);
    }

    #[test]
    fn hidden_sections_take_no_space() {
        let content = Content { sections: [Some(1), None], apps: AppsBody::Empty };
        let l = layout(content, [0.0, 1.0], apps_target(content.apps, REASON_H), None);
        assert!(l.sections[1].is_none());
        assert_eq!(l.height, SECTION + APPS_TITLE_H + APPS_GAP + EMPTY_H);
        let unsupported = Content { sections: [None, None], apps: AppsBody::Reason };
        assert_eq!(
            layout(unsupported, [0.0; 2], apps_target(unsupported.apps, 40.0), None).height,
            APPS_TITLE_H + APPS_GAP + 40.0
        );
    }
}
