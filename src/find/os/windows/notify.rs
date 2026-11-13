//! Watches each fixed drive with ReadDirectoryChangesExW.

mod port;

use std::sync::mpsc::Sender;

use crate::find::os::Root;
use crate::find::shared::Order;
use crate::find::watch::Watcher;

pub const KEEPS_CURSOR: bool = false;

pub fn cursor_now() -> u64 {
    0
}

pub fn watch(roots: &[Root], _since: Option<u64>, sink: Sender<Order>) -> Option<Watcher> {
    if roots.is_empty() {
        return None;
    }
    port::watch(roots, sink)
}
