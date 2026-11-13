//! The calendar thread. It reads events around now every 30 seconds, sooner when poked, and makes the edits it gets.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use notchbase_sdk::now_ms;
use notchbase_sdk::protocol::CalendarFeed;

use super::change::Change;
use super::imp::Store;
use super::window;
use crate::poll::{Latest, Outbox, Shift, Worker, lock};

const THREAD: &str = "notchbase-calendar";
const POLL: Duration = Duration::from_secs(30);
const ASKING: Duration = Duration::from_secs(1);
const NAP: Duration = Duration::from_millis(100);

#[derive(Default)]
struct Shared {
    latest: Latest<CalendarFeed>,
    changes: Outbox<Change>,
    problems: Outbox<String>,
    reach: Mutex<Option<[u64; 2]>>,
    poked: AtomicBool,
}

impl Shared {
    fn stirred(&self) -> bool {
        self.poked.load(Ordering::Relaxed) || !self.changes.is_empty()
    }
}

pub struct SystemCalendar {
    worker: Worker<Shared>,
}

impl SystemCalendar {
    pub fn start() -> Self {
        Self { worker: Worker::start(THREAD, NAP, Shared::default(), watch) }
    }

    pub fn take(&self) -> Option<CalendarFeed> {
        self.worker.state().latest.take()
    }

    pub fn change(&self, change: Change) {
        self.worker.state().changes.push(change);
    }

    pub fn reach(&self, range: Option<[u64; 2]>) {
        let shared = self.worker.state();
        let mut reach = lock(&shared.reach);
        if *reach != range {
            *reach = range;
            shared.poked.store(true, Ordering::Relaxed);
        }
    }

    pub fn take_problems(&self) -> Vec<String> {
        self.worker.state().problems.take_all()
    }
}

fn watch(shift: &Shift<Shared>) {
    let shared = shift.state();
    let mut store = Store::new();
    while shift.on_duty() {
        let changes = shared.changes.take_all();
        let problems: Vec<String> = changes.iter().filter_map(|change| change.apply(&mut store).err()).collect();
        shared.problems.extend(problems);
        shared.poked.store(false, Ordering::Relaxed);
        let (from, to) = window::around(now_ms(), *lock(&shared.reach));
        let feed = store.feed(from, to);
        let wait = if feed == CalendarFeed::Waiting { ASKING } else { POLL };
        shared.latest.publish(feed);
        shift.nap_until(Instant::now() + wait, Shared::stirred);
    }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::EventDraft;

    use super::*;

    const PATIENCE: Duration = Duration::from_secs(5);

    fn eventually<T>(mut found: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(value) = found() {
                return Some(value);
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn only_changes_and_reach_stir() {
        let shared = Shared::default();
        assert!(!shared.stirred());
        shared.changes.push(Change::Delete("event".to_owned()));
        assert!(shared.stirred());
        shared.changes.take_all();
        shared.poked.store(true, Ordering::Relaxed);
        assert!(shared.stirred());
    }

    #[cfg_attr(target_os = "macos", ignore = "reads this computer's real calendars")]
    #[test]
    fn no_calendar_says_so_once() {
        let calendar = SystemCalendar::start();
        assert_eq!(eventually(|| calendar.take()), Some(CalendarFeed::Unavailable));
        assert_eq!(calendar.take(), None);
    }

    #[cfg_attr(target_os = "macos", ignore = "edits this computer's real calendars")]
    #[test]
    fn failed_change_reports_at_once() {
        let calendar = SystemCalendar::start();
        eventually(|| calendar.take());
        let asked = Instant::now();
        calendar.change(Change::Delete("event".to_owned()));
        calendar.change(Change::Save(EventDraft::default()));
        let problems = eventually(|| Some(calendar.take_problems()).filter(|problems| problems.len() == 2));
        assert_eq!(problems.map(|problems| problems.len()), Some(2));
        assert!(asked.elapsed() < POLL, "answered after {:?}", asked.elapsed());
    }

    #[test]
    fn reach_pokes_only_when_it_moves() {
        let calendar = SystemCalendar { worker: Worker::start("notchbase-test-reach", NAP, Shared::default(), |_| {}) };
        let poked = || calendar.worker.state().poked.swap(false, Ordering::Relaxed);
        calendar.reach(Some([1, 2]));
        assert!(poked());
        calendar.reach(Some([1, 2]));
        assert!(!poked(), "the same reach again is no news");
        calendar.reach(None);
        assert!(poked());
        assert_eq!(*lock(&calendar.worker.state().reach), None);
    }
}
