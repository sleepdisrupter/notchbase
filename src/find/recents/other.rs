//! Other systems keep no list of recent files that Find can read.

use crate::find::context::Stamp;
use crate::find::usage::Used;

pub fn recently_used(_since: Stamp, _most: usize) -> Vec<Used> {
    Vec::new()
}
