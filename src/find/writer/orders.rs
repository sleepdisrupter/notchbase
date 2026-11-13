//! Orders from the app: picks and skips for the usage model, last-used dates to seed it, and settings changes.
//! Turning Find off rests the writer, a new exclusion buries what it covers, and one taken away is scanned again.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use memchr::memmem;

use crate::config::FindConfig;
use crate::find::context::{Context, Stamp};
use crate::find::key::Key;
use crate::find::kind::Kind;
use crate::find::model::{EntryId, fold_query};
use crate::find::shared::lock;
use crate::find::store::mark;
use crate::find::usage::{Seen, Used};
use crate::find::view::Pick;
use crate::find::watch::coalesce::Coalescer;

use super::Writer;
use super::apply::plain;
use super::system::Moment;
use super::upkeep::cap;

const HOME_MARK: char = '~';
const SEPARATORS: [char; 2] = ['/', '\\'];
const END_OF_NAME: u8 = 0;

impl Writer {
    pub(super) fn picked(&mut self, pick: &Pick, context: &Context) {
        if !self.config.learn {
            return;
        }
        let name = pick.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let flags = {
            let live = self.shared.reading();
            live.resolve(&pick.path).map_or(0, |entry| live.flags(entry))
        };
        self.shared.usage.picked(pick, Kind::of(&fold_query(&name), flags), context);
        if !self.launched || !self.plan.wanted(&pick.path) {
            return;
        }
        if let Ok(Some(found)) = self.system.stat(&pick.path).map(|found| found.filter(|found| found.file_id != 0)) {
            let path = pick.path.clone();
            let added = Seen::Added { path, file_id: found.file_id, created: found.created, plain: plain(flags) };
            self.shared.usage.observe(&[added], context.now);
        }
    }

    pub(super) fn left(&mut self, picked: Option<Key>, at: Stamp) {
        if self.config.learn {
            let predicted = lock(&self.shared.predicted).clone();
            self.shared.usage.left(&predicted, picked, at);
        }
    }

    pub(super) fn seed(&mut self, used: &[Used], now: Moment) {
        if self.config.learn && self.launched {
            let wanted: Vec<Used> = used.iter().filter(|seen| self.plan.wanted(&seen.path)).cloned().collect();
            self.shared.usage.seed(&wanted, now.stamp);
        }
    }

    pub(super) fn follow(&mut self, config: FindConfig, now: Moment) {
        let before = std::mem::replace(&mut self.config, config);
        if before.learn && !self.config.learn {
            self.shared.usage.forget();
        }
        if !self.config.enabled {
            return self.rest(&before);
        }
        if !before.enabled {
            if self.writes {
                self.launch();
            }
            return;
        }
        if before.excluded != self.config.excluded {
            self.replan();
            self.exclude();
            self.include(&before.excluded, now);
        }
        if before.volumes != self.config.volumes {
            self.rediscover(now);
        }
        let grew = cap(self.ram_bytes, self.config.max_entries) > cap(self.ram_bytes, before.max_entries);
        if grew && self.meta.level > 0 {
            self.meta.level = 0;
            self.forget_complete();
            self.replan();
            self.scan_incomplete(now);
        }
    }

    fn rest(&mut self, before: &FindConfig) {
        if before.enabled {
            self.meta.cursor = self.ledger.safe(self.coalescer.oldest());
            (self.watcher, self.launched) = (None, false);
            self.errands.abandon();
            (self.deferred, self.reconciles) = (Vec::new(), Vec::new());
            (self.coalescer, self.timers.save_soon) = (Coalescer::new(), false);
            self.relisted.clear();
            self.rescanned.clear();
            self.keeping.clear();
        }
        self.replan();
        if before.excluded != self.config.excluded {
            self.exclude();
            if before.excluded.iter().any(|entry| !self.config.excluded.contains(entry)) {
                self.forget_complete();
            }
        }
    }

    fn forget_complete(&mut self) {
        self.meta.roots.iter_mut().for_each(|found| found.flags &= !mark::COMPLETE);
    }

    pub(super) fn exclude(&mut self) {
        let (home, data) = (self.home.as_deref(), self.dir.as_deref());
        let plan = Arc::new(self.system.plan(&self.config, &self.roots, home, data, true));
        let names: Vec<Vec<u8>> = self.config.excluded.iter().filter_map(|entry| last_name(entry)).collect();
        let doomed: Vec<(EntryId, PathBuf)> = {
            let live = self.shared.reading();
            let mut doomed = Vec::new();
            for name in names {
                let whole: Vec<u8> = [END_OF_NAME].into_iter().chain(name).chain([END_OF_NAME]).collect();
                for (index, arena) in live.arenas().iter().enumerate() {
                    for at in memmem::find_iter(arena.bytes, &whole) {
                        let id = live.id_at(index, at + 1);
                        let mut path = PathBuf::new();
                        live.path_into(id, &mut path);
                        if !live.is_dead(id) && !plan.wanted(&path) {
                            doomed.push((id, path));
                        }
                    }
                }
            }
            doomed
        };
        {
            let mut live = self.shared.writing();
            doomed.iter().for_each(|(id, _)| live.bury(*id));
            live.settle();
        }
        self.shared.usage.purge(&|path: &Path| plan.wanted(path));
    }

    fn include(&mut self, before: &[String], now: Moment) {
        let freed = before.iter().filter(|entry| !self.config.excluded.contains(entry));
        let (mut paths, mut words) = (Vec::new(), false);
        for entry in freed {
            match self.path_of(entry) {
                Some(path) => paths.push(path),
                None => words = true,
            }
        }
        if words {
            self.forget_complete();
            return self.scan_incomplete(now);
        }
        paths.into_iter().for_each(|path| self.look(&path, None, now));
    }

    fn path_of(&self, entry: &str) -> Option<PathBuf> {
        let trimmed = entry.trim().trim_end_matches(SEPARATORS);
        if let Some(rest) = trimmed.strip_prefix(HOME_MARK) {
            return self.home.as_ref().map(|home| home.join(rest.trim_start_matches(SEPARATORS)));
        }
        trimmed.contains(SEPARATORS).then(|| PathBuf::from(trimmed))
    }
}

fn last_name(entry: &str) -> Option<Vec<u8>> {
    let trimmed = entry.trim().trim_end_matches(SEPARATORS);
    let name = trimmed.rsplit(SEPARATORS).next().filter(|name| !name.is_empty() && *name != "~")?;
    Some(fold_query(name))
}
