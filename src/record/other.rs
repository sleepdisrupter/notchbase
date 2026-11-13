//! Other systems have no screen recording.

use std::time::Instant;

use super::job::{Job, Take};
use crate::poll::Wake;

pub(super) const EXTENSION: &str = "mp4";

pub(super) struct Session;

impl Session {
    pub(super) fn start(_job: Job, _wake: Wake) -> Result<Self, String> {
        Err("Screen recording isn't available on this system".into())
    }

    pub(super) fn started(&self) -> Option<Instant> {
        None
    }

    pub(super) fn stop(&mut self) {}

    pub(super) fn finished(&mut self) -> Option<Result<Take, String>> {
        None
    }

    pub(super) fn wait(&mut self) {}
}
