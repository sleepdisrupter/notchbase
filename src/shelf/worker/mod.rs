//! The shelf's worker thread. It reads each new item's size, pages, length and thumbnail and runs one operation at a
//! time, handing results back through `take`.

mod probe;
mod queues;
mod thread;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SendError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui::ColorImage;

use super::ops::{Outcome, Outputs, Source};
use super::{ItemId, Kind, Op};
use crate::poll::Wake;
use queues::{Job, Lookup};

const THREAD_NAME: &str = "notchbase-shelf";

pub(super) struct Info {
    pub(super) id: ItemId,
    pub(super) bytes: Option<u64>,
    pub(super) dims: Option<[u32; 2]>,
    pub(super) pages: Option<u32>,
    pub(super) seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Busy {
    pub op: Op,
    pub progress: f32,
    pub items: Vec<ItemId>,
}

pub(super) enum Done {
    Info(Info),
    Thumb { id: ItemId, thumb: Arc<ColorImage> },
    Op(Outcome),
}

#[derive(Default)]
struct Shared {
    finished: Vec<Done>,
    running_op: Option<Busy>,
    live_items: HashSet<ItemId>,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) struct Worker {
    shared: Arc<Mutex<Shared>>,
    jobs: Option<Sender<Job>>,
    cancel: Arc<AtomicBool>,
    wake: Wake,
    outputs: Outputs,
}

impl Worker {
    pub(super) fn new(wake: Wake, outputs: Outputs) -> Self {
        Self { shared: Arc::default(), jobs: None, cancel: Arc::default(), wake, outputs }
    }

    pub(super) fn watch(&mut self, id: ItemId, path: PathBuf, kind: Kind) {
        lock(&self.shared).live_items.insert(id);
        self.send(Job::Probe(Lookup { id, path, kind }));
    }

    pub(super) fn forget(&mut self, id: ItemId) {
        lock(&self.shared).live_items.remove(&id);
    }

    pub(super) fn start(&mut self, op: Op, items: Vec<ItemId>, sources: Vec<Source>) -> bool {
        {
            let mut shared = lock(&self.shared);
            if shared.running_op.is_some() {
                return false;
            }
            shared.running_op = Some(Busy { op, progress: 0.0, items });
        }
        self.cancel.store(false, Ordering::Relaxed);
        let sent = self.send(Job::Op(op, sources));
        if !sent {
            lock(&self.shared).running_op = None;
        }
        sent
    }

    pub(super) fn busy(&self) -> Option<Busy> {
        lock(&self.shared).running_op.clone()
    }

    pub(super) fn cancel(&self) {
        if lock(&self.shared).running_op.is_some() {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn take(&self) -> (Vec<Done>, Option<Busy>) {
        let mut shared = lock(&self.shared);
        (std::mem::take(&mut shared.finished), shared.running_op.clone())
    }

    fn send(&mut self, job: Job) -> bool {
        let job = match &self.jobs {
            Some(jobs) => match jobs.send(job) {
                Ok(()) => return true,
                Err(SendError(job)) => job,
            },
            None => job,
        };
        self.jobs = self.spawn();
        self.jobs.as_ref().is_some_and(|jobs| jobs.send(job).is_ok())
    }

    fn spawn(&self) -> Option<Sender<Job>> {
        let (sender, receiver) = mpsc::channel();
        let shared = self.shared.clone();
        let wake = self.wake.clone();
        let outputs = self.outputs.clone();
        let cancel = self.cancel.clone();
        std::thread::Builder::new()
            .name(THREAD_NAME.into())
            .spawn(move || thread::run(&receiver, &shared, &wake, &outputs, &cancel))
            .ok()
            .map(|_| sender)
    }
}
