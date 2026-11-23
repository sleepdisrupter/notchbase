//! Other systems never report Mission Control.

pub(super) fn reader() -> impl FnMut() -> bool {
    || false
}
