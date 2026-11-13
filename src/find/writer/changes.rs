//! What the writer does with each change the watcher reports. A touched path is buried when it is gone, restamped when
//! it changed and relisted from its parent when it is new. Moves keep the old place findable until the new one is
//! listed, and an overflow rescans the root.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::find::model::{EntryId, flag};
use crate::find::os::{Raw, raw};
use crate::find::store::{mark, meta};
use crate::find::usage::Seen;
use crate::find::watch::{Burst, Change, Event};

use super::errands::{Duty, Purpose};
use super::system::Moment;
use super::{Deferred, Work, Writer};

pub(super) const RELIST_SLOW: usize = 20_000;
pub(super) const SLOW_RELIST_GAP: Duration = Duration::from_secs(5);
pub(super) const RESCAN_GAP: Duration = Duration::from_secs(10 * 60);
pub(super) const SHALLOW_REACH: u8 = 3;
pub(super) const RECONCILE_REACH: u8 = 1;
pub(super) const EVERYTHING: u8 = 3;

impl Writer {
    pub(super) fn changed(&mut self, burst: Burst, now: Moment) {
        if !self.config.enabled {
            return;
        }
        if self.full {
            self.probe(now);
        }
        for event in burst.events {
            self.ledger.saw(event.id);
            self.coalescer.push(event, &self.plan, now.at);
        }
    }

    pub(super) fn change(&mut self, event: Event, now: Moment) {
        let id = Some(event.id);
        match event.change {
            Change::Touched(path) => self.look(&path, id, now),
            Change::Relist(dir) => self.relist(dir, id, now),
            Change::Subtree(dir) => self.subtree(dir, id, now),
            Change::Moved { from, to } => self.moved(from, to, id, now),
            Change::Overflow(root) if self.is_root(&root) => self.rescan(root, RECONCILE_REACH, id, now),
            Change::Everything(root) if self.is_root(&root) => {
                self.mark_of(&root).iter_mut().for_each(|found| found.flags &= !mark::COMPLETE);
                self.rescan(root, EVERYTHING, id, now);
            }
            Change::Overflow(_) | Change::Everything(_) => self.rediscover(now),
        }
    }

    pub(super) fn look(&mut self, path: &Path, id: Option<u64>, now: Moment) {
        if !self.plan.wanted(path) {
            let adopted = self.shared.reading().resolve(path);
            if let Some(entry) = adopted.filter(|_| matches!(self.system.stat(path), Ok(None))) {
                self.bury(path, entry, now);
            }
            return;
        }
        let found = self.shared.reading().resolve(path);
        if let Some(entry) = found {
            match self.system.stat(path) {
                Ok(None) => return self.bury(path, entry, now),
                Ok(Some(raw)) if self.same_identity(entry, &raw) => return self.restat(path, entry, &raw, now),
                Ok(Some(_)) => {}
                Err(_) => return,
            }
        }
        if let Some(parent) = path.parent().and_then(|parent| self.known_at_or_above(parent)) {
            self.relist(parent, id, now);
        }
    }

    pub(super) fn relist(&mut self, dir: PathBuf, id: Option<u64>, now: Moment) {
        let Some(entry) = self.shared.reading().resolve(&dir) else { return self.look(&dir, id, now) };
        let crowded = {
            let mut children = Vec::new();
            self.shared.reading().children_into(entry, &mut children);
            children.len() > RELIST_SLOW
        };
        let recent = self.relisted.get(&dir).map(|at| *at + SLOW_RELIST_GAP).filter(|due| *due > now.at);
        let work = Work { dir: dir.clone(), deep: false, reach: SHALLOW_REACH };
        match recent.filter(|_| crowded) {
            Some(due) => self.defer(work, due, true, id),
            None => {
                self.relisted_now(dir, now);
                self.errand(id, Vec::new(), Purpose::Live, vec![work], now);
            }
        }
    }

    pub(super) fn relisted_now(&mut self, dir: PathBuf, now: Moment) {
        self.relisted.retain(|_, at| now.at.saturating_duration_since(*at) < SLOW_RELIST_GAP);
        self.relisted.insert(dir, now.at);
    }

    fn subtree(&mut self, dir: PathBuf, id: Option<u64>, now: Moment) {
        if self.shared.reading().resolve(&dir).is_none() {
            return self.look(&dir, id, now);
        }
        let work = Work { dir, deep: true, reach: EVERYTHING };
        self.errand(id, Vec::new(), Purpose::Live, vec![work], now);
    }

    fn moved(&mut self, from: PathBuf, to: PathBuf, id: Option<u64>, now: Moment) {
        let (left, arrived) = (self.system.stat(&from), self.system.stat(&to));
        let place = to.parent().and_then(|parent| self.known_at_or_above(parent));
        let (Ok(None), Ok(Some(_)), Some(place)) = (left, arrived, place) else {
            self.look(&from, id, now);
            return self.look(&to, id, now);
        };
        if self.config.learn {
            self.shared.usage.observe(&[Seen::Moved { from: from.clone(), to }], now.stamp);
        }
        self.keeping.insert(from.clone());
        let work = Work { dir: place, deep: false, reach: SHALLOW_REACH };
        self.errand(id, vec![Duty::Bury(from)], Purpose::Live, vec![work], now);
    }

    fn rescan(&mut self, root: PathBuf, reach: u8, id: Option<u64>, now: Moment) {
        let work = Work { dir: root.clone(), deep: true, reach };
        let waiting = self.deferred.iter_mut().find(|deferred| deferred.work.dir == root && deferred.work.deep);
        if let Some(waiting) = waiting {
            waiting.work.reach = waiting.work.reach.max(reach);
            let errand = waiting.errand;
            for duty in self.errands.duties(errand).into_iter().flatten() {
                if let Duty::Scanned { reach: planned, .. } = duty {
                    *planned = (*planned).max(reach);
                }
            }
            id.into_iter().for_each(|id| self.ledger.opened(errand, id));
            return;
        }
        let duties = vec![Duty::Scanned { root: root.clone(), reach }];
        match self.rescanned.get(&root).map(|at| *at + RESCAN_GAP).filter(|due| *due > now.at) {
            Some(due) => {
                let errand = self.errands.open(duties, Purpose::Reconcile);
                id.into_iter().for_each(|id| self.ledger.opened(errand, id));
                self.deferred.push(Deferred { errand, due, work, live: false });
            }
            None => {
                self.rescanned.insert(root, now.at);
                self.errand(id, duties, Purpose::Reconcile, vec![work], now);
            }
        }
    }

    pub(super) fn land(&mut self, errand: u32, duties: Vec<Duty>, purpose: Purpose, now: Moment) {
        self.ledger.landed(errand);
        for duty in duties {
            match duty {
                Duty::Bury(path) => {
                    self.keeping.remove(&path);
                    let found = self.shared.reading().resolve(&path).filter(|_| self.plan.wanted(&path));
                    if let Some(entry) = found.filter(|_| matches!(self.system.stat(&path), Ok(None))) {
                        self.bury(&path, entry, now);
                    }
                }
                Duty::Scanned { root, reach } => self.stamp(&root, reach, now),
                Duty::Guarded if self.full => {
                    self.meta.flags |= meta::GUARDED_DONE;
                    self.timers.save_soon = true;
                }
                Duty::Guarded => {}
            }
        }
        if purpose == Purpose::Reconcile {
            self.timers.save_soon = true;
        }
        self.tell_of_apps();
    }

    fn defer(&mut self, work: Work, due: Instant, live: bool, id: Option<u64>) {
        let waiting = self.deferred.iter().find(|deferred| deferred.work == work).map(|deferred| deferred.errand);
        let errand = waiting.unwrap_or_else(|| {
            let errand = self.errands.open(Vec::new(), Purpose::Live);
            self.deferred.push(Deferred { errand, due, work, live });
            errand
        });
        id.into_iter().for_each(|id| self.ledger.opened(errand, id));
    }

    fn bury(&mut self, path: &Path, entry: EntryId, now: Moment) {
        {
            let mut live = self.shared.writing();
            live.bury(entry);
            live.settle();
        }
        if self.config.learn {
            self.shared.usage.observe(&[Seen::Removed { path: path.to_path_buf() }], now.stamp);
        }
    }

    fn restat(&mut self, path: &Path, entry: EntryId, raw: &Raw, now: Moment) {
        let (before, flags) = {
            let live = self.shared.reading();
            (live.modified(entry), live.flags(entry))
        };
        let seen = [(raw::HIDDEN, flag::HIDDEN), (raw::CLOUD, flag::CLOUD)];
        let wanted =
            seen.iter().fold(
                flags & !(flag::HIDDEN | flag::CLOUD),
                |all, (bit, kept)| {
                    if raw.bits & bit != 0 { all | kept } else { all }
                },
            );
        let live = self.shared.reading();
        if wanted != flags {
            live.reflag(entry, wanted);
        }
        if before == raw.modified {
            return;
        }
        live.touch(entry, raw.modified);
        drop(live);
        if raw.modified > before && flags & flag::DIR == 0 && self.config.learn {
            let edited = Seen::Edited { path: path.to_path_buf(), at: raw.modified, plain: super::apply::plain(flags) };
            self.shared.usage.observe(&[edited], now.stamp);
        }
    }

    fn same_identity(&self, entry: EntryId, raw: &Raw) -> bool {
        let stored = self.shared.reading().flags(entry) & flag::IDENTITY;
        let found = [(raw::DIR, flag::DIR), (raw::LINK, flag::LINK)];
        stored == found.iter().filter(|(bit, _)| raw.bits & bit != 0).fold(0, |all, (_, kept)| all | kept)
    }

    pub(super) fn known_at_or_above(&self, path: &Path) -> Option<PathBuf> {
        let live = self.shared.reading();
        path.ancestors().find(|above| live.resolve(above).is_some()).map(Path::to_path_buf)
    }
}
