//! Smooths the level meters: a quick 25 ms attack and a gentle 300 ms release, so they look alive without jitter.

use std::collections::HashMap;
use std::time::Instant;

use super::snapshot::Snapshot;
use crate::audio::AppId;

const ATTACK: f32 = 0.025;
const RELEASE: f32 = 0.3;
const MAX_STEP_SECS: f32 = 0.25;
const SILENCE: f32 = 0.002;

#[derive(Default)]
pub struct Meters {
    levels: HashMap<AppId, f32>,
    last: Option<Instant>,
}

impl Meters {
    pub fn apply(&mut self, snapshot: &mut Snapshot, now: Instant) {
        let dt = self.last.map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(MAX_STEP_SECS));
        self.last = Some(now);
        let mut next = HashMap::with_capacity(self.levels.len());
        for row in snapshot.rows_mut() {
            let level = smooth(self.levels.get(&row.id).copied().unwrap_or(0.0), row.peak, dt);
            row.peak = level;
            next.insert(row.id, level);
        }
        self.levels = next;
    }
}

fn smooth(level: f32, peak: f32, dt: f32) -> f32 {
    let target = peak.clamp(0.0, 1.0);
    let next = if target >= level {
        level + (target - level) * (1.0 - (-dt / ATTACK).exp())
    } else {
        target.max(level * (-dt / RELEASE).exp())
    };
    if next < SILENCE { 0.0 } else { next }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioApp;
    use crate::audio::driver::cadence::TICK;

    fn app(id: AppId, peak: f32) -> AudioApp {
        AudioApp { id, name: format!("app {id}"), volume: 0.5, muted: false, peak, icon: None }
    }

    #[test]
    fn attack_fast_release_gently() {
        let mut level = 0.0;
        level = smooth(level, 1.0, 0.033);
        assert!(level > 0.7, "attack {level}");
        let mut t = 0.0;
        while level > 0.0 {
            level = smooth(level, 0.0, 0.033);
            t += 0.033;
            assert!(t < 3.0, "release must reach zero");
        }
        assert!(t > 0.8, "release too fast: {t}");
        assert_eq!(smooth(0.5, 0.5, 0.0), 0.5);
    }

    #[test]
    fn meters_are_smoothed_per_row() {
        let mut meters = Meters::default();
        let t0 = Instant::now();
        let mut snap = Snapshot { apps: vec![app(1, 1.0), app(2, 0.0)], ..Snapshot::default() };
        meters.apply(&mut snap, t0);
        assert_eq!(snap.apps[0].peak, 0.0, "first sample has no elapsed time");
        let mut snap = Snapshot { apps: vec![app(1, 1.0), app(2, 0.0)], ..Snapshot::default() };
        meters.apply(&mut snap, t0 + TICK);
        assert!(snap.apps[0].peak > 0.5);
        assert_eq!(snap.apps[1].peak, 0.0);
    }
}
