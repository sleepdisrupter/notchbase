//! Other systems have no calendar to read.

use notchbase_sdk::protocol::{CalendarFeed, EventDraft};

const UNSUPPORTED: &str = "Editing events needs Apple Calendar";

pub(super) struct Store;

impl Store {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn feed(&mut self, _from_ms: u64, _to_ms: u64) -> CalendarFeed {
        CalendarFeed::Unavailable
    }

    pub(super) fn save(&mut self, _draft: &EventDraft) -> Result<(), String> {
        Err(UNSUPPORTED.to_owned())
    }

    pub(super) fn delete(&mut self, _id: &str) -> Result<(), String> {
        Err(UNSUPPORTED.to_owned())
    }
}
