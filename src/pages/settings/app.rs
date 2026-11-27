//! The rows about notchbase itself: installing it, starting at sign-in, updates and What's New.

use egui::Id;

use super::news;
use super::rows::{Row, Toggle, nav_row, toggled};
use super::search::Said;
use crate::bridge::{Action, AppView};
use crate::installer::INSTALL_HINT;
use crate::pages::PageCtx;
use crate::ui::glyph::Glyph;

const INSTALL: &str = "Install notchbase";
const LOGIN: &str = "Open at login";
const LOGIN_DETAIL: &str = "Start notchbase when you sign in";
const UPDATES: &str = "Automatic updates";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AppRow {
    Install,
    Login,
    Updates,
    WhatsNew,
}

/// The rows the notchbase view shows now: Install until it's installed, What's New while this version has notes.
pub(super) fn rows(app: &AppView) -> Vec<AppRow> {
    let install = offers_install(app).then_some(AppRow::Install);
    let news = news::version().map(|_| AppRow::WhatsNew);
    install.into_iter().chain([AppRow::Login, AppRow::Updates]).chain(news).collect()
}

/// The tile's line.
pub(super) fn summary(app: &AppView) -> String {
    if offers_install(app) {
        return INSTALL.to_owned();
    }
    let version = env!("CARGO_PKG_VERSION");
    if app.updates.detail.is_empty() { version.to_owned() } else { format!("{version} \u{b7} {}", app.updates.detail) }
}

impl AppRow {
    pub(super) fn said(self, app: &AppView) -> Said {
        let (title, detail, words) = match self {
            AppRow::Install => (INSTALL.to_owned(), INSTALL_HINT.to_owned(), "applications setup"),
            AppRow::Login => (LOGIN.to_owned(), LOGIN_DETAIL.to_owned(), "startup launch boot sign in"),
            AppRow::Updates => (UPDATES.to_owned(), app.updates.detail.clone(), "version upgrade update"),
            AppRow::WhatsNew => {
                let version = news::version().unwrap_or_default();
                (news::TITLE.to_owned(), news::detail(version), "changes release notes version")
            }
        };
        Said { title, detail, words }
    }

    pub(super) fn paint(self, ctx: &mut PageCtx<'_>, rect: egui::Rect, app: &AppView, live: bool) -> Option<Action> {
        match self {
            AppRow::Install => {
                let row = Row { rect, title: INSTALL, detail: INSTALL_HINT };
                nav_row(ctx, &row, Id::new("notchbase.settings.install"), Glyph::ChevronRight, live)
                    .then_some(Action::InstallApp)
            }
            AppRow::Login => {
                let row = Row { rect, title: LOGIN, detail: LOGIN_DETAIL };
                let toggle = Toggle { id: Id::new("notchbase.settings.login"), on: app.open_at_login, enabled: live };
                toggled(ctx, &row, &toggle).then_some(Action::SetOpenAtLogin(!app.open_at_login))
            }
            AppRow::Updates => {
                let row = Row { rect, title: UPDATES, detail: &app.updates.detail };
                let enabled = live && app.updates.available;
                let toggle = Toggle { id: Id::new("notchbase.settings.updates"), on: app.updates.on, enabled };
                toggled(ctx, &row, &toggle).then_some(Action::SetAutoUpdate(!app.updates.on))
            }
            AppRow::WhatsNew => news::row(ctx, rect, news::version()?, live),
        }
    }
}

fn offers_install(app: &AppView) -> bool {
    app.can_install && !app.installed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_row_until_installed() {
        let news = usize::from(news::version().is_some());
        let mut app = AppView { can_install: true, ..AppView::default() };
        assert_eq!(rows(&app).len(), 3 + news);
        assert_eq!(rows(&app)[0], AppRow::Install);
        assert_eq!(summary(&app), INSTALL);
        app.installed = true;
        assert_eq!(rows(&app).len(), 2 + news);
        assert_eq!(rows(&AppView::default()).len(), 2 + news, "nothing to install on this platform");
    }

    #[test]
    fn tile_shows_version() {
        let mut app = AppView::default();
        assert_eq!(summary(&app), env!("CARGO_PKG_VERSION"));
        app.updates.detail = "Up to date".into();
        assert!(summary(&app).ends_with("\u{b7} Up to date"));
    }
}
