//! The system's search key (⌘Space on a Mac, the Windows key on Windows) opens Find while the switch is on.

use crate::app::App;
use crate::app::preview::previewing;
use crate::bridge::SystemKey;

const NOT_SAVED: &str = "Couldn't save the Find setting";

impl App {
    pub(in crate::app) fn follow_system_key(&mut self) {
        self.summon.set_enabled(self.config.config().find.system_key && !previewing());
        self.show_find();
    }

    pub(in crate::app) fn set_find_system_key(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.find.system_key = on) {
            self.report(NOT_SAVED, err);
        }
        self.follow_system_key();
    }

    pub(in crate::app) fn system_key(&self) -> SystemKey {
        system_key(self.config.config().find.system_key, self.summon.blocked())
    }

    pub(in crate::app) fn sync_system_key(&mut self) {
        if self.panel.find.system_key != self.system_key() {
            self.show_find();
        }
    }

    pub(in crate::app) fn summon_find(&mut self) {
        if self.keys.is_some() || self.capture_requested() {
            return;
        }
        if !self.typing {
            self.notches.follow_cursor();
        }
        let Some(nw) = self.notches.active() else { return };
        if nw.notch.find_settling() {
            return;
        }
        if nw.notch.finding() {
            return self.find_close();
        }
        let pointed = self.notches.under_cursor().filter(|_| self.typing);
        let meant = match pointed {
            Some(index) => self.notches.nth_mut(index),
            None => self.notches.active_mut(),
        };
        if let Some(nw) = meant {
            nw.notch.summon_find();
            nw.redraw();
        }
    }
}

fn system_key(wanted: bool, blocked: bool) -> SystemKey {
    match (wanted, blocked) {
        (false, _) => SystemKey::Off,
        (true, false) => SystemKey::On,
        (true, true) => SystemKey::Blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_shows_refusal() {
        assert_eq!(system_key(false, false), SystemKey::Off);
        assert_eq!(system_key(false, true), SystemKey::Off);
        assert_eq!(system_key(true, false), SystemKey::On);
        assert_eq!(system_key(true, true), SystemKey::Blocked);
    }
}
