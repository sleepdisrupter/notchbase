//! A recording on Windows runs on its own `notchbase-record` thread. It marks when it started, and once asked to
//! stop it finishes the file before the thread ends.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

use super::recording::record;
use crate::poll::Wake;
use crate::record::job::{Job, Take};

pub(in crate::record) struct Session {
    stop: Arc<AtomicBool>,
    started: Arc<OnceLock<Instant>>,
    thread: Option<JoinHandle<Result<Take, String>>>,
}

impl Session {
    pub(in crate::record) fn start(job: Job, wake: Wake) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let started = Arc::new(OnceLock::new());
        let (flag, mark) = (stop.clone(), started.clone());
        let thread = std::thread::Builder::new()
            .name("notchbase-record".into())
            .spawn(move || {
                let announce = |at: Instant| {
                    let _ = mark.set(at);
                    wake();
                };
                let result = record(&job, &flag, &announce);
                wake();
                result
            })
            .map_err(|err| format!("Couldn't start recording: {err}"))?;
        Ok(Self { stop, started, thread: Some(thread) })
    }

    pub(in crate::record) fn started(&self) -> Option<Instant> {
        self.started.get().copied()
    }

    pub(in crate::record) fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub(in crate::record) fn finished(&mut self) -> Option<Result<Take, String>> {
        if !self.thread.as_ref()?.is_finished() {
            return None;
        }
        self.join()
    }

    pub(in crate::record) fn wait(&mut self) {
        self.join();
    }

    fn join(&mut self) -> Option<Result<Take, String>> {
        let thread = self.thread.take()?;
        Some(thread.join().unwrap_or_else(|_| Err("The recording stopped unexpectedly".into())))
    }
}
