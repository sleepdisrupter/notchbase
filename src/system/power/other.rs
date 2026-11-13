//! Other systems report no battery.

use notchbase_sdk::protocol::Power;

pub(super) fn read() -> Power {
    Power::default()
}
