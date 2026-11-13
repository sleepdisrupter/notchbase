//! The rows that lead to the other views.

use egui::Id;

use super::super::assistant;
use super::super::catalog::Kind;
use super::super::dock;
use super::super::rows::{Row, card, nav_row};
use super::{Outcome, news};
use crate::bridge::{Action, AppView, PanelData};
use crate::pages::PageCtx;
use crate::ui::glyph::Glyph;

pub(super) const ROWS: usize = 4 + PERMISSION_ROWS + DOCK_ROWS;
/// notchbase dock is macOS only.
pub(super) const DOCK_ROWS: usize = if crate::system::dock::SUPPORTED { 1 } else { 0 };
pub(super) const PERMISSION_ROWS: usize = if crate::system::permissions::SUPPORTED { 1 } else { 0 };
const INDICATORS_DETAIL: &str = "Battery, camera, downloads and other notices";
const PERMISSIONS_DETAIL: &str = "Choose what the notch can use";
const NOTHING_NEW: &str = "Nothing new";

pub(super) fn ui(
    ctx: &mut PageCtx<'_>,
    top: f32,
    data: &PanelData,
    version: Option<&str>,
    live: bool,
    actions: &mut Vec<Action>,
) -> Outcome {
    let mut rects = card(ctx, top, ROWS + news::rows(version));
    if let (Some(rect), Some(version)) = (rects.get(ROWS), version) {
        news::row(ctx, *rect, version, live, actions);
    }
    rects.truncate(ROWS);
    let permissions = rects.split_off(rects.len() - PERMISSION_ROWS).pop();
    let docking = rects.split_off(rects.len() - DOCK_ROWS).pop();
    let [notifications, assistant, tools, indicators] = rects[..] else { return Outcome::Stay };

    let detail = notifications_summary(&data.app);
    let row = Row { rect: notifications, title: "Notifications", detail: &detail };
    let id = Id::new("notchbase.settings.notifications");
    let opened_notifications = nav_row(ctx, &row, id, Glyph::ChevronRight, live);

    let detail = assistant::summary(data.assistant, data.questions_in_notch, data.progress_in_notch);
    let row = Row { rect: assistant, title: "Assistant", detail: &detail };
    let opened_assistant = nav_row(ctx, &row, Id::new("notchbase.settings.assistant"), Glyph::ChevronRight, live);

    let count = Kind::Tools.count(data.catalog);
    let detail = format!("{count} installed \u{b7} their settings and folders");
    let row = Row { rect: tools, title: Kind::Tools.title(), detail: &detail };
    let opened_tools = nav_row(ctx, &row, Id::new("notchbase.settings.tools"), Glyph::ChevronRight, live);

    let row = Row { rect: indicators, title: Kind::Indicators.title(), detail: INDICATORS_DETAIL };
    let id = Id::new("notchbase.settings.indicators");
    let opened_indicators = nav_row(ctx, &row, id, Glyph::ChevronRight, live);

    let opened_dock = docking.is_some_and(|rect| {
        let row = Row { rect, title: "Dock", detail: dock::summary(data.dock) };
        nav_row(ctx, &row, Id::new("notchbase.settings.dock"), Glyph::ChevronRight, live)
    });

    if let Some(rect) = permissions {
        let row = Row { rect, title: "Permissions", detail: PERMISSIONS_DETAIL };
        if nav_row(ctx, &row, Id::new("notchbase.settings.permissions"), Glyph::ChevronRight, live) {
            actions.push(Action::ShowPermissions);
        }
    }

    if opened_dock {
        return Outcome::ShowDock;
    }
    match (opened_notifications, opened_assistant, opened_tools, opened_indicators) {
        (true, ..) => Outcome::ShowNotifications,
        (_, true, ..) => Outcome::ShowAssistant,
        (_, _, true, _) => Outcome::ShowTools,
        (.., true) => Outcome::ShowIndicators,
        _ => Outcome::Stay,
    }
}

fn notifications_summary(app: &AppView) -> String {
    match (app.notifications.as_slice(), app.unread) {
        ([], _) => NOTHING_NEW.to_owned(),
        ([only], _) => only.title.clone(),
        (all, 0) => format!("{} notifications", all.len()),
        (_, unread) => format!("{unread} unread"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{NotificationKind, NotificationView};

    #[test]
    fn notifications_row_sums_up() {
        let note = |title: &str| NotificationView {
            key: title.into(),
            kind: NotificationKind::Update,
            title: title.into(),
            detail: String::new(),
            progress: None,
            action: None,
        };
        let mut app = AppView::default();
        assert_eq!(notifications_summary(&app), NOTHING_NEW);
        app.notifications = vec![note("Update available")];
        app.unread = 1;
        assert_eq!(notifications_summary(&app), "Update available");
        app.notifications.push(note("Tool notice"));
        app.unread = 2;
        assert_eq!(notifications_summary(&app), "2 unread");
        app.unread = 0;
        assert_eq!(notifications_summary(&app), "2 notifications");
    }
}
