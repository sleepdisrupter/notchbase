//! Keeps one notchbase running per data folder. With no data folder there's nothing to guard, so any copy may run.

use std::path::Path;

use super::imp::{HeldInstance, hold_instance};

pub struct InstanceLock {
    _held: Option<HeldInstance>,
}

impl InstanceLock {
    pub fn unguarded() -> Self {
        Self { _held: None }
    }
}

pub fn claim_instance() -> Option<InstanceLock> {
    match super::data_dir() {
        Some(dir) => claim_instance_in(&dir),
        None => Some(InstanceLock::unguarded()),
    }
}

pub fn claim_instance_in(dir: &Path) -> Option<InstanceLock> {
    hold_instance(dir).map(|held| InstanceLock { _held: Some(held) })
}
