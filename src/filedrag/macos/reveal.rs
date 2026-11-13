//! Shows a file selected in a Finder window.

use std::path::Path;

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSArray;

use super::paths::{file_url, ns_string_path};

pub(in crate::filedrag) fn reveal(path: &Path) {
    autoreleasepool(|_| {
        let Some(path) = ns_string_path(path) else {
            return;
        };
        let selection = NSArray::from_retained_slice(&[file_url(&path)]);
        NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&selection);
    });
}
