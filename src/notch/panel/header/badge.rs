//! The small dots on the gear and the tabs, for unread notifications or a run that waits.

use egui::{Color32, Pos2, vec2};

use crate::bridge::{Page, PanelData};
use crate::ui::canvas::Canvas;
use crate::ui::color;

const RADIUS: f32 = 3.5;
const ACROSS: f32 = 0.42;
const UP: f32 = 0.38;
const CLEAR_OF_ICON: f32 = 1.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::notch) struct Badges {
    pub(in crate::notch) unread: bool,
    pub(in crate::notch) waiting: bool,
}

impl Badges {
    pub(in crate::notch) fn of(data: &PanelData) -> Self {
        Self { unread: data.app.unread > 0, waiting: data.agents.running.iter().any(|row| row.run.waiting) }
    }

    pub(super) fn gear(self) -> Option<Color32> {
        self.unread.then_some(color::ACCENT)
    }

    pub(super) fn tab(self, page: Page) -> Option<Color32> {
        (page == Page::Agents && self.waiting).then_some(color::YELLOW)
    }
}

pub(super) fn at(icon: Pos2, size: f32) -> Pos2 {
    icon + vec2(size * ACROSS, -size * UP)
}

pub(super) fn beside(icon: Pos2, size: f32) -> Pos2 {
    at(icon, size) + vec2(CLEAR_OF_ICON, -CLEAR_OF_ICON)
}

pub(super) fn paint(canvas: &mut Canvas, dot: Pos2, tint: Option<Color32>) {
    if let Some(tint) = tint {
        canvas.circle(dot, RADIUS, tint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_shows_unread_in_blue() {
        assert_eq!(Badges { unread: true, ..Badges::default() }.gear(), Some(color::ACCENT));
        assert_eq!(Badges { waiting: true, ..Badges::default() }.gear(), None);
    }

    #[test]
    fn agents_tab_shows_waiting_yellow() {
        let waiting = Badges { waiting: true, ..Badges::default() };
        assert_eq!(waiting.tab(Page::Agents), Some(color::YELLOW));
        assert_eq!(waiting.tab(Page::Stats), None);
        assert_eq!(Badges { unread: true, ..Badges::default() }.tab(Page::Agents), None);
    }

    #[test]
    fn dot_rides_icon_top_right() {
        let dot = at(Pos2::new(100.0, 50.0), 14.0);
        assert!(dot.x > 100.0 && dot.y < 50.0);
    }

    #[test]
    fn tab_dot_clears_sparkle() {
        let icon = Pos2::new(100.0, 50.0);
        for size in [14.0, 14.0 * 1.18] {
            let dot = beside(icon, size);
            for (x, y, reach) in crate::ui::glyph::SPARKLES {
                let star = icon + vec2(x, y) * size;
                let tips = [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)];
                for tip in tips.map(|toward| star + toward * reach * size) {
                    assert!(dot.distance(tip) > RADIUS, "the dot clears a star tip at size {size}");
                }
            }
        }
    }
}
