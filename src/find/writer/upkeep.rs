//! Upkeep on every tick: reading the power and setting the scan's pace, rebuilding when the overlay grows, saving the
//! index and reporting status. When the index outgrows its budget, each level up holds back more of the scan.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::Power;

use crate::find::context::{HOUR, MINUTE, Stamp};
use crate::find::model::Room;
use crate::find::os::root;
use crate::find::scan::Pace;
use crate::find::shared::{Asked, lock};
use crate::find::store;
use crate::find::view::{Access, Phase, Status};

use super::Writer;
use super::errands::Purpose;
use super::system::Moment;

pub(super) const HARD_STOP: u8 = 5;
pub(super) const ROW_BYTES: u64 = 35;
pub(super) const OVERLAY_ROOM: u64 = 60 << 20;
const PEAK_SHARE: u64 = 32;
const PEAK_LEAST: u64 = 192 << 20;
const PEAK_MOST: u64 = 512 << 20;
const MOST_ENTRIES: u64 = 50_000_000;
const FAST_TICK: Duration = Duration::from_millis(250);
const TICK: Duration = Duration::from_secs(1);
const POWER_EVERY: Duration = Duration::from_secs(30);
const PROBE_EVERY: Duration = Duration::from_secs(30);
pub(super) const IDLE: Duration = Duration::from_secs(30);
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
const REFRESH_EVERY: Duration = Duration::from_millis(500);
pub(super) const IDLE_ROWS: usize = 16_384;
pub(super) const SAVE_EVERY: Stamp = 60 * MINUTE;
pub(super) const SAVE_LATEST: Stamp = 6 * HOUR;

pub(super) struct Timers {
    pub last_order: Instant,
    pub last_tick: Instant,
    pub added: usize,
    pub save_soon: bool,
    power: Option<(Instant, Power)>,
    pace: Option<Pace>,
    pub probed: Instant,
    pub saved_at: Stamp,
    pub saved_revision: u64,
    reported: Option<Instant>,
    unsent: bool,
    refreshed: Option<(Instant, u64)>,
}

impl Timers {
    pub(super) fn new(now: Moment) -> Self {
        Self {
            last_order: now.at,
            last_tick: now.at,
            added: 0,
            save_soon: false,
            power: None,
            pace: None,
            probed: now.at,
            saved_at: now.stamp,
            saved_revision: 0,
            reported: None,
            unsent: false,
            refreshed: None,
        }
    }

    pub(super) fn on_battery(&self) -> bool {
        self.power.is_some_and(|(_, power)| !power.plugged && power.level.is_some())
    }
}

/// The most rows the index may hold. A rebuild briefly holds two copies, and that peak may use a
/// thirty-second of the memory, between 192 and 512 MB.
pub fn cap(ram_bytes: u64, max_entries: u64) -> usize {
    if max_entries > 0 {
        return max_entries.min(MOST_ENTRIES) as usize;
    }
    let peak = (ram_bytes / PEAK_SHARE).clamp(PEAK_LEAST, PEAK_MOST);
    ((peak - OVERLAY_ROOM) / (2 * ROW_BYTES)) as usize
}

impl Writer {
    pub(super) fn upkeep(&mut self, now: Moment) {
        if self.timers.power.is_none_or(|(read, _)| now.at.saturating_duration_since(read) >= POWER_EVERY) {
            self.timers.power = Some((now.at, self.system.power()));
        }
        self.steer();
        let visible = self.shared.visible.load(Ordering::Acquire);
        if !self.full && visible && now.at.saturating_duration_since(self.timers.probed) >= PROBE_EVERY {
            self.probe(now);
        }
        let idle = now.at.saturating_duration_since(self.timers.last_order) >= IDLE;
        if idle && !self.timers.on_battery() && self.timers.added > IDLE_ROWS {
            self.rebuild();
        }
        self.keep_index_in_room(now);
        self.save_when_due(now);
        self.reconcile(now);
        if self.config.learn {
            let _ = self.shared.usage.save_if_due(now.stamp);
        }
    }

    pub(super) fn keep_index_in_room(&mut self, _now: Moment) {
        if self.shared.reading().wants_rebuild() {
            self.rebuild();
        }
        let cap = cap(self.ram_bytes, self.config.max_entries);
        while self.shared.reading().alive() >= cap && self.meta.level < HARD_STOP {
            self.meta.level += 1;
            self.scanner.follow(self.plan.clone(), self.meta.level);
            self.rebuild();
        }
    }

    pub(super) fn rebuild(&mut self) {
        let boot = self.roots.iter().find(|found| found.flags & root::BOOT != 0).map(|found| found.path.clone());
        let cap = cap(self.ram_bytes, self.config.max_entries);
        let rebuilt = {
            let live = self.shared.reading();
            let boot = boot.and_then(|path| live.resolve(&path));
            live.rebuilt(&Room { cap, level: self.meta.level, boot })
        };
        self.shared.writing().adopt(rebuilt);
        self.timers.added = 0;
    }

    pub(super) fn report(&mut self, now: Moment) {
        let status = self.status();
        let visible = self.shared.visible.load(Ordering::Acquire);
        let previous = std::mem::replace(&mut *lock(&self.shared.status), status);
        let notable = (status.phase, status.access, status.truncated, status.unwatched)
            != (previous.phase, previous.access, previous.truncated, previous.unwatched);
        self.timers.unsent |= status != previous;
        let progress =
            visible && self.timers.reported.is_none_or(|at| now.at.saturating_duration_since(at) >= PROGRESS_EVERY);
        if notable || (self.timers.unsent && progress) {
            (self.timers.reported, self.timers.unsent) = (Some(now.at), false);
            self.shared.publish();
        }
        let revision = self.shared.reading().revision();
        let changed = self.timers.refreshed.is_none_or(|(_, seen)| seen != revision);
        let spaced = self.timers.refreshed.is_none_or(|(at, _)| now.at.saturating_duration_since(at) >= REFRESH_EVERY);
        if visible && changed && spaced {
            self.timers.refreshed = Some((now.at, revision));
            if let Some(asks) = &self.asks {
                let _ = asks.send(Asked::Refresh);
            }
        }
    }

    pub(super) fn wait(&self) -> Duration {
        let now = self.system.now();
        match self.interval() {
            Some(interval) => interval.saturating_sub(now.at.saturating_duration_since(self.timers.last_tick)),
            None => self.idle_wait(now),
        }
    }

    pub(super) fn tick_due(&self) -> bool {
        self.wait().is_zero()
    }

    fn interval(&self) -> Option<Duration> {
        let visible = self.shared.visible.load(Ordering::Acquire);
        let busy = !self.errands.is_empty() || !self.deferred.is_empty() || !self.reconciles.is_empty();
        if !self.config.enabled {
            None
        } else if !self.coalescer.is_empty() {
            Some(FAST_TICK)
        } else if busy || visible || self.timers.save_soon {
            Some(TICK)
        } else {
            None
        }
    }

    fn status(&self) -> Status {
        let phase = match () {
            _ if !self.config.enabled => Phase::Off,
            _ if !self.launched => Phase::Loading,
            _ if self.errands.any(Purpose::Launch) => Phase::Scanning,
            _ => Phase::Watching,
        };
        let entries = u32::try_from(self.shared.reading().alive()).unwrap_or(u32::MAX);
        let access = if self.full { Access::Full } else { Access::Limited };
        let unwatched = self.roots.iter().filter(|found| self.unwatched.contains(&found.path)).count();
        let unwatched = u8::try_from(unwatched).unwrap_or(u8::MAX);
        Status { phase, entries, access, truncated: self.meta.level > 0, unwatched }
    }

    pub(super) fn steer(&mut self) {
        let visible = self.shared.visible.load(Ordering::Acquire);
        let power =
            self.timers.power.map(|(_, power)| power).unwrap_or(Power { level: None, plugged: true, charging: false });
        let mut pace = Pace::of(&power, visible);
        let reconciling = self.errands.any(Purpose::Reconcile) && !self.errands.any(Purpose::Launch);
        if pace == Pace::Full && reconciling && !visible {
            pace = Pace::Gentle;
        }
        if self.timers.pace != Some(pace) {
            self.timers.pace = Some(pace);
            self.scanner.pace(pace);
        }
    }

    fn save_when_due(&mut self, now: Moment) {
        if !self.writes || !self.launched || !self.config.enabled {
            return;
        }
        let dirty = self.shared.reading().revision() != self.timers.saved_revision;
        let since = now.stamp.saturating_sub(self.timers.saved_at);
        let periodic = dirty && since >= SAVE_EVERY && (!self.timers.on_battery() || since >= SAVE_LATEST);
        if self.timers.save_soon || periodic {
            self.save(now);
        }
    }

    pub(super) fn save(&mut self, now: Moment) {
        let revision = match self.dir.clone() {
            Some(dir) => self.write_index(&dir, now),
            None => self.shared.reading().revision(),
        };
        (self.timers.saved_at, self.timers.saved_revision, self.timers.save_soon) = (now.stamp, revision, false);
    }

    fn write_index(&mut self, dir: &Path, now: Moment) -> u64 {
        if !self.shared.reading().settled() {
            self.rebuild();
        }
        self.meta.saved_at = now.stamp;
        self.meta.cursor = self.ledger.safe(self.coalescer.oldest());
        let live = self.shared.reading();
        if let Err(failed) = store::save(dir, &live, &self.meta) {
            eprintln!("notchbase: Find could not save its index: {failed}");
        }
        live.revision()
    }
}
