//! Other systems tell Find nothing about their apps.

use std::collections::HashMap;
use std::path::Path;

use crate::find::key::Key;

use super::{Read, Used};

pub fn read(_path: &Path) -> Read {
    Read::default()
}

pub fn every() -> Vec<std::path::PathBuf> {
    Vec::new()
}

pub fn used() -> Option<HashMap<Key, Used>> {
    None
}

pub fn settle(_menu: Option<&Path>) {}

pub fn opens(_path: &Path) -> Option<String> {
    None
}
