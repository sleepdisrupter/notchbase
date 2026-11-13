//! Files used lately, from the system, for the empty field. Only macOS keeps them, through Spotlight.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as imp;

use super::context::Stamp;
use super::usage::Used;

pub fn recently_used(since: Stamp, most: usize) -> Vec<Used> {
    imp::recently_used(since, most)
}
