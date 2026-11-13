//! The downloads thread: once access is granted it lists the folder every second and queues the changes for the app.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::{Download, DownloadChange};

use super::tracker::{Outcome, Tracker};
use super::{imp, scan};
use crate::poll::{Outbox, Shift, Worker};
use crate::system::permissions::{Access, Gate};

const THREAD: &str = "notchbase-downloads";
const EVERY: Duration = Duration::from_secs(1);
const NAP: Duration = Duration::from_millis(100);
const JUST_LANDED: Duration = Duration::from_secs(10);
const RENAMED_ON_FINISH: &str = ".part";

pub struct Downloads {
    worker: Worker<Outbox<DownloadChange>>,
}

impl Downloads {
    pub fn start(folder: PathBuf) -> Self {
        Self { worker: Worker::start(THREAD, NAP, Outbox::default(), move |shift| watch(shift, &folder)) }
    }

    pub fn take_changes(&self) -> Vec<DownloadChange> {
        self.worker.state().take_all()
    }
}

fn watch(shift: &Shift<Outbox<DownloadChange>>, folder: &Path) {
    let mut tracker = Tracker::default();
    let mut published: Option<imp::Subscription> = None;
    let mut gate = Gate::new(Access::Downloads);
    while shift.on_duty() {
        if !gate.open() {
            shift.nap_until(Instant::now() + EVERY, |_| false);
            continue;
        }
        if let Some(mut listing) = scan::list(folder) {
            if let Some(published) = &published {
                published.fill(&mut listing);
            }
            let outcomes = tracker.observe(&listing, Instant::now());
            if !outcomes.is_empty() {
                shift.state().extend(outcomes.into_iter().map(|outcome| change(folder, outcome)));
            }
        }
        published.get_or_insert_with(|| imp::Subscription::start(folder));
        shift.nap_until(Instant::now() + EVERY, |_| false);
    }
}

fn change(folder: &Path, outcome: Outcome) -> DownloadChange {
    match outcome {
        Outcome::Progress(download) => DownloadChange::Progress { download },
        Outcome::Finished(download, name) => finished(folder, download, &name),
        Outcome::Stalled(download) => DownloadChange::Cancelled { download },
        Outcome::Lost(download) if download.id.ends_with(RENAMED_ON_FINISH) => {
            match scan::newest(folder, JUST_LANDED) {
                Some(name) => finished(folder, Download { name: name.clone(), ..download }, &name),
                None => DownloadChange::Cancelled { download },
            }
        }
        Outcome::Lost(download) => DownloadChange::Cancelled { download },
    }
}

fn finished(folder: &Path, download: Download, name: &str) -> DownloadChange {
    let path = folder.join(name);
    let size = std::fs::metadata(&path).map_or(download.received, |meta| meta.len());
    let download = Download { received: size, total: Some(size), ..download };
    DownloadChange::Finished { download, path: path.to_string_lossy().into_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOON: Duration = Duration::from_secs(10);

    fn wait_for(downloads: &Downloads, wanted: impl Fn(&DownloadChange) -> bool) -> DownloadChange {
        let deadline = Instant::now() + SOON;
        loop {
            if let Some(change) = downloads.take_changes().into_iter().find(&wanted) {
                return change;
            }
            assert!(Instant::now() < deadline, "the download was never reported");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn reported_until_it_lands() {
        let folder = std::env::temp_dir().join(format!("notchbase-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let downloads = Downloads::start(folder.clone());
        std::thread::sleep(EVERY + NAP * 3);
        std::fs::write(folder.join("clip.mp4.crdownload"), [0u8; 6]).unwrap();
        let started = wait_for(&downloads, |change| matches!(change, DownloadChange::Progress { .. }));
        let DownloadChange::Progress { download } = started else { unreachable!() };
        assert_eq!((download.name.as_str(), download.received), ("clip.mp4", 6));
        std::fs::write(folder.join("clip.mp4.crdownload"), [0u8; 9]).unwrap();
        std::fs::rename(folder.join("clip.mp4.crdownload"), folder.join("clip.mp4")).unwrap();
        let landed = wait_for(&downloads, |change| matches!(change, DownloadChange::Finished { .. }));
        let DownloadChange::Finished { download, path } = landed else { unreachable!() };
        assert_eq!((download.received, download.total), (9, Some(9)));
        assert_eq!(PathBuf::from(path), folder.join("clip.mp4"));
        drop(downloads);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
