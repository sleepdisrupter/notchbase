//! Paths as NSString and file URLs. A path that isn't UTF-8 has no NSString here.

use std::path::Path;

use objc2::rc::Retained;
use objc2_foundation::{NSString, NSURL};

pub(super) fn ns_string_path(path: &Path) -> Option<Retained<NSString>> {
    path.to_str().map(NSString::from_str)
}

pub(super) fn file_url(path: &NSString) -> Retained<NSURL> {
    NSURL::fileURLWithPath(path)
}
