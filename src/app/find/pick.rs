//! Opening, revealing, dragging or shelving what was picked in Find, and on Windows running an app as administrator.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::bridge::{How, Pick};
use crate::notch::Notch;
use crate::{filedrag, platform};

const NOT_THERE: &str = "is no longer there";
const NOT_OPENED: &str = "Couldn't open it";
const NOT_RUN_AS_ADMIN: &str = "Couldn't run it as administrator";
const LIFT: Duration = Duration::from_millis(240);
const PANE_ENDING: &str = "appex";

#[derive(Debug, PartialEq)]
enum Opened {
    Done,
    Missing,
    Failed(String),
}

fn open_existing(path: &Path, open: impl FnOnce(&Path) -> Result<(), String>) -> Opened {
    if std::fs::symlink_metadata(path).is_err() {
        return Opened::Missing;
    }
    match open(path) {
        Ok(()) => Opened::Done,
        Err(err) => Opened::Failed(err),
    }
}

fn way(path: &Path, how: How) -> Option<How> {
    let pane = path.extension().is_some_and(|ending| ending.eq_ignore_ascii_case(PANE_ENDING));
    match (pane, how) {
        (false, how) => Some(how),
        (true, How::Open | How::AsAdmin | How::Reveal) => Some(How::Open),
        (true, How::Drag | How::Shelve) => None,
    }
}

fn open_with_the_system(path: &Path) -> Result<(), String> {
    let address = crate::find::opens(path);
    platform::open(&address.unwrap_or_else(|| path.to_string_lossy().into_owned()))
}

impl App {
    pub(in crate::app) fn find_pick(&mut self, pick: &Pick) {
        if !self.find.reachable(&pick.path) {
            return;
        }
        let Some(how) = way(&pick.path, pick.how) else { return };
        let done = match how {
            How::Open => self.open_found(&pick.path, open_with_the_system),
            How::AsAdmin => {
                let run = self.as_admin();
                self.open_found(&pick.path, run)
            }
            How::Reveal => self.reveal_found(&pick.path),
            How::Drag => {
                self.drag_paths(std::slice::from_ref(&pick.path));
                true
            }
            How::Shelve => {
                self.panel.shelf.add(vec![pick.path.clone()]);
                true
            }
        };
        if done && let Some(service) = &self.find.service {
            service.picked(pick, self.find.context());
        }
    }

    #[cfg(windows)]
    fn as_admin(&self) -> impl FnOnce(&Path) -> Result<(), String> + use<> {
        let proxy = self.proxy.clone();
        move |path: &Path| {
            let failed = move |err| drop(proxy.send_event(crate::app::events::UserEvent::NotRunAsAdmin(err)));
            platform::run_as_admin(&path.to_string_lossy(), failed)
        }
    }

    #[cfg(not(windows))]
    fn as_admin(&self) -> impl FnOnce(&Path) -> Result<(), String> + use<> {
        open_with_the_system
    }

    pub(in crate::app) fn not_run_as_admin(&mut self, err: &str) {
        self.report(NOT_RUN_AS_ADMIN, err);
    }

    fn open_found(&mut self, path: &Path, open: impl FnOnce(&Path) -> Result<(), String>) -> bool {
        match open_existing(path, open) {
            Opened::Done => {
                let shown = self.panel.find.rows.iter().chain(self.panel.find.pins.iter());
                let lifted = shown.into_iter().any(|row| row.app && *row.path == *path);
                self.hand_over_to_what_opened(lifted);
                true
            }
            Opened::Missing => {
                self.found_missing(path);
                false
            }
            Opened::Failed(err) => {
                self.report(NOT_OPENED, err);
                false
            }
        }
    }

    fn reveal_found(&mut self, path: &Path) -> bool {
        if std::fs::symlink_metadata(path).is_err() {
            self.found_missing(path);
            return false;
        }
        filedrag::reveal(path);
        self.hand_over_to_what_opened(false);
        true
    }

    fn found_missing(&mut self, path: &Path) {
        if self.find.missing(path) {
            let name = path.file_name().map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
            self.notify(&format!("{name} {NOT_THERE}"));
        }
        self.find_unpin(path);
        self.show_find();
    }

    fn hand_over_to_what_opened(&mut self, lifted: bool) {
        self.forget_the_way_back();
        if lifted {
            self.find.lifting = Some(Instant::now() + LIFT);
            self.redraw_notch();
        } else {
            self.update_notch(Notch::collapse);
        }
    }

    pub(in crate::app) fn sync_launch(&mut self, now: Instant) {
        if self.find.lifting.is_none_or(|at| at > now) {
            return;
        }
        self.find.lifting = None;
        if let Some(nw) = self.notches.active_mut() {
            if nw.notch.finding() {
                nw.notch.collapse();
            }
            nw.notch.swallow();
            nw.redraw();
        }
    }

    pub(in crate::app) fn launch_wakeup(&self) -> Option<Instant> {
        self.find.lifting
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn settings_pane_only_opens() {
        let pane = Path::new("/System/Library/ExtensionKit/Extensions/Bluetooth.appex");
        assert_eq!((way(pane, How::Open), way(pane, How::Reveal)), (Some(How::Open), Some(How::Open)));
        assert_eq!((way(pane, How::Drag), way(pane, How::Shelve)), (None, None), "there is no file to hand over");
        assert_eq!(way(pane, How::AsAdmin), Some(How::Open), "a settings pane opens as it always does");
        for how in [How::Open, How::AsAdmin, How::Reveal, How::Drag, How::Shelve] {
            assert_eq!(way(Path::new("/Applications/Maps.app"), how), Some(how));
        }
    }

    #[test]
    fn missing_path_makes_no_folder() {
        let missing = std::env::temp_dir().join(format!("notchbase-find-missing-{}", std::process::id()));
        let path = missing.join("README");
        let asked = Cell::new(false);
        let opened = open_existing(&path, |_| {
            asked.set(true);
            Ok(())
        });
        assert_eq!(opened, Opened::Missing);
        assert!(!asked.get(), "the system is never asked to open what is not there");
        assert!(!missing.exists() && !path.exists());
    }

    #[test]
    fn existing_file_goes_to_system() {
        let file = std::env::temp_dir().join(format!("notchbase-find-present-{}.txt", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(open_existing(&file, |path| if path == file { Ok(()) } else { Err("other".into()) }), Opened::Done);
        assert_eq!(open_existing(&file, |_| Err("no app".into())), Opened::Failed("no app".into()));
        let _ = std::fs::remove_file(&file);
    }
}
