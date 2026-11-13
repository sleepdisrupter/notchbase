//! Watches the roots with an FSEvents stream that can replay from a saved event id.

mod stream;

use std::sync::mpsc::Sender;

use crate::find::os::Root;
use crate::find::shared::Order;
use crate::find::watch::Watcher;

use stream::{FSEventsGetCurrentEventId, Stream};

pub const KEEPS_CURSOR: bool = true;

pub fn cursor_now() -> u64 {
    unsafe { FSEventsGetCurrentEventId() }
}

pub fn watch(roots: &[Root], since: Option<u64>, sink: Sender<Order>) -> Option<Watcher> {
    if roots.is_empty() {
        return None;
    }
    let stream = Stream::start(roots, since, sink)?;
    Some(Watcher::new(move || drop(stream)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::find::tests::Scratch;
    use crate::find::watch::Change;

    const PATIENCE: Duration = Duration::from_secs(10);

    fn touched(orders: &mpsc::Receiver<Order>, wanted: &Path) -> bool {
        let started = Instant::now();
        while let Some(left) = PATIENCE.checked_sub(started.elapsed()) {
            let Ok(Order::Changes(burst)) = orders.recv_timeout(left) else { continue };
            let paths = burst.events.into_iter().filter_map(|event| match event.change {
                Change::Touched(path) | Change::Relist(path) | Change::Subtree(path) => Some(path),
                Change::Moved { to, .. } => Some(to),
                Change::Overflow(_) | Change::Everything(_) => None,
            });
            if paths.into_iter().any(|path| path == wanted) {
                return true;
            }
        }
        false
    }

    #[test]
    #[ignore = "starts a real FSEvents stream over a temporary folder"]
    fn written_file_arrives_as_hint() {
        let scratch = Scratch::new("fsevents");
        let folder: PathBuf = fs::canonicalize(scratch.dir()).expect("a real path");
        let root = Root { path: folder.clone(), volume: [0; 16], devices: [0; 2], flags: 0 };
        let before = cursor_now();
        let (sink, orders) = mpsc::channel();
        let watcher = watch(&[root], None, sink).expect("the stream starts");
        let note = folder.join("note.txt");
        fs::write(&note, "hello").expect("a file");
        assert!(touched(&orders, &note), "the write was reported");
        drop(watcher);
        assert!(cursor_now() > before);
    }
}
