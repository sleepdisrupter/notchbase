//! An edit to the calendar: save an event, or delete one.

use notchbase_sdk::protocol::EventDraft;

use super::imp::Store;

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Save(EventDraft),
    Delete(String),
}

impl Change {
    pub(super) fn apply(&self, store: &mut Store) -> Result<(), String> {
        match self {
            Change::Save(draft) => store.save(draft),
            Change::Delete(id) => store.delete(id),
        }
    }
}
