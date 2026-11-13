//! The Shelf on the app's side: running file operations on it, AirDrop, and posting their notices.

pub(super) mod airdrop;
pub(super) mod drops;

use std::path::PathBuf;
use std::time::Instant;

use crate::bridge::Page;
use crate::filedrag;
use crate::shelf::{Item, ItemId, Notice, Op};

use super::App;

impl App {
    pub(super) fn sync_shelf(&mut self, now: Instant) {
        self.panel.shelf.sync();
        self.show_shelf_notice(now);
    }

    pub(super) fn run_shelf_op(&mut self, op: Op, ids: &[ItemId]) {
        self.panel.shelf.run(op, ids);
        self.show_shelf_notice(Instant::now());
    }

    pub(super) fn reveal_shelf_item(&self, id: ItemId) {
        if let Some(item) = self.shelf_item(id) {
            filedrag::reveal(&item.path);
        }
    }

    pub(super) fn shelf_paths(&self, ids: &[ItemId]) -> Vec<PathBuf> {
        ids.iter().filter_map(|&id| self.shelf_item(id)).map(|item| item.path.clone()).collect()
    }

    fn shelf_item(&self, id: ItemId) -> Option<&Item> {
        self.panel.shelf.items().iter().find(|item| item.id == id)
    }

    fn show_shelf_notice(&mut self, now: Instant) {
        if let Some(notice) = self.panel.shelf.take_notice() {
            self.post_shelf_notice(notice, now);
        }
    }

    /// Shows `notice` under the Shelf, and in the notch too when the Shelf isn't on screen.
    pub(super) fn post_shelf_notice(&mut self, notice: Notice, now: Instant) {
        if self.visible_page() != Some(Page::Shelf) {
            self.notify(&notice.text);
        }
        self.panel.show_notice(notice, now);
        self.redraw_notch();
    }
}
