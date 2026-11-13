//! State shared between a session and the system's callbacks. The first outcome wins and every change wakes the app.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use objc2::rc::Retained;
use objc2_screen_capture_kit::{SCRecordingOutput, SCStream};

use super::delegate::Delegate;
use crate::poll::Wake;
use crate::record::job::Take;

/// The running stream, with its output and delegate kept alive alongside it.
pub(super) struct Capture {
    pub(super) stream: Retained<SCStream>,
    pub(super) _output: Retained<SCRecordingOutput>,
    pub(super) _delegate: Retained<Delegate>,
}

unsafe impl Send for Capture {}

#[derive(Default)]
pub(super) struct State {
    pub(super) started: Option<Instant>,
    pub(super) outcome: Option<Result<Take, String>>,
    pub(super) capture: Option<Capture>,
    pub(super) stop_requested: bool,
    pub(super) stopped_at: Option<Instant>,
}

pub(super) struct Shared {
    state: Mutex<State>,
    wake: Wake,
}

impl Shared {
    pub(super) fn new(wake: Wake) -> Self {
        Self { state: Mutex::new(State::default()), wake }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn started(&self) {
        self.lock().started.get_or_insert_with(Instant::now);
        (self.wake)();
    }

    pub(super) fn finish(&self, outcome: Result<Take, String>) {
        {
            let mut state = self.lock();
            if state.outcome.is_none() {
                state.outcome = Some(outcome);
            }
        }
        (self.wake)();
    }
}
