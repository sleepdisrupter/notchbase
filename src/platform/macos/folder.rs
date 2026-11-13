//! Picks a folder with NSOpenPanel, starting in the given folder when there is one.

use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSModalResponseOK, NSOpenPanel};
use objc2_foundation::{NSString, NSURL};

pub fn pick_folder(start: Option<PathBuf>, done: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    let Some(mtm) = MainThreadMarker::new() else { return done(None) };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(true);
    if let Some(start) = start.filter(|start| start.is_dir()) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&start.to_string_lossy()));
        panel.setDirectoryURL(Some(&url));
    }
    NSApplication::sharedApplication(mtm).activate();
    let chosen = panel.runModal() == NSModalResponseOK;
    let picked =
        chosen.then(|| panel.URL()).flatten().and_then(|url| url.path()).map(|path| PathBuf::from(path.to_string()));
    done(picked);
}
