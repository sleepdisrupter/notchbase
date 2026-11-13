//! The worker loop: the next job by priority, skipping items that left the shelf, and a wake for each result.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, TryRecvError};

use super::queues::{Job, Lookup, Queues, Work};
use super::{Done, Info, Shared, lock, probe};
use crate::poll::Wake;
use crate::shelf::ops::{self, Outcome, Outputs, Source};
use crate::shelf::{ItemId, Op, Task};

pub(super) fn run(jobs: &Receiver<Job>, shared: &Mutex<Shared>, wake: &Wake, outputs: &Outputs, cancel: &AtomicBool) {
    let mut queues = Queues::default();
    while collect_jobs(jobs, &mut queues) {
        match queues.next_by_priority() {
            Some(Work::Op(op, sources)) => run_op(op, &sources, shared, wake, outputs, cancel),
            Some(Work::Info(task)) => {
                if is_live(shared, task.id) {
                    publish(shared, wake, Done::Info(probe_info(&task)));
                    queues.push_thumb(task);
                }
            }
            Some(Work::Thumb(task)) => {
                if is_live(shared, task.id)
                    && let Some(thumb) = probe_thumb(&task)
                {
                    publish(shared, wake, thumb);
                }
            }
            None => {}
        }
    }
}

fn collect_jobs(jobs: &Receiver<Job>, queues: &mut Queues) -> bool {
    if queues.is_empty() {
        match jobs.recv() {
            Ok(job) => queues.push(job),
            Err(_) => return false,
        }
    }
    loop {
        match jobs.try_recv() {
            Ok(job) => queues.push(job),
            Err(TryRecvError::Empty) => return true,
            Err(TryRecvError::Disconnected) => return false,
        }
    }
}

fn is_live(shared: &Mutex<Shared>, id: ItemId) -> bool {
    lock(shared).live_items.contains(&id)
}

fn publish(shared: &Mutex<Shared>, wake: &Wake, done: Done) {
    lock(shared).finished.push(done);
    wake();
}

fn run_op(op: Op, sources: &[Source], shared: &Mutex<Shared>, wake: &Wake, outputs: &Outputs, cancel: &AtomicBool) {
    let mut report_progress = |fraction: f32| {
        if let Some(busy) = lock(shared).running_op.as_mut() {
            busy.progress = fraction.clamp(0.0, 1.0);
        }
        wake();
    };
    let mut task = Task::new(&mut report_progress, cancel);
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| ops::run(op, sources, outputs, &mut task)))
        .unwrap_or_else(|_| Outcome::failed(format!("Couldn't {} these files", op.verb())));
    {
        let mut shared = lock(shared);
        shared.running_op = None;
        shared.finished.push(Done::Op(outcome));
    }
    wake();
}

fn probe_info(task: &Lookup) -> Info {
    panic::catch_unwind(|| probe::info(task.id, &task.path, task.kind)).unwrap_or(Info {
        id: task.id,
        bytes: None,
        dims: None,
        pages: None,
        seconds: None,
    })
}

fn probe_thumb(task: &Lookup) -> Option<Done> {
    let thumb = panic::catch_unwind(|| probe::thumbnail(&task.path, task.kind)).ok()??;
    Some(Done::Thumb { id: task.id, thumb: thumb.into() })
}
