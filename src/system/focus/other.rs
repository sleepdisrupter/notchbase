//! Other systems never report a Focus.

use notchbase_sdk::protocol::Focus;

use super::Reading;

pub(super) fn read() -> Reading {
    Reading::Seen(Focus::default())
}
