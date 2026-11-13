//! Remembers which apps made a sound in the last eight seconds, so they sort ahead of quiet ones.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::AppId;

const AUDIBLE: f32 = 0.001;
const RECENT: Duration = Duration::from_secs(8);

#[derive(Default)]
pub struct Activity {
    last_audible: HashMap<AppId, Instant>,
}

impl Activity {
    pub fn note(&mut self, id: AppId, peak: f32, now: Instant) {
        if peak > AUDIBLE {
            self.last_audible.insert(id, now);
        }
    }

    pub fn is_recent(&self, id: AppId, now: Instant) -> bool {
        self.last_audible.get(&id).is_some_and(|&t| now.saturating_duration_since(t) <= RECENT)
    }

    pub fn retain(&mut self, alive: impl Fn(AppId) -> bool) {
        self.last_audible.retain(|&id, _| alive(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_expires() {
        let mut activity = Activity::default();
        let t0 = Instant::now();
        activity.note(7, 0.0, t0);
        assert!(!activity.is_recent(7, t0), "silence doesn't count");
        activity.note(7, 0.5, t0);
        assert!(activity.is_recent(7, t0 + Duration::from_secs(2)));
        assert!(!activity.is_recent(7, t0 + RECENT + Duration::from_millis(1)));
        activity.retain(|id| id != 7);
        assert!(!activity.is_recent(7, t0));
    }
}
