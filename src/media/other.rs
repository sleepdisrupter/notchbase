//! Other systems have no media sessions and no output capture.

use super::track::{Control, Track};

const UNSUPPORTED: &str = "Listening to system audio isn't supported here";

pub struct Source;

impl Source {
    pub fn new() -> Self {
        Self
    }

    pub fn poll(&mut self) -> Option<Track> {
        None
    }

    pub fn artwork(&mut self) -> Option<Vec<u8>> {
        None
    }

    pub fn control(&mut self, _control: Control) {}
}

pub struct Capture;

impl Capture {
    pub fn open() -> Result<Self, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn rate(&self) -> u32 {
        48_000
    }

    pub fn read(&mut self, _mono: &mut Vec<f32>) -> Result<(), String> {
        Ok(())
    }
}
