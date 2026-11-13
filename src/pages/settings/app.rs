//! The rows about notchbase itself: starting at sign-in, installing it, and Quit.

use egui::Id;

use super::rows::{ROW_H, Row, Toggle, card, nav_row, text_button, toggled};
use crate::bridge::{Action, AppView};
use crate::installer::INSTALL_HINT;
use crate::pages::PageCtx;
use crate::ui::glyph::Glyph;

const ALWAYS: usize = 2;
pub(super) const QUIT_H: f32 = 32.0;
const LOGIN_DETAIL: &str = "Start notchbase when you sign in";
const QUIT: &str = "Quit notchbase";

pub(super) fn rows(app: &AppView) -> usize {
    ALWAYS + usize::from(offers_install(app))
}

pub(super) fn height(app: &AppView) -> f32 {
    rows(app) as f32 * ROW_H
}

pub(super) fn ui(ctx: &mut PageCtx<'_>, top: f32, app: &AppView, live: bool, actions: &mut Vec<Action>) {
    let mut rects = card(ctx, top, rows(app)).into_iter();
    if offers_install(app)
        && let Some(rect) = rects.next()
    {
        let row = Row { rect, title: "Install notchbase", detail: INSTALL_HINT };
        if nav_row(ctx, &row, Id::new("notchbase.settings.install"), Glyph::ChevronRight, live) {
            actions.push(Action::InstallApp);
        }
    }
    let (Some(login), Some(updates)) = (rects.next(), rects.next()) else { return };

    let row = Row { rect: login, title: "Open at login", detail: LOGIN_DETAIL };
    let toggle = Toggle { id: Id::new("notchbase.settings.login"), on: app.open_at_login, enabled: live };
    if toggled(ctx, &row, &toggle) {
        actions.push(Action::SetOpenAtLogin(!app.open_at_login));
    }

    let row = Row { rect: updates, title: "Automatic updates", detail: &app.updates.detail };
    let toggle = Toggle {
        id: Id::new("notchbase.settings.updates"),
        on: app.updates.on,
        enabled: live && app.updates.available,
    };
    if toggled(ctx, &row, &toggle) {
        actions.push(Action::SetAutoUpdate(!app.updates.on));
    }
}

pub(super) fn quit(ctx: &mut PageCtx<'_>, top: f32, live: bool, actions: &mut Vec<Action>) {
    if text_button(ctx, top, QUIT_H, Id::new("notchbase.settings.quit"), QUIT, live) {
        actions.push(Action::Quit);
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
        let mut app = AppView { can_install: true, ..AppView::default() };
        assert_eq!(rows(&app), 3);
        app.installed = true;
        assert_eq!(rows(&app), 2);
        assert_eq!(rows(&AppView::default()), 2, "nothing to install on this platform");
    }
}
