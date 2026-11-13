//! Where everything sits in the window: the title strip, the picture's stage and the toolbar along the bottom.

use egui::{Rect, Vec2, pos2, vec2};

use super::tool::Tool;
use crate::annotate::Brush;

pub(super) const SWATCHES: usize = Brush::PALETTE.len();
pub(super) const TOOLS: usize = Tool::ALL.len();
const TITLE: f32 = 44.0;
const MARGIN: f32 = 28.0;
const HEADROOM: f32 = 8.0;
const TOOLBAR: f32 = 52.0;
const FLOOR: f32 = 28.0;
const INSET: f32 = 10.0;
const CONTROL: f32 = TOOLBAR - 2.0 * INSET;
const DIVIDER_REACH: f32 = CONTROL * 0.3;
const SLOT: f32 = 28.0;
const GROUP_GAP: f32 = 22.0;
const WELL_GAP: f32 = 8.0;
const TRACK: f32 = 96.0;
const BUTTON_GAP: f32 = 6.0;
const DONE_GAP: f32 = 14.0;
const DONE: f32 = 64.0;
const TOOLBAR_WIDTH: f32 = INSET
    + TOOLS as f32 * CONTROL
    + GROUP_GAP
    + SWATCHES as f32 * SLOT
    + GROUP_GAP
    + CONTROL
    + WELL_GAP
    + TRACK
    + GROUP_GAP
    + CONTROL
    + BUTTON_GAP
    + CONTROL
    + DONE_GAP
    + DONE
    + INSET;

pub const CHROME: Vec2 = vec2(2.0 * MARGIN, TITLE + HEADROOM + MARGIN + TOOLBAR + FLOOR);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) window: Rect,
    pub(super) title: Rect,
    pub(super) stage: Rect,
    pub(super) toolbar: Rect,
    pub(super) tools: [Rect; TOOLS],
    pub(super) swatches: [Rect; SWATCHES],
    pub(super) well: Rect,
    pub(super) slider: Rect,
    pub(super) undo: Rect,
    pub(super) redo: Rect,
    pub(super) done: Rect,
    pub(super) dividers: [f32; 3],
}

impl Layout {
    pub(super) fn of(window: Rect) -> Self {
        let title = Rect::from_min_max(window.min, pos2(window.right(), window.top() + TITLE));
        let toolbar = Rect::from_center_size(
            pos2(window.center().x, window.bottom() - FLOOR - TOOLBAR / 2.0),
            vec2(TOOLBAR_WIDTH, TOOLBAR),
        );
        let stage = Rect::from_min_max(
            pos2(window.left() + MARGIN, title.bottom() + HEADROOM),
            pos2(window.right() - MARGIN, toolbar.top() - MARGIN),
        );
        let mut row = Row { at: toolbar.left() + INSET, top: toolbar.top() + INSET };
        let tools = std::array::from_fn(|_| row.take(CONTROL, 0.0));
        let first = row.gap(GROUP_GAP);
        let swatches = std::array::from_fn(|_| row.take(SLOT, 0.0));
        let second = row.gap(GROUP_GAP);
        let well = row.take(CONTROL, WELL_GAP);
        let slider = row.take(TRACK, 0.0);
        let third = row.gap(GROUP_GAP);
        let undo = row.take(CONTROL, BUTTON_GAP);
        let redo = row.take(CONTROL, DONE_GAP);
        let done = row.take(DONE, 0.0);
        let dividers = [first, second, third];
        Self { window, title, stage, toolbar, tools, swatches, well, slider, undo, redo, done, dividers }
    }

    pub(super) fn title_line(&self, title_bar: f32) -> f32 {
        let strip = self.title.height();
        self.title.top() + if title_bar > 0.0 { title_bar.min(strip) } else { strip } / 2.0
    }

    pub(super) fn divider_span(&self) -> [f32; 2] {
        [self.toolbar.center().y - DIVIDER_REACH, self.toolbar.center().y + DIVIDER_REACH]
    }
}

struct Row {
    at: f32,
    top: f32,
}

impl Row {
    fn take(&mut self, width: f32, then: f32) -> Rect {
        let rect = Rect::from_min_size(pos2(self.at, self.top), vec2(width, CONTROL));
        self.at += width + then;
        rect
    }

    fn gap(&mut self, width: f32) -> f32 {
        self.at += width;
        self.at - width / 2.0
    }
}

#[cfg(test)]
mod tests {
    use egui::Pos2;

    use super::*;
    use crate::annotate::SMALLEST_WINDOW;

    fn layout(size: Vec2) -> Layout {
        Layout::of(Rect::from_min_size(Pos2::ZERO, size))
    }

    fn controls(layout: &Layout) -> Vec<Rect> {
        let single = [layout.well, layout.slider, layout.undo, layout.redo, layout.done];
        layout.tools.into_iter().chain(layout.swatches).chain(single).collect()
    }

    #[test]
    fn nothing_overlaps_in_smallest_window() {
        for size in [SMALLEST_WINDOW, vec2(900.0, 640.0), vec2(2400.0, 420.0), vec2(SMALLEST_WINDOW.x, 1400.0)] {
            let layout = layout(size);
            let controls = controls(&layout);
            assert!(layout.window.contains_rect(layout.toolbar), "{size:?}");
            assert!(layout.window.contains_rect(layout.stage) && layout.stage.is_positive(), "{size:?}");
            assert!(!layout.stage.intersects(layout.toolbar) && !layout.stage.intersects(layout.title), "{size:?}");
            assert!(!layout.title.intersects(layout.toolbar), "{size:?}");
            for (index, control) in controls.iter().enumerate() {
                assert!(layout.toolbar.contains_rect(*control), "{control:?} in {size:?}");
                for other in &controls[index + 1..] {
                    assert!(!control.intersect(*other).is_positive(), "{control:?} over {other:?}");
                }
            }
        }
    }

    #[test]
    fn toolbar_centred_groups_divided() {
        let layout = layout(vec2(900.0, 640.0));
        assert_eq!(layout.toolbar.center().x, 450.0);
        assert_eq!(layout.tools[0].left() - layout.toolbar.left(), INSET);
        assert_eq!(layout.toolbar.right() - layout.done.right(), INSET);
        assert_eq!((layout.toolbar.width(), TOOLBAR_WIDTH), (574.0, 574.0));
        let [first, second, third] = layout.dividers;
        assert!(first > layout.tools[TOOLS - 1].right() && first < layout.swatches[0].left());
        assert!(second > layout.swatches[SWATCHES - 1].right() && second < layout.well.left());
        assert!(third > layout.slider.right() && third < layout.undo.left());
    }

    #[test]
    fn tools_sit_left_of_swatches() {
        for size in [SMALLEST_WINDOW, vec2(900.0, 640.0)] {
            let layout = layout(size);
            let [brush, text] = layout.tools;
            assert_eq!((brush.size(), text.size()), (vec2(CONTROL, CONTROL), vec2(CONTROL, CONTROL)), "{size:?}");
            assert!(brush.right() == text.left() && text.right() < layout.swatches[0].left(), "{size:?}");
            assert!(layout.toolbar.shrink(INSET).contains_rect(brush.union(text)), "{size:?}");
        }
    }

    #[test]
    fn smallest_window_keeps_margins() {
        let layout = layout(SMALLEST_WINDOW);
        assert!(layout.toolbar.left() - layout.window.left() >= MARGIN);
        assert_eq!(SMALLEST_WINDOW.y - CHROME.y, layout.stage.height());
        assert_eq!(SMALLEST_WINDOW.x - CHROME.x, layout.stage.width());
    }

    #[test]
    fn title_lines_up_with_title_bar() {
        let layout = layout(SMALLEST_WINDOW);
        assert_eq!(layout.title_line(32.0), 16.0);
        assert_eq!(layout.title_line(28.0), 14.0);
        assert_eq!(layout.title_line(0.0), TITLE / 2.0, "with no title bar over the picture it is centred");
    }

    #[test]
    fn title_stays_inside_its_strip() {
        let layout = layout(SMALLEST_WINDOW);
        for title_bar in [0.0, 22.0, 28.0, 32.0, TITLE, 52.0, 400.0] {
            let line = layout.title_line(title_bar);
            assert!(layout.title.contains(pos2(layout.title.center().x, line)), "{title_bar}");
        }
    }
}
