//! The safe cursor, the change id Find resumes from after a restart. It follows the newest event but never passes one
//! whose work has not landed.

use std::collections::HashMap;

pub struct Ledger {
    floor: u64,
    highest: Option<u64>,
    open: HashMap<u32, u64>,
}

impl Ledger {
    pub fn seeded(floor: u64) -> Self {
        Self { floor, highest: None, open: HashMap::new() }
    }

    pub fn saw(&mut self, id: u64) {
        self.highest = Some(self.highest.map_or(id, |highest| highest.max(id)));
    }

    pub fn opened(&mut self, errand: u32, oldest: u64) {
        let held = self.open.entry(errand).or_insert(oldest);
        *held = (*held).min(oldest);
    }

    pub fn landed(&mut self, errand: u32) {
        self.open.remove(&errand);
    }

    /// Just before the oldest change still being worked on, or the newest one seen when nothing is.
    pub fn safe(&self, coalescing: Option<u64>) -> u64 {
        let after_the_newest = self.highest.map(|highest| highest.saturating_add(1));
        let unlanded = self.open.values().copied().chain(coalescing).chain(after_the_newest).min();
        unlanded.map_or(self.floor, |oldest| oldest.saturating_sub(1).max(self.floor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_starts_on_its_floor() {
        let ledger = Ledger::seeded(500);
        assert_eq!(ledger.safe(None), 500);
    }

    #[test]
    fn cursor_follows_newest_event() {
        let mut ledger = Ledger::seeded(500);
        [510, 530, 520].into_iter().for_each(|id| ledger.saw(id));
        assert_eq!(ledger.safe(None), 530);
    }

    #[test]
    fn cursor_waits_for_unlanded_work() {
        let mut ledger = Ledger::seeded(100);
        (101..=140).for_each(|id| ledger.saw(id));
        assert_eq!(ledger.safe(Some(120)), 119, "a change still waiting in the slow lane");
        ledger.opened(7, 110);
        assert_eq!(ledger.safe(Some(120)), 109, "an errand whose listing has not arrived");
        ledger.opened(9, 105);
        assert_eq!(ledger.safe(None), 104, "a job the scanner holds while paused");
        ledger.landed(9);
        assert_eq!(ledger.safe(None), 109);
        ledger.landed(7);
        assert_eq!(ledger.safe(Some(120)), 119);
        assert_eq!(ledger.safe(None), 140);
    }

    #[test]
    fn reopened_errand_keeps_oldest() {
        let mut ledger = Ledger::seeded(0);
        ledger.saw(90);
        ledger.opened(3, 60);
        ledger.opened(3, 80);
        assert_eq!(ledger.safe(None), 59);
    }

    #[test]
    fn cursor_never_drops_below_floor() {
        let mut ledger = Ledger::seeded(1_000);
        ledger.saw(1_200);
        ledger.opened(1, 3);
        assert_eq!(ledger.safe(Some(0)), 1_000);
    }
}
