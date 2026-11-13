//! Frame times to keep while decoding, evenly spaced up to the limit. A long frame stands in for every time it covers.

const MIN_FPS: f64 = 0.01;

pub(super) struct Schedule {
    fps: f64,
    limit: f64,
    next: u64,
}

impl Schedule {
    pub(super) fn new(fps: f64, limit: f64) -> Self {
        Self { fps: fps.max(MIN_FPS), limit, next: 0 }
    }

    pub(super) fn finished(&self) -> bool {
        self.upcoming() >= self.limit
    }

    pub(super) fn due_before(&mut self, until: f64) -> Vec<f64> {
        let end = until.min(self.limit);
        let mut due = Vec::new();
        while self.upcoming() < end {
            due.push(self.upcoming());
            self.next += 1;
        }
        due
    }

    fn upcoming(&self) -> f64 {
        self.next as f64 / self.fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_spaced_up_to_limit() {
        let mut schedule = Schedule::new(12.0, 2.0);
        assert_eq!(schedule.due_before(0.03), vec![0.0]);
        assert_eq!(schedule.due_before(0.1).len(), 1);
        assert_eq!(schedule.due_before(10.0).len(), 22);
        assert!(schedule.finished());
    }

    #[test]
    fn long_frame_covers_its_moments() {
        let mut schedule = Schedule::new(10.0, 1.0);
        assert_eq!(schedule.due_before(0.35).len(), 4);
        assert!(!schedule.finished());
    }
}
