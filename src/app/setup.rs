//! The install and open-at-login rows in Settings.

use winit::event_loop::ActiveEventLoop;

use crate::bridge::AppView;
use crate::installer;

use super::App;

const NOT_INSTALLED: &str = "Couldn't install notchbase";
const LOGIN_NOT_CHANGED: &str = "Couldn't change opening at login";

pub(super) struct Setup {
    installed: bool,
    can_install: bool,
    open_at_login: bool,
}

impl Setup {
    pub(super) fn read() -> Self {
        Self {
            installed: installer::is_installed(),
            can_install: installer::can_install(),
            open_at_login: installer::opens_at_login(),
        }
    }
}

impl App {
    pub(super) fn app_view(&self) -> AppView {
        let (notifications, unread) = self.notifications();
        AppView {
            installed: self.setup.installed,
            can_install: self.setup.can_install,
            open_at_login: self.setup.open_at_login,
            updates: self.updates.view(self.config.config()),
            notifications,
            unread,
            news: self.news.unseen(),
        }
    }

    pub(super) fn set_open_at_login(&mut self, on: bool) {
        match installer::set_open_at_login(on) {
            Ok(()) => self.setup.open_at_login = on,
            Err(err) => self.report(LOGIN_NOT_CHANGED, err),
        }
        self.redraw_notch();
    }

    pub(super) fn install_app(&mut self, event_loop: &ActiveEventLoop) {
        match installer::install(installer::Choices::default()) {
            Ok(()) => event_loop.exit(),
            Err(err) => self.report(NOT_INSTALLED, err),
        }
    }
}
