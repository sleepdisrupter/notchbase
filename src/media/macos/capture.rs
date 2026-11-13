//! The output on a Mac comes from the Core Audio process tap in `audio::Listener`.

use crate::audio::Listener;

pub struct Capture {
    listener: Listener,
}

impl Capture {
    pub fn open() -> Result<Self, String> {
        Ok(Self { listener: Listener::open()? })
    }

    pub fn rate(&self) -> u32 {
        self.listener.rate()
    }

    pub fn read(&mut self, mono: &mut Vec<f32>) -> Result<(), String> {
        self.listener.read(mono);
        Ok(())
    }
}
