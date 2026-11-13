//! Access to the guarded folders. When it arrives they are scanned, and when it goes what they held is buried.

use std::path::PathBuf;

use crate::find::os::root;
use crate::find::store::meta;

use super::Writer;
use super::launch::media;
use super::system::Moment;

impl Writer {
    pub(super) fn probe(&mut self, now: Moment) {
        self.timers.probed = now.at;
        if !self.launched {
            return;
        }
        let full = self.system.full_access();
        if full == self.full {
            return;
        }
        self.full = full;
        self.replan();
        if full {
            self.open_guarded(false, now);
        } else {
            self.meta.flags &= !meta::GUARDED_DONE;
            self.bury_guarded();
        }
        self.watch();
    }

    pub(super) fn bury_guarded(&mut self) {
        let mut folders = self.home.as_deref().map(|home| self.system.guarded(home)).unwrap_or_default();
        let removable = self.roots.iter().filter(|found| found.flags & root::REMOVABLE != 0);
        folders.extend(removable.map(|found| found.path.clone()));
        folders.extend(media(&self.shared.reading()));
        let mut live = self.shared.writing();
        let (mut inside, mut doomed) = (Vec::new(), Vec::new());
        for folder in folders.iter().map(PathBuf::as_path) {
            if let Some(row) = live.resolve(folder) {
                live.children_into(row, &mut inside);
                doomed.append(&mut inside);
            }
        }
        doomed.into_iter().for_each(|row| live.bury(row));
        live.settle();
    }
}
