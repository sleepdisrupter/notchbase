//! Other systems have only the partial files to go on.

use std::path::Path;

use super::scan::Listing;

pub(super) fn bundle_total(_bundle: &Path) -> Option<u64> {
    None
}

pub(super) struct Subscription;

impl Subscription {
    pub(super) fn start(_folder: &Path) -> Self {
        Self
    }

    pub(super) fn fill(&self, _listing: &mut Listing) {}
}
