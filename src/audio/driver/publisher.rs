//! Takes a snapshot of the backend each tick and passes it on only when something visible changed.

use std::time::Instant;

use super::meters::Meters;
use super::snapshot::Snapshot;
use crate::audio::imp;

#[derive(Default)]
pub struct Publisher {
    meters: Meters,
    last: Option<Snapshot>,
}

impl Publisher {
    pub fn capture(&mut self, engine: &imp::Backend, seq: u64, now: Instant) -> Option<Snapshot> {
        let mut snapshot = Snapshot::capture(engine, seq);
        self.meters.apply(&mut snapshot, now);
        if self.last.as_ref().is_some_and(|last| !snapshot.differs(last)) {
            return None;
        }
        self.last = Some(snapshot.clone());
        Some(snapshot)
    }
}
