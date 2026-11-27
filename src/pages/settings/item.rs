//! One row of settings, wherever it shows: in its own view or among the search results.

use egui::{Id, Rect};

use super::app::AppRow;
use super::assistant::AssistantRow;
use super::catalog::{self, ToolsRow};
use super::dock::DockRow;
use super::editing::Editing;
use super::find::FindRow;
use super::notch::NotchRow;
use super::rows::{Row, nav_row};
use super::search::Said;
use super::setting::{self, has_settings, shown, tool_setting};
use crate::bridge::{Action, AppView, PanelData};
use crate::pages::PageCtx;
use crate::ui::glyph::Glyph;

const NOTHING_NEW: &str = "Nothing new";
const NOTIFICATIONS: &str = "Notifications";
const PERMISSIONS: &str = "Permissions";
const PERMISSIONS_DETAIL: &str = "Choose what the notch can use";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Item {
    Notch(NotchRow),
    Find(FindRow),
    Assistant(AssistantRow),
    Dock(DockRow),
    Tools(ToolsRow),
    /// An installed tool or indicator, by its id.
    Installed(String),
    /// One of a tool's own settings: the tool's id and the setting's key.
    Setting(String, String),
    App(AppRow),
    Notifications,
    Permissions,
}

/// Where a row asks to go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Went {
    Tool(String),
    Notifications,
}

impl Item {
    /// What the row says, or None when what it is about has gone (a tool that was removed, a hidden setting).
    pub(super) fn said(&self, data: &PanelData) -> Option<Said> {
        let said = match self {
            Item::Notch(row) => row.said(),
            Item::Find(row) => row.said(data.find),
            Item::Assistant(row) => row.said(data),
            Item::Dock(row) => row.said(data.dock),
            Item::Tools(row) => row.said(data.locations),
            Item::Installed(id) => catalog::said(data.catalog.iter().find(|info| info.id == *id)?),
            Item::Setting(id, key) => {
                let tool = data.tools.iter().find(|tool| tool.id == *id)?;
                setting::said(shown(tool).find(|setting| setting.spec.key == *key)?)
            }
            Item::App(row) => row.said(&data.app),
            Item::Notifications => Said {
                title: NOTIFICATIONS.to_owned(),
                detail: notifications_summary(&data.app),
                words: "alerts updates unread",
            },
            Item::Permissions => Said {
                title: PERMISSIONS.to_owned(),
                detail: PERMISSIONS_DETAIL.to_owned(),
                words: "accessibility screen recording privacy allow",
            },
        };
        Some(said)
    }

    pub(super) fn paint(
        &self,
        ctx: &mut PageCtx<'_>,
        rect: Rect,
        data: &PanelData,
        (live, editing): (bool, &mut Editing),
        actions: &mut Vec<Action>,
    ) -> Option<Went> {
        let action = match self {
            Item::Notch(row) => row.paint(ctx, rect, data, live),
            Item::Find(row) => row.paint(ctx, rect, data.find, live),
            Item::Assistant(row) => row.paint(ctx, rect, data, live),
            Item::Dock(row) => row.paint(ctx, rect, data.dock, live),
            Item::Tools(row) => row.paint(ctx, rect, data.locations, live),
            Item::Installed(id) => {
                let info = data.catalog.iter().find(|info| info.id == *id)?;
                let cog = data.tools.iter().any(|tool| tool.id == *id && has_settings(tool));
                let opened = catalog::installed(ctx, info, rect, (cog, live), actions);
                return opened.then(|| Went::Tool(id.clone()));
            }
            Item::Setting(id, key) => {
                let tool = data.tools.iter().find(|tool| tool.id == *id)?;
                let setting = shown(tool).find(|setting| setting.spec.key == *key)?;
                tool_setting(ctx, tool, setting, rect, live, editing, actions)
            }
            Item::App(row) => row.paint(ctx, rect, &data.app, live),
            Item::Notifications => {
                let detail = notifications_summary(&data.app);
                let row = Row { rect, title: NOTIFICATIONS, detail: &detail };
                let id = Id::new("notchbase.settings.notifications");
                return nav_row(ctx, &row, id, Glyph::ChevronRight, live).then_some(Went::Notifications);
            }
            Item::Permissions => {
                let row = Row { rect, title: PERMISSIONS, detail: PERMISSIONS_DETAIL };
                let id = Id::new("notchbase.settings.permissions");
                nav_row(ctx, &row, id, Glyph::ChevronRight, live).then_some(Action::ShowPermissions)
            }
        };
        actions.extend(action);
        None
    }
}

pub(super) fn notifications_summary(app: &AppView) -> String {
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
