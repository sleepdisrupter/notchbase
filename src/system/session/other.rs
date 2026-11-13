//! Other systems never report a lock.

pub(super) fn locked() -> bool {
    false
}
