//! Reports transcoding progress by media time, in small steps.

use crate::shelf::Task;

const STEP: f32 = 0.005;

pub(super) struct Progress {
    total_seconds: f64,
    reported: f32,
}

impl Progress {
    pub(super) fn new(total_seconds: Option<f64>) -> Self {
        Self { total_seconds: total_seconds.unwrap_or(0.0), reported: 0.0 }
    }

    pub(super) fn reached(&mut self, task: &mut Task, seconds: f64) {
        if self.total_seconds <= 0.0 {
            return;
        }
        let fraction = (seconds / self.total_seconds).clamp(0.0, 1.0) as f32;
        if fraction - self.reported >= STEP {
            self.reported = fraction;
            task.report(fraction);
        }
    }
}
