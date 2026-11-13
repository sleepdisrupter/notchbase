//! Other systems have no output to read and no keys to take.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use super::steps::{Change, Steps};
use super::tap::TapState;
use super::{Key, Reading};

pub const STEPS: Steps = Steps { coarse: 16.0, fine: 64.0 };

pub struct Output;

impl Output {
    pub fn open() -> Self {
        Self
    }

    pub fn read(&mut self) -> Option<Reading> {
        None
    }

    pub fn apply(&mut self, _change: Change) {}
}

pub struct KeyTap;

impl KeyTap {
    pub fn install(_keys: Sender<Key>, _state: Arc<TapState>) -> Result<Self, String> {
        Err("volume keys can't be watched here".into())
    }
}
