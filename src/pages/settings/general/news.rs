//! The What's New row, there while this version has notes.

use egui::{Id, Rect};

use super::super::rows::{Row, nav_row};
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::ui::glyph::Glyph;

const TITLE: &str = "What's New";
const ROW_ID: &str = "notchbase.settings.whats-new";

pub(super) fn version() -> Option<&'static str> {
    crate::whats_new::latest().map(|release| release.version)
}

pub(super) fn rows(version: Option<&str>) -> usize {
    usize::from(version.is_some())
}

pub(super) fn row(ctx: &mut PageCtx<'_>, rect: Rect, version: &str, live: bool, actions: &mut Vec<Action>) {
    let detail = detail(version);
    let row = Row { rect, title: TITLE, detail: &detail };
    if nav_row(ctx, &row, Id::new(ROW_ID), Glyph::ChevronRight, live) {
        actions.push(Action::ViewChanges);
    }
}

fn detail(version: &str) -> String {
    format!("See what changed in {version}")
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::super::super::rows::ROW_H;
    use super::*;
    use crate::pages::harness::{Harness, PAGE, press, release};

    fn clicked(live: bool) -> Vec<Action> {
        let mut harness = Harness::new();
        let rect = Rect::from_min_size(PAGE.min, vec2(PAGE.width(), ROW_H));
        let mut frame = |events: Vec<egui::Event>| {
            harness.frame(events, |ctx| {
                let mut actions = Vec::new();
                row(ctx, rect, "0.14.0", live, &mut actions);
                actions
            })
        };
        frame(Vec::new());
        let mut actions = frame(press(rect.center()));
        actions.extend(frame(release(rect.center())));
        actions
    }

    #[test]
    fn row_only_with_notes() {
        assert_eq!(rows(None), 0);
        assert_eq!(rows(Some("0.14.0")), 1);
        assert_eq!(rows(version()), 1, "this build ships its notes");
        assert_eq!(detail("0.14.0"), "See what changed in 0.14.0");
    }

    #[test]
    fn pressing_row_views_changes() {
        assert_eq!(clicked(true), [Action::ViewChanges]);
        assert!(clicked(false).is_empty(), "a page that is fading takes no clicks");
    }
}
