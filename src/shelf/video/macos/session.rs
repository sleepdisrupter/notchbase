//! Runs an export session, reports its progress, and cancels it cleanly when asked.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use block2::RcBlock;
use objc2_av_foundation::{AVAssetExportSession, AVAssetExportSessionStatus};

use super::super::MediaError;
use super::diagnose;
use super::wait::{self, Wait};
use crate::shelf::Task;

const CANCEL_GRACE: Duration = Duration::from_secs(5);

pub(super) fn run(session: &AVAssetExportSession, source: &Path, task: &mut Task) -> Result<(), MediaError> {
    let (sender, finished) = mpsc::channel();
    start(session, sender);
    match wait::until(&finished, task, None, |task| task.report(unsafe { session.progress() })) {
        Wait::Cancelled => {
            cancel(session, &finished);
            Err(MediaError::Cancelled)
        }
        Wait::Ready(()) | Wait::Abandoned | Wait::Stalled => outcome(session, source, task),
    }
}

fn start(session: &AVAssetExportSession, sender: Sender<()>) {
    let on_finished = RcBlock::new(move || {
        let _ = sender.send(());
    });
    unsafe { session.exportAsynchronouslyWithCompletionHandler(&on_finished) };
}

fn cancel(session: &AVAssetExportSession, finished: &Receiver<()>) {
    unsafe { session.cancelExport() };
    let _ = finished.recv_timeout(CANCEL_GRACE);
}

fn outcome(session: &AVAssetExportSession, source: &Path, task: &mut Task) -> Result<(), MediaError> {
    match unsafe { session.status() } {
        AVAssetExportSessionStatus::Completed => {
            task.report(1.0);
            Ok(())
        }
        AVAssetExportSessionStatus::Cancelled => Err(MediaError::Cancelled),
        _ => Err(diagnose::failure("export", source, unsafe { session.error() }.as_deref())),
    }
}
