//! What the copier thread reports about each picture: copied, or why not.

use std::io;

use crate::annotate::Copier;
use crate::poll::{Mailbox, Wake};

#[derive(Debug, PartialEq)]
pub(super) enum Report {
    Copied(u64),
    NotCopied(u64, String),
}

impl Report {
    pub(super) fn revision(&self) -> u64 {
        match self {
            Self::Copied(revision) | Self::NotCopied(revision, _) => *revision,
        }
    }
}

#[derive(Clone)]
pub(super) struct Reports(Mailbox<(u64, Report)>);

impl Reports {
    pub(super) fn new(wake: Wake) -> Self {
        Self(Mailbox::new(wake))
    }

    pub(super) fn copier(&self, session: u64) -> io::Result<Copier> {
        let (done, failed) = (self.reporter(session), self.reporter(session));
        Copier::start(
            move |revision| done(Report::Copied(revision)),
            move |revision, problem| failed(Report::NotCopied(revision, problem)),
        )
    }

    pub(super) fn take(&self) -> Vec<(u64, Report)> {
        self.0.take()
    }

    fn reporter(&self, session: u64) -> impl Fn(Report) + Send + 'static {
        let filed = self.0.clone();
        move |report| filed.post((session, report))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn reports_filed_by_session() {
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = woken.clone();
        let reports = Reports::new(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        let (first, second) = (reports.reporter(1), reports.reporter(2));
        first(Report::Copied(4));
        second(Report::NotCopied(7, "The clipboard is busy".to_owned()));
        assert_eq!(woken.load(Ordering::SeqCst), 2);
        let filed = reports.take();
        assert_eq!(filed, [(1, Report::Copied(4)), (2, Report::NotCopied(7, "The clipboard is busy".to_owned()))]);
        assert!(reports.take().is_empty(), "a report is heard once");
    }

    #[test]
    fn report_covers_one_picture() {
        assert_eq!(Report::Copied(4).revision(), 4);
        assert_eq!(Report::NotCopied(7, "The clipboard is busy".to_owned()).revision(), 7);
    }
}
