//! Without sips there is no fallback decoder.

use std::path::Path;

pub(super) fn to_png(_path: &Path, _max_side: Option<u32>) -> Option<Vec<u8>> {
    None
}

pub(super) fn dimensions(_path: &Path) -> Option<[u32; 2]> {
    None
}
