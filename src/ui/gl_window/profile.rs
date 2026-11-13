//! With NOTCHBASE_PROFILE set, prints how long each stage of a frame took, averaged over 120 frames.

use std::time::Duration;

const FRAMES: u32 = 120;

#[derive(Debug, Default, PartialEq)]
pub(super) struct Profile {
    frames: u32,
    stages: [Duration; 4],
}

impl Profile {
    pub(super) fn add(&mut self, title: &str, stages: [Duration; 4]) {
        self.frames += 1;
        for (total, d) in self.stages.iter_mut().zip(stages) {
            *total += d;
        }
        if self.frames == FRAMES {
            let [ui, tessellate, paint, swap] = self.stages.map(average_ms);
            eprintln!(
                "[profile] {title}: ui {ui:.2} ms, tessellate {tessellate:.2} ms, paint {paint:.2} ms, swap {swap:.2} ms"
            );
            *self = Self::default();
        }
    }
}

fn average_ms(total: Duration) -> f64 {
    total.as_secs_f64() * 1000.0 / f64::from(FRAMES)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGES: [Duration; 4] =
        [Duration::from_millis(1), Duration::from_millis(2), Duration::from_millis(3), Duration::from_millis(4)];

    #[test]
    fn frames_add_up_until_report() {
        let mut profile = Profile::default();
        for _ in 0..FRAMES - 1 {
            profile.add("notch", STAGES);
        }
        assert_eq!(profile.frames, FRAMES - 1);
        assert_eq!(profile.stages[3], Duration::from_millis(4) * (FRAMES - 1));
    }

    #[test]
    fn report_starts_a_fresh_count() {
        let mut profile = Profile::default();
        for _ in 0..FRAMES {
            profile.add("notch", STAGES);
        }
        assert_eq!(profile, Profile::default());
        profile.add("notch", STAGES);
        assert_eq!((profile.frames, profile.stages), (1, STAGES));
    }

    #[test]
    fn average_is_one_frames_ms() {
        assert_eq!(average_ms(Duration::from_millis(240)), 2.0);
        assert_eq!(average_ms(Duration::ZERO), 0.0);
        assert!((average_ms(Duration::from_micros(150)) - 0.00125).abs() < 1e-12);
    }
}
