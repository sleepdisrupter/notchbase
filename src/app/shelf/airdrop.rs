//! Sending Shelf files with AirDrop on a Mac, folding the notch away once they lift off, and saying how it went.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::notch::Notch;
use crate::platform::{self, FrontApp};
use crate::poll::Wake;
use crate::shelf::airdrop::{self, Ended, Finished};
use crate::shelf::{ItemId, Notice};

/// How long the tiles take to lift off the Shelf before the notch folds away.
const LIFT_MS: u64 = 450;

pub(in crate::app) struct AirDrops {
    wake: Wake,
    last_id: u64,
    underway: HashMap<u64, Underway>,
    collapse_at: Option<Instant>,
}

struct Underway {
    what: String,
    front: Option<FrontApp>,
}

impl AirDrops {
    pub(in crate::app) fn new(wake: Wake) -> Self {
        Self { wake, last_id: 0, underway: HashMap::new(), collapse_at: None }
    }
}

impl App {
    pub(in crate::app) fn airdrop_items(&mut self, items: &[ItemId]) {
        let files = self.shelf_paths(items);
        if files.is_empty() {
            return;
        }
        let drops = &mut self.airdrops;
        drops.last_id += 1;
        let id = drops.last_id;
        let front = platform::remember_front_app();
        match airdrop::send(id, &files, drops.wake.clone()) {
            Ok(()) => {
                drops.underway.insert(id, Underway { what: what(&files), front });
                drops.collapse_at = Some(Instant::now() + Duration::from_millis(LIFT_MS));
                self.forget_the_way_back();
            }
            Err(reason) => self.post_shelf_notice(Notice { text: reason, ok: false }, Instant::now()),
        }
    }

    pub(in crate::app) fn sync_airdrops(&mut self, now: Instant) {
        if self.airdrops.collapse_at.is_some_and(|at| now >= at) {
            self.airdrops.collapse_at = None;
            self.update_notch(Notch::collapse);
        }
        for Finished { id, ended } in airdrop::take_finished() {
            let Some(underway) = self.airdrops.underway.remove(&id) else { continue };
            if let Some(app) = underway.front {
                platform::restore_front_app(app);
            }
            if let Some(notice) = notice(&underway.what, &ended) {
                self.post_shelf_notice(notice, now);
            }
        }
    }

    pub(in crate::app) fn airdrop_wakeup(&self) -> Option<Instant> {
        self.airdrops.collapse_at
    }
}

fn what(files: &[PathBuf]) -> String {
    let name = |path: &PathBuf| path.file_name().map(|name| name.to_string_lossy().into_owned());
    match files {
        [one] => name(one).unwrap_or_else(|| "1 file".to_owned()),
        many => format!("{} files", many.len()),
    }
}

fn notice(what: &str, ended: &Ended) -> Option<Notice> {
    match ended {
        Ended::Sent => Some(Notice { text: format!("AirDropped {what}"), ok: true }),
        Ended::Failed(_) => Some(Notice { text: format!("Couldn't AirDrop {what}"), ok: false }),
        Ended::Cancelled => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_named_several_counted() {
        assert_eq!(what(&[PathBuf::from("/Users/john/Desktop/photo.png")]), "photo.png");
        assert_eq!(what(&[PathBuf::from("/a.png"), PathBuf::from("/b"), PathBuf::from("/c.pdf")]), "3 files");
        assert_eq!(what(&[PathBuf::from("/")]), "1 file");
    }

    #[test]
    fn outcome_notices() {
        let sent = notice("photo.png", &Ended::Sent).unwrap();
        assert_eq!((sent.text.as_str(), sent.ok), ("AirDropped photo.png", true));
        let failed = notice("3 files", &Ended::Failed("Declined".into())).unwrap();
        assert_eq!((failed.text.as_str(), failed.ok), ("Couldn't AirDrop 3 files", false));
        assert_eq!(notice("photo.png", &Ended::Cancelled), None);
    }
}
