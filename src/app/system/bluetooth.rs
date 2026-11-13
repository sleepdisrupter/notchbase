//! Passes Bluetooth changes on to the tools and the notch.

use crate::app::App;
use crate::app::preview::previewing;

impl App {
    pub(in crate::app) fn sync_bluetooth(&mut self) {
        let changes = self.bluetooth.take_changes();
        if previewing() {
            return;
        }
        for change in changes {
            self.feed_bluetooth(&change);
            self.update_notch(|notch| notch.show_device(&change));
        }
    }
}
