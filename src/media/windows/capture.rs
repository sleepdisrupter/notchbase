//! The output on Windows is WASAPI loopback of the default device, the same capture the screen recorder uses.

use crate::platform::Apartment;
use crate::record::Loopback;

pub struct Capture {
    loopback: Loopback,
    _apartment: Apartment,
}

impl Capture {
    pub fn open() -> Result<Self, String> {
        let apartment = Apartment::join();
        Ok(Self { loopback: Loopback::open()?, _apartment: apartment })
    }

    pub fn rate(&self) -> u32 {
        self.loopback.rate()
    }

    pub fn read(&mut self, mono: &mut Vec<f32>) -> Result<(), String> {
        self.loopback.read(mono)
    }
}
