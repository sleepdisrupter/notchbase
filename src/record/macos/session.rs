//! A recording session on macOS. It fetches the shareable content, builds the stream and starts it, then reports
//! the saved movie, or gives up waiting 10 seconds after the stop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2_foundation::NSError;
use objc2_screen_capture_kit::SCShareableContent;

use super::delegate::describe;
use super::shared::{Capture, Shared};
use super::stream::build;
use crate::poll::Wake;
use crate::record::job::{Job, Take};

/// How long a stopped recording may take to save before it's reported as unsaved.
const SAVE_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_STEP: Duration = Duration::from_millis(20);
const NOT_ALLOWED: &str = "Allow Screen Recording for notchbase in System Settings, then reopen it";
const NOT_SAVED: &str = "The recording didn't finish saving";

pub(in crate::record) struct Session {
    shared: Arc<Shared>,
}

impl Session {
    pub(in crate::record) fn start(job: Job, wake: Wake) -> Result<Self, String> {
        let shared = Arc::new(Shared::new(wake));
        let pending = shared.clone();
        let handler = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
            let Some(content) = (unsafe { content.as_ref() }) else {
                let reason = unsafe { error.as_ref() }.map_or_else(|| NOT_ALLOWED.to_owned(), describe);
                pending.finish(Err(reason));
                return;
            };
            match build(content, &job, &pending) {
                Ok(capture) => begin(capture, &pending),
                Err(reason) => pending.finish(Err(reason)),
            }
        });
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
        Ok(Self { shared })
    }

    pub(in crate::record) fn started(&self) -> Option<Instant> {
        self.shared.lock().started
    }

    pub(in crate::record) fn stop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.stop_requested = true;
            state.stopped_at.get_or_insert_with(Instant::now);
        }
        stop_stream(&self.shared);
    }

    pub(in crate::record) fn finished(&mut self) -> Option<Result<Take, String>> {
        let mut state = self.shared.lock();
        if let Some(outcome) = state.outcome.take() {
            return Some(outcome);
        }
        state.stopped_at.filter(|at| at.elapsed() > SAVE_TIMEOUT).map(|_| Err(NOT_SAVED.into()))
    }

    pub(in crate::record) fn wait(&mut self) {
        self.stop();
        let deadline = Instant::now() + SAVE_TIMEOUT;
        while self.shared.lock().outcome.is_none() && Instant::now() < deadline {
            std::thread::sleep(WAIT_STEP);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.lock().capture = None;
    }
}

fn begin(capture: Capture, shared: &Arc<Shared>) {
    let stream = capture.stream.clone();
    shared.lock().capture = Some(capture);
    let after = shared.clone();
    let on_started = RcBlock::new(move |error: *mut NSError| {
        if let Some(error) = unsafe { error.as_ref() } {
            after.finish(Err(describe(error)));
        // A stop that came while the stream was starting is carried out now.
        } else if after.lock().stop_requested {
            stop_stream(&after);
        }
    });
    unsafe { stream.startCaptureWithCompletionHandler(Some(&on_started)) };
}

fn stop_stream(shared: &Shared) {
    let stream = shared.lock().capture.as_ref().map(|capture| capture.stream.clone());
    if let Some(stream) = stream {
        unsafe { stream.stopCaptureWithCompletionHandler(None) };
    }
}
