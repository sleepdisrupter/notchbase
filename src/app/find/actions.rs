//! What the Find page asks for: queries, pins, the settings switch and Full Disk Access.

use std::path::{Path, PathBuf};

use crate::app::App;
use crate::bridge::{Action, NotificationKind, NotificationView, PINNED_AT_MOST, Pinned};
use crate::config::{Config, FindPin};
use crate::notch::Notch;
use crate::system::permissions::{self, Access};

const NOT_SAVED: &str = "Couldn't save the Find setting";
const ASK_FOR_ACCESS: &str = "Allow Full Disk Access so Find can see every file";
const NOT_ASKED: &str = "Couldn't remember asking for Full Disk Access";
const PERMISSION_KEY: &str = "find-full-disk-access";
const PERMISSION_TITLE: &str = "See every file";
const PERMISSION_DETAIL: &str =
    "Find skips Desktop, Documents and Downloads until notchbase has Full Disk Access. Nothing is asked again.";
const OPEN_SETTINGS: &str = "Open Settings";

impl App {
    pub(in crate::app) fn sync_find(&mut self) {
        if self.find.synced() {
            self.ask_for_find_access();
            self.show_find();
        }
    }

    pub(in crate::app) fn show_find(&mut self) {
        let mut view = self.find.view(self.config.config());
        view.system_key = self.system_key();
        if view != self.panel.find {
            self.panel.find = view;
            self.redraw_notch();
        }
    }

    pub(in crate::app) fn find_query(&mut self, text: &str) {
        if let Some(service) = &self.find.service {
            service.ask(text);
        }
    }

    pub(super) fn find_left(&mut self) {
        self.find.left();
        self.show_find();
    }

    pub(in crate::app) fn find_pin(&mut self, pinned: &Pinned) {
        let pin = FindPin {
            path: pinned.path.to_string_lossy().into_owned(),
            name: pinned.name.clone(),
            app: pinned.app,
            folder: pinned.folder,
        };
        self.change_pins(|pins| with_pin(pins, pin));
    }

    pub(in crate::app) fn find_unpin(&mut self, path: &Path) {
        let path = path.to_string_lossy().into_owned();
        self.change_pins(|pins| pins.retain(|pin| pin.path != path));
    }

    pub(in crate::app) fn find_order_pins(&mut self, order: &[PathBuf]) {
        self.change_pins(|pins| in_order(pins, order));
    }

    fn change_pins(&mut self, change: impl FnOnce(&mut Vec<FindPin>)) {
        let before = self.config.config().find.pins.clone();
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            return;
        }
        if let Err(err) = self.config.update(|config| config.find.pins = after) {
            self.report(NOT_SAVED, err);
        }
        self.show_find();
    }

    pub(in crate::app) fn find_close(&mut self) {
        if self.typing {
            self.release_typing();
        } else {
            self.restore_front_app();
        }
        self.update_notch(Notch::collapse);
    }

    pub(in crate::app) fn set_find_enabled(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.find.enabled = on) {
            self.report(NOT_SAVED, err);
        }
        if let Some(service) = &self.find.service {
            service.follow(&self.config.config().find);
        }
        self.show_find();
    }

    pub(in crate::app) fn follow_find(&mut self, previous: &Config) {
        let current = &self.config.config().find;
        if previous.find != *current
            && let Some(service) = &self.find.service
        {
            service.follow(current);
        }
        self.follow_system_key();
    }

    pub(in crate::app) fn find_forget(&mut self) {
        if let Some(service) = &self.find.service {
            service.forget();
        }
    }

    pub(in crate::app) fn find_hide_access(&mut self) {
        if let Err(err) = self.config.update(|config| config.permissions.find_access_hidden = true) {
            self.report(NOT_SAVED, err);
        }
        self.show_find();
    }

    pub(in crate::app) fn find_notification(&self) -> Option<NotificationView> {
        (self.find.asked_this_run && self.find.limited()).then(|| NotificationView {
            key: PERMISSION_KEY.to_owned(),
            kind: NotificationKind::Permission,
            title: PERMISSION_TITLE.to_owned(),
            detail: PERMISSION_DETAIL.to_owned(),
            progress: None,
            action: Some((OPEN_SETTINGS.to_owned(), Action::AllowFullDiskAccess)),
        })
    }

    fn ask_for_find_access(&mut self) {
        let asked = self.config.config().permissions.find_disk_asked || !permissions::may_ask(Access::FullDisk);
        if !self.find.limited() || asked {
            return;
        }
        if let Err(err) = self.config.update(|config| config.permissions.find_disk_asked = true) {
            self.report(NOT_ASKED, err);
        }
        self.find.asked_this_run = true;
        self.notify(ASK_FOR_ACCESS);
    }
}

fn with_pin(pins: &mut Vec<FindPin>, pin: FindPin) {
    if pins.len() < PINNED_AT_MOST && pins.iter().all(|held| held.path != pin.path) {
        pins.push(pin);
    }
}

fn in_order(pins: &mut [FindPin], order: &[PathBuf]) {
    let place = |pin: &FindPin| order.iter().position(|path| path.as_os_str() == pin.path.as_str());
    pins.sort_by_key(|pin| place(pin).unwrap_or(usize::MAX));
}

#[cfg(test)]
mod pin_tests {
    use super::*;

    fn pin(path: &str) -> FindPin {
        FindPin { path: path.into(), name: path.rsplit('/').next().unwrap_or(path).into(), app: false, folder: false }
    }

    #[test]
    fn pins_added_once_up_to_four() {
        let mut pins = Vec::new();
        for name in ["/a", "/b", "/a"] {
            with_pin(&mut pins, pin(name));
        }
        assert_eq!(pins, [pin("/a"), pin("/b")], "the same thing is not pinned twice");
        for name in ["/c", "/d", "/e", "/f"] {
            with_pin(&mut pins, pin(name));
        }
        assert_eq!(pins.len(), PINNED_AT_MOST);
        assert_eq!(pins.last(), Some(&pin("/d")), "a fifth is not taken, and nothing is pushed out for it");
    }

    #[test]
    fn pins_follow_dragged_order() {
        let mut pins = vec![pin("/a"), pin("/b"), pin("/c")];
        in_order(&mut pins, &[PathBuf::from("/c"), PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(pins, [pin("/c"), pin("/a"), pin("/b")]);
        in_order(&mut pins, &[PathBuf::from("/b")]);
        assert_eq!(pins, [pin("/b"), pin("/c"), pin("/a")], "the rest keep their order behind it");
        in_order(&mut pins, &[]);
        assert_eq!(pins.len(), 3);
    }
}
