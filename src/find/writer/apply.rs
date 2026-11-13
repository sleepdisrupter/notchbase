//! Applies listings to the index. Each listing is compared with what the index holds for that folder: new names are
//! added, changed ones touched or reflagged, and missing ones buried. What changed is passed on to the usage model.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::find::model::{Entry, EntryId, Live, flag, tier, with_tier};
use crate::find::os::{Raw, raw};
use crate::find::scan::{Listing, Plan, Verdict};
use crate::find::usage::Seen;

use super::errands::Purpose;
use super::system::Moment;
use super::upkeep::HARD_STOP;
use super::{Work, Writer};

const END_OF_NAME: u8 = 0;
const KEPT_BY_REFLAG: u8 = flag::IDENTITY | flag::EXOTIC;
const CARRIED: [(u8, u8); 4] =
    [(raw::DIR, flag::DIR), (raw::HIDDEN, flag::HIDDEN), (raw::CLOUD, flag::CLOUD), (raw::LINK, flag::LINK)];

struct Diff<'a> {
    plan: &'a Plan,
    level: u8,
    keeping: &'a HashSet<PathBuf>,
    seen: Vec<Seen>,
    descend: Vec<(u32, Work)>,
    added: usize,
}

impl Writer {
    pub(super) fn listed(&mut self, mut listings: Vec<Listing>, now: Moment) {
        let rows: usize = listings.iter().map(|listing| listing.rows.len()).sum();
        if !self.config.enabled {
            self.scanner.landed(rows);
            return;
        }
        let plan = self.plan.clone();
        listings.retain(|listing| plan.wanted(&listing.dir));
        let mut diff = Diff {
            plan: &plan,
            level: self.meta.level,
            keeping: &self.keeping,
            seen: Vec::new(),
            descend: Vec::new(),
            added: 0,
        };
        {
            let mut live = self.shared.writing();
            listings.iter().for_each(|listing| diff.apply(&mut live, listing));
            live.settle();
        }
        let Diff { seen, descend, added, .. } = diff;
        self.scanner.landed(rows);
        self.timers.added += added;
        if self.config.learn && !seen.is_empty() {
            self.shared.usage.observe(&seen, now.stamp);
        }
        for (ticket, work) in descend {
            let errand = self.errands.errand_of(ticket).unwrap_or_else(|| self.errands.open(Vec::new(), Purpose::Live));
            self.dispatch(errand, vec![work], true, now);
        }
        self.keep_index_in_room(now);
    }
}

impl Diff<'_> {
    fn apply(&mut self, live: &mut Live, listing: &Listing) {
        let Some(dir) = live.resolve(&listing.dir).filter(|dir| live.flags(*dir) & flag::DIR != 0) else { return };
        let mut current = children(live, dir);
        let names = listing.names.split(|byte| *byte == END_OF_NAME);
        for (name, row) in names.zip(&listing.rows) {
            let name = String::from_utf8_lossy(name);
            let flags = flags_of(row, listing);
            match current.remove(&*name) {
                Some(id) if live.flags(id) & flag::IDENTITY == flags & flag::IDENTITY => {
                    self.refresh(live, id, listing, &name, row, flags);
                }
                Some(id) => {
                    live.bury(id);
                    self.add(live, dir, listing, &name, row, flags);
                }
                None => self.add(live, dir, listing, &name, row, flags),
            }
        }
        if !listing.complete {
            return;
        }
        for (name, id) in current {
            let path = listing.dir.join(&name);
            if !self.keeping.contains(&path) {
                live.bury(id);
                self.seen.push(Seen::Removed { path });
            }
        }
    }

    fn refresh(&mut self, live: &Live, id: EntryId, listing: &Listing, name: &str, row: &Raw, flags: u8) {
        let before = live.modified(id);
        if before != row.modified {
            live.touch(id, row.modified);
            if row.modified > before && flags & flag::DIR == 0 {
                let path = listing.dir.join(name);
                self.seen.push(Seen::Edited { path, at: row.modified, plain: plain(flags) });
            }
        }
        if live.flags(id) & !KEPT_BY_REFLAG != flags & !KEPT_BY_REFLAG {
            live.reflag(id, flags);
        }
    }

    fn add(&mut self, live: &mut Live, dir: EntryId, listing: &Listing, name: &str, row: &Raw, flags: u8) {
        if self.level >= HARD_STOP {
            return;
        }
        live.add(Some(dir), Entry { name, modified: row.modified, created: row.created, flags });
        self.added += 1;
        let path = listing.dir.join(name);
        let depth = listing.dir.components().count();
        let verdict = self.plan.verdict(&listing.dir, listing.tier, name, row.bits, depth);
        let descends = matches!(verdict, Verdict::Descend(inside)
            if !listing.deep && inside <= listing.reach && !self.plan.held_back(self.level, &path, inside));
        if descends {
            self.descend.push((listing.ticket, Work { dir: path.clone(), deep: true, reach: listing.reach }));
        }
        self.seen.push(Seen::Added { path, file_id: row.file_id, created: row.created, plain: plain(flags) });
    }
}

fn children(live: &Live, dir: EntryId) -> HashMap<String, EntryId> {
    let mut ids = Vec::new();
    live.children_into(dir, &mut ids);
    let mut name = String::new();
    ids.into_iter()
        .map(|id| {
            live.name_into(id, &mut name);
            (name.clone(), id)
        })
        .collect()
}

pub(super) fn flags_of(row: &Raw, listing: &Listing) -> u8 {
    let seen = CARRIED.iter().filter(|(bit, _)| row.bits & bit != 0).fold(0, |all, (_, kept)| all | kept);
    let package = if listing.inside_package { flag::INSIDE_PACKAGE } else { 0 };
    with_tier(seen | package, listing.tier)
}

pub(super) fn plain(flags: u8) -> bool {
    tier(flags) == 0 && flags & flag::HIDDEN == 0
}
