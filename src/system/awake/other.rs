//! Other systems can't be kept awake from here.

pub(super) struct Assertion;

impl Assertion {
    pub(super) fn hold() -> Option<Self> {
        None
    }

    pub(super) fn renew(&mut self) {}
}

pub(super) fn forget_lid() {}
