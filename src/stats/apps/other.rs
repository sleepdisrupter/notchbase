//! Other systems have no app list, and nothing can be quit.

use super::AppUsage;

pub(in crate::stats) struct Watch;

impl Watch {
    pub(in crate::stats) fn open() -> Self {
        Self
    }

    pub(in crate::stats) fn sample(&mut self) -> Vec<AppUsage> {
        Vec::new()
    }
}

pub(in crate::stats) fn quit(_key: &str, _force: bool) -> bool {
    false
}
