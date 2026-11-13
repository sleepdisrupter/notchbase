//! Copiers of closed windows, kept until their last picture is on the clipboard.

use crate::annotate::Copier;

struct Retired {
    session: u64,
    awaits: u64,
    copier: Copier,
}

#[derive(Default)]
pub(super) struct Retiring(Vec<Retired>);

impl Retiring {
    pub(super) fn keep(&mut self, session: u64, awaits: Option<u64>, copier: Option<Copier>) {
        if let (Some(awaits), Some(copier)) = (awaits, copier) {
            self.0.push(Retired { session, awaits, copier });
        }
    }

    pub(super) fn heard(&mut self, session: u64, revision: u64) {
        self.0.retain(|retired| retired.session != session || revision < retired.awaits);
    }

    pub(super) fn owes(&self, session: u64) -> Option<u64> {
        self.0.iter().filter(|retired| retired.session == session).map(|retired| retired.awaits).max()
    }

    pub(super) fn finish(&mut self) {
        for retired in self.0.drain(..) {
            retired.copier.finish();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::annotate::{Job, sample};
    use crate::capture::Screenshot;

    const SESSION: u64 = 4;
    const LAST_PICTURE: u64 = 9;
    const NOTHING: u64 = 0;
    const TINY: [usize; 2] = [8, 8];
    const SLOW_WRITE: Duration = Duration::from_millis(30);

    struct Gate(Sender<()>);

    impl Gate {
        fn open(&self) {
            self.0.send(()).expect("a copier waiting at the gate");
        }
    }

    fn gated_copier(landed: &Arc<AtomicU64>) -> (Copier, Gate) {
        let (gate, opened) = mpsc::channel();
        let landed = landed.clone();
        let write =
            move |_: &Screenshot| opened.recv().map(|()| thread::sleep(SLOW_WRITE)).map_err(|shut| shut.to_string());
        let copier = Copier::start_with(write, move |revision| landed.store(revision, Ordering::SeqCst), |_, _| {})
            .expect("a copy thread");
        (copier, Gate(gate))
    }

    fn closing_with_a_copy_under_way(landed: &Arc<AtomicU64>) -> (Retiring, Gate) {
        let (copier, gate) = gated_copier(landed);
        copier.copy(Job { revision: LAST_PICTURE, base: Arc::new(sample(TINY)), marks: Vec::new() });
        let mut retiring = Retiring::default();
        retiring.keep(SESSION, Some(LAST_PICTURE), Some(copier));
        (retiring, gate)
    }

    #[test]
    fn quit_waits_for_last_copy() {
        let landed = Arc::new(AtomicU64::new(NOTHING));
        let (mut retiring, gate) = closing_with_a_copy_under_way(&landed);
        assert_eq!(landed.load(Ordering::SeqCst), NOTHING, "closing the window waits for nothing");
        gate.open();
        retiring.finish();
        assert_eq!(landed.load(Ordering::SeqCst), LAST_PICTURE);
        assert!(retiring.0.is_empty());
    }

    #[test]
    fn copier_freed_after_last_report() {
        let landed = Arc::new(AtomicU64::new(NOTHING));
        let (mut retiring, _gate) = closing_with_a_copy_under_way(&landed);
        retiring.heard(SESSION, LAST_PICTURE - 1);
        retiring.heard(SESSION + 1, LAST_PICTURE);
        assert_eq!(retiring.0.len(), 1, "an older picture or another window's report ends nothing");
        retiring.heard(SESSION, LAST_PICTURE);
        assert!(retiring.0.is_empty());
    }

    #[test]
    fn closed_window_owes_last_picture() {
        let landed = Arc::new(AtomicU64::new(NOTHING));
        let (mut retiring, _gate) = closing_with_a_copy_under_way(&landed);
        assert_eq!(retiring.owes(SESSION), Some(LAST_PICTURE));
        assert_eq!(retiring.owes(SESSION + 1), None, "another window owes nothing");
        retiring.heard(SESSION, LAST_PICTURE - 1);
        assert_eq!(retiring.owes(SESSION), Some(LAST_PICTURE), "an older picture settles nothing");
        retiring.heard(SESSION, LAST_PICTURE);
        assert_eq!(retiring.owes(SESSION), None);
    }

    #[test]
    fn nothing_kept_without_debt() {
        let landed = Arc::new(AtomicU64::new(NOTHING));
        let mut retiring = Retiring::default();
        retiring.keep(SESSION, None, Some(gated_copier(&landed).0));
        retiring.keep(SESSION, Some(LAST_PICTURE), None);
        assert!(retiring.0.is_empty());
    }
}
