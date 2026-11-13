//! Progress and cancelling for one operation. A task splits into parts, and each part reports inside its own share.

use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Task<'a> {
    report: &'a mut dyn FnMut(f32),
    cancel: &'a AtomicBool,
    start: f32,
    span: f32,
}

impl<'a> Task<'a> {
    pub(super) fn new(report: &'a mut dyn FnMut(f32), cancel: &'a AtomicBool) -> Self {
        Self { report, cancel, start: 0.0, span: 1.0 }
    }

    pub(super) fn report(&mut self, fraction: f32) {
        (self.report)(self.start + self.span * fraction.clamp(0.0, 1.0));
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(super) fn part(&mut self, index: usize, count: usize) -> Task<'_> {
        let span = self.span / count.max(1) as f32;
        Task { report: &mut *self.report, cancel: self.cancel, start: self.start + span * index as f32, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_report_their_share() {
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let mut report = |fraction: f32| seen.push(fraction);
        let mut task = Task::new(&mut report, &cancel);
        task.part(1, 4).report(0.5);
        task.part(3, 4).part(1, 2).report(1.0);
        task.report(2.0);
        assert_eq!(seen, vec![0.375, 1.0, 1.0]);
    }

    #[test]
    fn cancellation_is_seen_by_every_part() {
        let cancel = AtomicBool::new(false);
        let mut report = |_: f32| {};
        let mut task = Task::new(&mut report, &cancel);
        assert!(!task.part(0, 2).cancelled());
        cancel.store(true, Ordering::Relaxed);
        assert!(task.part(1, 2).cancelled());
    }
}
