//! The gear and the Find button in the header's corners, and where each one leads.

use egui::{Id, Rect, Sense, Vec2};

use crate::bridge::{Action, Page};
use crate::notch::{Mode, Notch};
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, white};
use crate::ui::glass::Glass;
use crate::ui::glyph::{self, Glyph};

use super::{HOVER_TINT, PILL_INSET, RESTING, paint_pill};

impl Notch {
    pub(super) fn settings_on_click(&mut self, ui: &egui::Ui, gear: Rect) -> bool {
        let response = ui.interact(gear, Id::new("notchbase.settings"), Sense::click());
        if response.clicked() {
            self.toggle_settings();
        }
        response.hovered()
    }

    pub(super) fn toggle_settings(&mut self) {
        let inside = matches!(self.pager.page, Page::Settings | Page::Notifications);
        self.switch_to(if inside { self.way_back() } else { Page::Settings });
    }

    pub(super) fn find_on_click(&mut self, ui: &egui::Ui, find: Rect) -> bool {
        let response = ui.interact(find, Id::new("notchbase.find"), Sense::click());
        if response.clicked() {
            self.toggle_find();
        }
        response.hovered()
    }

    pub(in crate::notch) fn toggle_find(&mut self) {
        if self.pager.page == Page::Find {
            if !self.pages.find.settling() {
                self.switch_to(self.way_back());
            }
            return;
        }
        self.pages.find.arm();
        self.switch_to(Page::Find);
    }

    pub fn summon_find(&mut self) {
        if self.preview.is_some() || self.locked || self.finding() {
            return;
        }
        self.pages.find.arm();
        self.open_page(Page::Find);
    }

    pub fn finding(&self) -> bool {
        self.mode == Mode::Expanded && self.pager.page == Page::Find
    }

    pub fn find_open_for(&self) -> Option<f32> {
        self.finding().then(|| self.pages.find.entered_for()).flatten()
    }

    pub fn find_settling(&self) -> bool {
        self.finding() && (self.pages.find.armed() || self.pages.find.settling())
    }

    pub fn leave_find(&mut self) -> Vec<Action> {
        let committed = self.pages.let_go();
        if self.pager.page == Page::Find {
            self.switch_to(self.way_back());
        }
        committed
    }

    pub fn retire(&mut self) -> Vec<Action> {
        self.pages.hide()
    }

    pub(in crate::notch) fn never_reopen_into_a_passing_page(&mut self, hidden: bool) {
        let expanded = self.mode == Mode::Expanded;
        let reopened = self.pager.reopened(expanded) && !self.pages.find.armed();
        let closed = hidden && !expanded;
        let passed = match self.pager.page {
            Page::Find => reopened || closed,
            Page::WhatsNew | Page::Welcome => closed,
            _ => false,
        };
        if self.preview.is_none() && passed {
            self.pager.show(self.way_back());
        }
    }

    pub(in crate::notch) fn leave_changes_when_done(&mut self) {
        if self.pages.take_changes_done() {
            self.switch_to(self.way_back());
        }
    }

    pub(in crate::notch) fn way_back(&self) -> Page {
        let tabbed = |page: Page| self.tabs.iter().any(|tab| tab.page == page);
        if tabbed(self.pager.from) {
            return self.pager.from;
        }
        let at = self.pager.tab.target.round().max(0.0) as usize;
        self.tabs.get(at).or(self.tabs.first()).map_or(Page::Tools, |tab| tab.page)
    }
}

pub(super) fn paint_button(canvas: &mut Canvas, button: Rect, mark: Glyph, size: f32, shown: f32, hover: f32) {
    if shown > 0.001 {
        let lens = Rect::from_center_size(button.center(), Vec2::splat(button.height() - 2.0 * PILL_INSET));
        paint_pill(canvas, lens, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, 1.0), shown);
    }
    let tint = color::mix(white(RESTING), white(1.0), shown.max(HOVER_TINT * hover));
    glyph::paint(canvas, mark, button.center(), size, tint);
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::super::GEAR_GAP;
    use super::*;
    use crate::notch::tests::fixtures::settle;
    use crate::platform::Style;
    use crate::ui::metrics::PAD;

    fn both_styles() -> [Notch; 2] {
        [Style::Island, Style::Notch].map(|style| Notch::with_preview(style, Some(vec2(185.0, 32.0)), None))
    }

    #[test]
    fn find_mirrors_the_gear() {
        for n in both_styles() {
            let (header, panel) = (n.header(), n.body_rect(n.expanded_geometry()));
            let middle = panel.center().x;
            assert!((header.find.center().x - middle + header.gear.center().x - middle).abs() < 1e-3);
            assert_eq!(header.find.center().y, header.gear.center().y);
            assert_eq!(header.find.size(), header.gear.size());
            assert!((header.find.left() - panel.left() - PAD).abs() < 1e-3);
        }
    }

    #[test]
    fn find_sits_in_left_wing() {
        let n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
        let camera = Rect::from_min_size(pos2(340.0 - 92.5, 0.0), vec2(185.0, 32.0));
        let header = n.header();
        assert_eq!(header.find.center(), pos2(182.5, 16.0));
        assert!(header.find.right() < camera.left() && header.find.bottom() <= camera.bottom());
        let panel = n.body_rect(n.expanded_geometry());
        assert!(header.bar.left() >= panel.left() + PAD - 1e-3 && header.bar.right() <= panel.right() - PAD + 1e-3);
        assert!(header.bar.top() >= camera.bottom(), "the tab row keeps its place under the camera");
    }

    #[test]
    fn island_row_clears_find() {
        let n = Notch::with_preview(Style::Island, None, None);
        let (header, panel) = (n.header(), n.body_rect(n.expanded_geometry()));
        assert_eq!(header.find.center(), pos2(184.0, 33.0));
        assert!(header.bar.left() >= header.find.right() + GEAR_GAP - 1e-3);
        assert!(header.bar.right() <= header.gear.left() - GEAR_GAP + 1e-3);
        assert!((header.bar.center().x - panel.center().x).abs() < 1e-3);
    }

    #[test]
    fn find_toggles_back() {
        for mut n in both_styles() {
            n.open_page(Page::Stats);
            n.toggle_find();
            assert_eq!((n.pager.page, n.pager.from), (Page::Find, Page::Stats));
            n.toggle_find();
            assert_eq!(n.pager.page, Page::Stats);
        }
    }

    #[test]
    fn find_then_gear_twice_lands_on_tab() {
        let mut n = Notch::with_preview(Style::Island, None, None);
        n.open_page(Page::Shelf);
        n.toggle_find();
        n.toggle_settings();
        assert_eq!(n.pager.page, Page::Settings);
        n.toggle_settings();
        assert_eq!(n.pager.page, Page::Shelf, "the tab the lens rests under");
        n.toggle_settings();
        n.toggle_find();
        n.toggle_find();
        assert_eq!(n.pager.page, Page::Shelf, "and Find leaves Settings for a tab too");
    }

    #[test]
    fn only_button_arms_find() {
        let mut n = Notch::with_preview(Style::Island, None, None);
        n.open_page(Page::Find);
        assert_eq!(n.pager.page, Page::Find);
        assert!(!n.pages.find.armed(), "opening the page by any other road leaves the keyboard alone");
        n.open_page(Page::Tools);
        n.toggle_find();
        assert!(n.pages.find.armed());
        n.toggle_find();
        n.leave_find();
        assert_eq!(n.pager.page, Page::Tools);
        let mut n = Notch::with_preview(Style::Notch, None, None);
        n.pager.show(Page::Find);
        n.set_pinned(true);
        assert!(!n.pages.find.armed(), "nor does a pin or a restore");
    }

    #[test]
    fn no_tab_lit_during_find() {
        for mut n in both_styles() {
            n.open_page(Page::Stats);
            settle(&mut n);
            assert_eq!(n.tab_emphasis(), 1.0);
            n.toggle_find();
            settle(&mut n);
            assert_eq!((n.tab_emphasis(), n.shown(Page::Find), n.settings_shown()), (0.0, 1.0, 0.0));
            n.toggle_find();
            settle(&mut n);
            assert_eq!(n.tab_emphasis(), 1.0, "the lens comes back with the tab");
        }
    }

    #[test]
    fn leaving_find_returns_to_tab() {
        let mut n = Notch::with_preview(Style::Island, None, None);
        n.open_page(Page::Media);
        n.toggle_find();
        n.leave_find();
        assert_eq!(n.pager.page, Page::Media);
        n.switch_to(Page::Settings);
        n.leave_find();
        assert_eq!(n.pager.page, Page::Settings);
    }
}
