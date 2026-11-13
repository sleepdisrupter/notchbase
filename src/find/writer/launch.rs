//! Starting up. The writer loads the saved index, matches the roots it finds to the ones it remembers, watches from the
//! saved cursor and scans what was left unfinished. Without a saved cursor it reconciles the finished roots gently.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use memchr::memmem;

use crate::find::model::{Entry, Live, flag, with_tier};
use crate::find::os::{Root, root};
use crate::find::scan::Plan;
use crate::find::shared::Asked;
use crate::find::store::{self, RootMark, mark, meta};

use super::changes::EVERYTHING;
use super::errands::{Duty, Purpose};
use super::ledger::Ledger;
use super::system::Moment;
use super::{Work, Writer};

const MEDIA: [&[u8]; 3] = [b".photoslibrary\0", b".musiclibrary\0", b".tvlibrary\0"];

impl Writer {
    pub fn launch(&mut self) {
        let now = self.system.now();
        self.full = self.system.full_access();
        self.report(now);
        let known = self.restore();
        let roots = self.system.roots(self.config.volumes);
        self.adopt_roots(roots);
        if !self.full {
            self.meta.flags &= !meta::GUARDED_DONE;
            self.bury_guarded();
        }
        if !self.config.excluded.is_empty() {
            self.exclude();
        }
        self.shared.publish();
        let keeps = self.system.keeps_cursor();
        let floor = if known && keeps && self.meta.cursor != 0 { self.meta.cursor } else { self.system.cursor_now() };
        self.ledger = Ledger::seeded(floor);
        self.launched = true;
        self.tell_of_apps();
        self.watch();
        self.scan_incomplete(now);
        self.recheck_apps(now);
        if self.full && self.meta.flags & meta::GUARDED_DONE == 0 {
            self.open_guarded(true, now);
        }
        if !keeps {
            let complete = self.meta.roots.iter().filter(|found| found.flags & mark::COMPLETE != 0);
            self.reconciles = complete.map(|found| found.path.clone()).filter(|path| self.plan.wanted(path)).collect();
        }
        self.report(now);
    }

    pub(super) fn adopt_roots(&mut self, roots: Vec<Root>) {
        let mut marks = std::mem::take(&mut self.meta.roots);
        let kept: Vec<RootMark> = roots.iter().map(|found| matched(&mut marks, found)).collect();
        {
            let mut live = self.shared.writing();
            for gone in marks {
                if let Some(row) = live.resolve(&gone.path).filter(|row| live.parent(*row).is_none()) {
                    live.bury(row);
                }
            }
            live.settle();
        }
        (self.meta.roots, self.roots) = (kept, roots);
        self.replan();
        let mut live = self.shared.writing();
        for found in &self.roots {
            if live.resolve(&found.path).is_none_or(|row| live.parent(row).is_some()) {
                let flags = with_tier(flag::DIR, self.plan.inside(&found.path));
                live.add(None, Entry { name: &found.path.to_string_lossy(), modified: 0, created: 0, flags });
            }
        }
    }

    pub(super) fn replan(&mut self) {
        let (home, data) = (self.home.as_deref(), self.dir.as_deref());
        self.plan = Arc::new(self.system.plan(&self.config, &self.roots, home, data, self.full));
        self.shared.guard(self.plan.clone());
        let steering = if self.config.enabled { self.plan.clone() } else { Arc::new(Plan::resting()) };
        self.scanner.follow(steering, self.meta.level);
    }

    pub(super) fn watch(&mut self) {
        self.watcher = None;
        let watched: Vec<Root> = self.roots.iter().filter(|found| self.plan.wanted(&found.path)).cloned().collect();
        let since = self.system.keeps_cursor().then(|| self.ledger.safe(self.coalescer.oldest()));
        if self.writes && !watched.is_empty() {
            self.watcher = self.system.watch(&watched, since, self.sink.clone());
        }
        self.unwatched = match self.watcher {
            Some(_) => HashSet::new(),
            None => self.roots.iter().map(|found| found.path.clone()).collect(),
        };
    }

    pub(super) fn scan_incomplete(&mut self, now: Moment) {
        let incomplete = self.meta.roots.iter().filter(|found| found.flags & mark::COMPLETE == 0);
        let mut waiting: Vec<PathBuf> = incomplete.map(|found| found.path.clone()).collect();
        waiting.retain(|path| self.plan.wanted(path) && !self.scanning(path));
        waiting.sort_by_key(|path| self.rank(path));
        for root in waiting {
            let duties = vec![Duty::Scanned { root: root.clone(), reach: EVERYTHING }];
            self.errand(None, duties, Purpose::Launch, vec![Work { dir: root, deep: true, reach: EVERYTHING }], now);
        }
    }

    pub(super) fn recheck_apps(&mut self, now: Moment) {
        let places: Vec<PathBuf> = self.plan.app_places().map(Path::to_path_buf).collect();
        let mut known: Vec<PathBuf> = places
            .iter()
            .filter(|place| self.plan.wanted(place) && self.inside_complete(place))
            .filter_map(|place| self.known_at_or_above(place))
            .collect();
        known.sort();
        known.dedup();
        for place in known {
            self.relist(place, None, now);
        }
    }

    pub(super) fn tell_of_apps(&self) {
        if let Some(asks) = &self.asks {
            let _ = asks.send(Asked::Apps);
        }
    }

    pub(super) fn open_guarded(&mut self, only_where_complete: bool, now: Moment) {
        let mut targets = self.home.as_deref().map(|home| self.system.guarded(home)).unwrap_or_default();
        let removable = self.roots.iter().filter(|found| found.flags & root::REMOVABLE != 0);
        let removable: Vec<PathBuf> = removable.map(|found| found.path.clone()).collect();
        targets.extend(removable.iter().cloned());
        targets.extend(media(&self.shared.reading()));
        targets.retain(|target| self.plan.wanted(target) && (!only_where_complete || self.inside_complete(target)));
        targets.sort();
        targets.dedup();
        let mut duties = vec![Duty::Guarded];
        let scanned = removable.into_iter().filter(|path| targets.contains(path));
        duties.extend(scanned.map(|root| Duty::Scanned { root, reach: EVERYTHING }));
        let work = targets.into_iter().map(|dir| Work { dir, deep: true, reach: EVERYTHING }).collect();
        self.errand(None, duties, Purpose::Launch, work, now);
    }

    pub(super) fn rediscover(&mut self, now: Moment) {
        let roots = self.system.roots(self.config.volumes);
        if roots == self.roots {
            return;
        }
        self.adopt_roots(roots);
        self.watch();
        self.scan_incomplete(now);
    }

    pub(super) fn stamp(&mut self, root: &Path, reach: u8, now: Moment) {
        if let Some(found) = self.mark_of(root) {
            found.scanned_at = now.stamp;
            if reach >= EVERYTHING {
                found.swept_at = now.stamp;
                found.flags |= mark::COMPLETE;
            }
        }
        self.timers.save_soon = true;
    }

    pub(super) fn mark_of(&mut self, root: &Path) -> Option<&mut RootMark> {
        self.meta.roots.iter_mut().find(|found| found.path == root)
    }

    pub(super) fn is_root(&self, path: &Path) -> bool {
        self.roots.iter().any(|found| found.path == path)
    }

    fn restore(&mut self) -> bool {
        if self.shared.reading().len() > 0 {
            return true;
        }
        let Some(dir) = self.dir.as_deref().filter(|_| self.writes) else { return false };
        let Some((live, meta)) = store::load(dir) else { return false };
        *self.shared.writing() = live;
        self.meta = meta;
        true
    }

    fn scanning(&self, root: &Path) -> bool {
        self.errands.covers(&Duty::Scanned { root: root.to_path_buf(), reach: EVERYTHING })
    }

    fn inside_complete(&self, path: &Path) -> bool {
        let holders = self.meta.roots.iter().filter(|found| path.starts_with(&found.path));
        holders
            .max_by_key(|found| found.path.components().count())
            .is_some_and(|found| found.flags & mark::COMPLETE != 0)
    }

    fn rank(&self, path: &Path) -> Reverse<(bool, bool)> {
        let home = self.home.as_deref().is_some_and(|home| home.starts_with(path));
        Reverse((home, self.plan.leads_to_apps(path)))
    }
}

fn matched(marks: &mut Vec<RootMark>, found: &Root) -> RootMark {
    let Some(at) = marks.iter().position(|known| known.path == found.path) else {
        return RootMark { path: found.path.clone(), volume: found.volume, flags: 0, scanned_at: 0, swept_at: 0 };
    };
    let mut known = marks.swap_remove(at);
    if known.volume != found.volume {
        (known.volume, known.flags) = (found.volume, known.flags & !mark::COMPLETE);
    }
    known
}

pub(super) fn media(live: &Live) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (index, arena) in live.arenas().iter().enumerate() {
        for ending in MEDIA {
            for at in memmem::find_iter(arena.bytes, ending) {
                let id = live.id_at(index, at);
                if !live.is_dead(id) && live.flags(id) & flag::DIR != 0 {
                    let mut path = PathBuf::new();
                    live.path_into(id, &mut path);
                    out.push(path);
                }
            }
        }
    }
    out
}
