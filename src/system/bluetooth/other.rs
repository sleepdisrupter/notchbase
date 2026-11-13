//! Other systems report no Bluetooth devices.

use super::Device;

pub(super) struct Scanner;

impl Scanner {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn connected(&mut self) -> Option<Vec<Device>> {
        None
    }

    pub(super) fn battery(&mut self, _id: &str) -> Option<u8> {
        None
    }
}
