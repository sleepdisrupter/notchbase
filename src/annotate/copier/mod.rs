//! Copies the annotated picture to the clipboard on its own thread. Only the newest waiting job is copied, and a
//! crash while copying is reported as a failure instead of taking the thread down.

use std::borrow::Cow;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use super::composite::composite;
use super::mark::Mark;
use super::sheet::Sheet;
use crate::capture::Screenshot;

const COPY_THREAD: &str = "notchbase-annotate-copy";
const CRASHED: &str = "Couldn't copy the annotated screenshot";

pub struct Job {
    pub revision: u64,
    pub base: Arc<Screenshot>,
    pub marks: Vec<Mark>,
}

impl Job {
    pub fn of(sheet: &Sheet) -> Self {
        Self { revision: sheet.revision(), base: sheet.base().clone(), marks: sheet.marks().to_vec() }
    }
}

pub struct Copier {
    jobs: Sender<Job>,
    worker: JoinHandle<()>,
}

impl Copier {
    pub fn start(
        done: impl Fn(u64) + Send + 'static,
        failed: impl Fn(u64, String) + Send + 'static,
    ) -> io::Result<Self> {
        Self::start_with(write_to_clipboard, done, failed)
    }

    pub fn start_with(
        write: impl FnMut(&Screenshot) -> Result<(), String> + Send + 'static,
        done: impl Fn(u64) + Send + 'static,
        failed: impl Fn(u64, String) + Send + 'static,
    ) -> io::Result<Self> {
        let (jobs, queue) = mpsc::channel();
        let worker = thread::Builder::new().name(COPY_THREAD.into()).spawn(move || run(&queue, write, done, failed))?;
        Ok(Self { jobs, worker })
    }

    pub fn copy(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn finish(self) {
        drop(self.jobs);
        let _ = self.worker.join();
    }
}

fn run(
    queue: &Receiver<Job>,
    mut write: impl FnMut(&Screenshot) -> Result<(), String>,
    done: impl Fn(u64),
    failed: impl Fn(u64, String),
) {
    while let Ok(first) = queue.recv() {
        let job = latest(first, queue);
        match copy(&job, &mut write) {
            Ok(()) => done(job.revision),
            Err(problem) => failed(job.revision, problem),
        }
    }
}

fn latest(first: Job, queue: &Receiver<Job>) -> Job {
    queue.try_iter().last().unwrap_or(first)
}

fn copy(job: &Job, write: &mut impl FnMut(&Screenshot) -> Result<(), String>) -> Result<(), String> {
    let attempt = AssertUnwindSafe(|| write(&composite(&job.base, &job.marks)));
    panic::catch_unwind(attempt).unwrap_or_else(|_| Err(CRASHED.to_owned()))
}

fn write_to_clipboard(shot: &Screenshot) -> Result<(), String> {
    let image = arboard::ImageData { width: shot.width, height: shot.height, bytes: Cow::Borrowed(&shot.rgba) };
    arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_image(image)).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests;
