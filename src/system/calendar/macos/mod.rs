//! Apple Calendar through EventKit, asking for full access the first time it's needed.

mod read;
mod write;

use block2::RcBlock;
use notchbase_sdk::protocol::{CalendarFeed, EventDraft};
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
use objc2_foundation::NSError;

use crate::system::permissions::{Access, may_ask};

const NO_ACCESS: &str = "notchbase can't reach Apple Calendar";

pub(super) struct Store {
    store: Retained<EKEventStore>,
    opened_with_access: bool,
    asked: bool,
}

impl Store {
    pub(super) fn new() -> Self {
        let opened_with_access = status() == EKAuthorizationStatus::FullAccess;
        Self { store: unsafe { EKEventStore::new() }, opened_with_access, asked: false }
    }

    pub(super) fn feed(&mut self, from_ms: u64, to_ms: u64) -> CalendarFeed {
        match status() {
            EKAuthorizationStatus::FullAccess => {
                if !std::mem::replace(&mut self.opened_with_access, true) {
                    self.store = unsafe { EKEventStore::new() };
                }
                unsafe { self.store.reset() };
                CalendarFeed::Events {
                    events: read::events(&self.store, from_ms, to_ms),
                    calendars: read::calendars(&self.store),
                }
            }
            EKAuthorizationStatus::NotDetermined => {
                self.opened_with_access = false;
                if !self.asked && may_ask(Access::Calendar) {
                    self.asked = true;
                    self.ask();
                }
                CalendarFeed::Waiting
            }
            _ => {
                self.opened_with_access = false;
                CalendarFeed::Denied
            }
        }
    }

    pub(super) fn save(&mut self, draft: &EventDraft) -> Result<(), String> {
        self.allowed()?;
        write::save(&self.store, draft)
    }

    pub(super) fn delete(&mut self, id: &str) -> Result<(), String> {
        self.allowed()?;
        write::delete(&self.store, id)
    }

    fn allowed(&self) -> Result<(), String> {
        (status() == EKAuthorizationStatus::FullAccess).then_some(()).ok_or_else(|| NO_ACCESS.to_owned())
    }

    fn ask(&self) {
        let answered = RcBlock::new(|_granted: Bool, _error: *mut NSError| {});
        unsafe { self.store.requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&answered)) };
    }
}

fn status() -> EKAuthorizationStatus {
    unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) }
}
