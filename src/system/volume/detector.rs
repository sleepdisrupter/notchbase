//! Notices a volume change made elsewhere: the same device, with the level moved or the mute flipped since last time.

use super::{Level, Reading};

const MOVED: f32 = 0.002;

#[derive(Default)]
pub struct Detector {
    last: Option<Reading>,
}

impl Detector {
    pub fn observe(&mut self, reading: Option<Reading>) -> Option<Level> {
        let Some(reading) = reading else {
            self.last = None;
            return None;
        };
        let changed = self.last.as_ref().is_some_and(|last| {
            last.device == reading.device
                && ((last.level.volume - reading.level.volume).abs() > MOVED || last.level.muted != reading.level.muted)
        });
        let level = reading.level;
        self.last = Some(reading);
        changed.then_some(level)
    }

    pub fn settle(&mut self, reading: Option<Reading>) {
        self.last = reading;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(device: &str, volume: f32, muted: bool) -> Option<Reading> {
        Some(Reading { device: device.into(), level: Level { volume, muted } })
    }

    #[test]
    fn only_real_changes_count() {
        let mut detector = Detector::default();
        assert_eq!(detector.observe(reading("a", 0.5, false)), None, "the first reading is the baseline");
        assert_eq!(detector.observe(reading("a", 0.5005, false)), None);
        assert_eq!(detector.observe(reading("a", 0.6, false)), Some(Level { volume: 0.6, muted: false }));
        assert_eq!(detector.observe(reading("a", 0.6, true)), Some(Level { volume: 0.6, muted: true }));
        assert_eq!(detector.observe(reading("b", 0.2, false)), None, "switching outputs isn't a change");
        assert_eq!(detector.observe(None), None);
        assert_eq!(detector.observe(reading("b", 0.9, false)), None);
    }

    #[test]
    fn settling_swallows_a_change_we_made() {
        let mut detector = Detector::default();
        detector.observe(reading("a", 0.5, false));
        detector.settle(reading("a", 0.7, false));
        assert_eq!(detector.observe(reading("a", 0.7, false)), None);
    }
}
