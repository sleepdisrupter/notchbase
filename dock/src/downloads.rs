//! Downloads coming into ~/Downloads, for a thin bar under the browser fetching them. Each browser leaves a partial
//! file there while it works (Safari `.download`, Chromium browsers `.crdownload`, Firefox and its forks `.part`),
//! and Safari and Chromium browsers also publish how far along it is, which Finder shows on the file. The partial
//! files say which browser and that something is coming; the published progress, when there is one, how far along.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Browsers by how they name a download in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Safari,
    Chromium,
    Firefox,
}

/// The browser family a partial file in Downloads belongs to, by its extension.
pub fn family_of(name: &str) -> Option<Family> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "download" => Some(Family::Safari),
        "crdownload" | "opdownload" => Some(Family::Chromium),
        "part" => Some(Family::Firefox),
        _ => None,
    }
}

const SAFARI: [&str; 2] = ["safari", "safari technology preview"];
/// By bundle name on macOS (`Google Chrome.app`), and by program name on Windows (`chrome.exe`).
const CHROMIUM: [&str; 20] = [
    "google chrome",
    "google chrome beta",
    "google chrome dev",
    "google chrome canary",
    "chromium",
    "arc",
    "brave browser",
    "brave browser beta",
    "microsoft edge",
    "microsoft edge beta",
    "vivaldi",
    "opera",
    "opera gx",
    "dia",
    "comet",
    "helium",
    "chrome",
    "msedge",
    "brave",
    "thorium",
];
const FIREFOX: [&str; 10] = [
    "firefox",
    "firefox developer edition",
    "firefox nightly",
    "zen",
    "zen browser",
    "librewolf",
    "floorp",
    "waterfox",
    "mullvad browser",
    "tor browser",
];

/// The family of a browser app, by its bundle's name (`Google Chrome.app`) or its path.
pub fn family_of_app(path: &Path) -> Option<Family> {
    let stem = path.file_stem()?.to_str()?.to_lowercase();
    let stem = stem.as_str();
    if SAFARI.contains(&stem) {
        Some(Family::Safari)
    } else if CHROMIUM.contains(&stem) {
        Some(Family::Chromium)
    } else if FIREFOX.contains(&stem) {
        Some(Family::Firefox)
    } else {
        None
    }
}

/// One download in progress: its family, and how many bytes of how many have come (`total` 0 when unknown).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub family: Family,
    pub done: i64,
    pub total: i64,
}

/// What a browser is fetching, all its downloads as one: how far along, or `None` when that isn't known for all of
/// them (the bar then glides instead of filling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fetching {
    pub family: Family,
    pub fraction: Option<f32>,
}

/// Several downloads in one browser make one bar: the bytes so far over the bytes in all, when every size is known.
pub fn combine(parts: &[Part]) -> Vec<Fetching> {
    let mut out: Vec<Fetching> = Vec::new();
    for family in [Family::Safari, Family::Chromium, Family::Firefox] {
        let mine: Vec<&Part> = parts.iter().filter(|p| p.family == family).collect();
        if mine.is_empty() {
            continue;
        }
        let known = mine.iter().all(|p| p.total > 0);
        let fraction = known.then(|| {
            let done: i64 = mine.iter().map(|p| p.done.clamp(0, p.total)).sum();
            let total: i64 = mine.iter().map(|p| p.total).sum();
            (done as f64 / total as f64).clamp(0.0, 1.0) as f32
        });
        out.push(Fetching { family, fraction });
    }
    out
}

/// Whether a published progress at `published` is the one for the partial file at `partial`. Safari publishes on
/// the `.download` bundle itself; others may publish on the file it will become.
fn same_download(published: &Path, partial: &Path) -> bool {
    published == partial || partial.with_extension("") == published || published.with_extension("") == partial
}

/// Reads ~/Downloads now and then on a thread of its own, and keeps what it finds for the dock.
pub struct Watch {
    found: Arc<Mutex<Vec<Fetching>>>,
    #[cfg(target_os = "macos")]
    _subscriber: Option<objc2::rc::Retained<objc2::runtime::AnyObject>>,
}

/// How often Downloads is read while nothing's coming, and while something is.
const QUIET: Duration = Duration::from_millis(1500);
const BUSY: Duration = Duration::from_millis(400);

impl Watch {
    /// Starts watching. Call on the main thread: published progress arrives there.
    pub fn start() -> Option<Self> {
        let dir = dirs_downloads()?;
        let found = Arc::new(Mutex::new(Vec::new()));
        #[cfg(target_os = "macos")]
        let subscriber = mac::subscribe(&dir);
        let out = found.clone();
        std::thread::Builder::new()
            .name("downloads".into())
            .spawn(move || {
                loop {
                    let parts = read(&dir);
                    let now = combine(&parts);
                    let busy = !now.is_empty();
                    if let Ok(mut slot) = out.lock() {
                        *slot = now;
                    }
                    std::thread::sleep(if busy { BUSY } else { QUIET });
                }
            })
            .ok()?;
        Some(Self {
            found,
            #[cfg(target_os = "macos")]
            _subscriber: subscriber,
        })
    }

    /// What's coming in now, by browser.
    pub fn now(&self) -> Vec<Fetching> {
        self.found.lock().map(|f| f.clone()).unwrap_or_default()
    }
}

fn dirs_downloads() -> Option<PathBuf> {
    crate::config::downloads_folder().filter(|d| d.is_dir())
}

/// The downloads in progress in `dir`, with their sizes where a browser published them.
fn read(dir: &Path) -> Vec<Part> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let partials: Vec<(PathBuf, Family)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let family = family_of(name.to_str()?)?;
            Some((e.path(), family))
        })
        .collect();
    if partials.is_empty() {
        return Vec::new();
    }
    #[cfg(target_os = "macos")]
    let published = mac::published();
    #[cfg(not(target_os = "macos"))]
    let published: Vec<(PathBuf, i64, i64)> = Vec::new();
    partials
        .into_iter()
        .map(|(path, family)| {
            let (done, total) = published
                .iter()
                .find(|(p, _, _)| same_download(p, &path))
                .map_or((0, 0), |(_, done, total)| (*done, *total));
            Part { family, done, total }
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::{Path, PathBuf};
    use std::ptr::NonNull;
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSProgress, NSString, NSURL};

    /// Progress browsers published for files in Downloads, while they're unfinished.
    static PUBLISHED: Mutex<Vec<Retained<NSProgress>>> = Mutex::new(Vec::new());

    pub(super) fn subscribe(dir: &Path) -> Option<Retained<AnyObject>> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
        let handler = RcBlock::new(|progress: NonNull<NSProgress>| -> *mut block2::DynBlock<dyn Fn()> {
            // SAFETY: the system hands over a valid progress object, which we retain.
            if let Some(progress) = unsafe { Retained::retain(progress.as_ptr()) }
                && let Ok(mut list) = PUBLISHED.lock()
            {
                list.retain(|p| !p.isFinished() && !p.isCancelled());
                list.push(progress);
            }
            std::ptr::null_mut()
        });
        // SAFETY: the block is valid for the call, which copies it.
        Some(unsafe { NSProgress::addSubscriberForFileURL_withPublishingHandler(&url, RcBlock::as_ptr(&handler)) })
    }

    /// Each unfinished published progress: its file, and the bytes done of the bytes in all (0 when unknown).
    pub(super) fn published() -> Vec<(PathBuf, i64, i64)> {
        let Ok(mut list) = PUBLISHED.lock() else { return Vec::new() };
        list.retain(|p| !p.isFinished() && !p.isCancelled());
        list.iter()
            .filter_map(|p| {
                let path = PathBuf::from(p.fileURL()?.path()?.to_string());
                let (done, total) = if p.isIndeterminate() {
                    (0, 0)
                } else if p.totalUnitCount() > 0 {
                    (p.completedUnitCount(), p.totalUnitCount())
                } else {
                    ((p.fractionCompleted().clamp(0.0, 1.0) * 1000.0) as i64, 1000)
                };
                Some((path, done, total))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_browsers_partial_file_says_which_browser_is_fetching_it() {
        assert_eq!(family_of("Movie.mp4.download"), Some(Family::Safari));
        assert_eq!(family_of("Unconfirmed 81023.crdownload"), Some(Family::Chromium));
        assert_eq!(family_of("setup.dmg.CRDOWNLOAD"), Some(Family::Chromium));
        assert_eq!(family_of("song.mp3.opdownload"), Some(Family::Chromium));
        assert_eq!(family_of("archive.zip.part"), Some(Family::Firefox));
        for done in ["Report.pdf", "download", "part", "notes.txt", "partial.zip"] {
            assert_eq!(family_of(done), None, "{done}");
        }
    }

    #[test]
    fn browsers_are_known_by_their_apps_name() {
        let app = |name: &str| PathBuf::from(format!("/Applications/{name}.app"));
        assert_eq!(family_of_app(&app("Safari")), Some(Family::Safari));
        for name in ["Google Chrome", "Arc", "Brave Browser", "Microsoft Edge", "Vivaldi"] {
            assert_eq!(family_of_app(&app(name)), Some(Family::Chromium), "{name}");
        }
        for name in ["Firefox", "Zen", "Zen Browser", "LibreWolf"] {
            assert_eq!(family_of_app(&app(name)), Some(Family::Firefox), "{name}");
        }
        assert_eq!(family_of_app(&app("Notes")), None);
    }

    #[test]
    fn several_downloads_in_one_browser_make_one_bar_weighted_by_size() {
        let part = |family, done, total| Part { family, done, total };
        let bars =
            combine(&[part(Family::Chromium, 50, 100), part(Family::Chromium, 850, 900), part(Family::Safari, 1, 4)]);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0], Fetching { family: Family::Safari, fraction: Some(0.25) });
        assert_eq!(bars[1], Fetching { family: Family::Chromium, fraction: Some(0.9) });
    }

    #[test]
    fn a_download_of_unknown_size_makes_its_browsers_bar_glide() {
        let bars = combine(&[
            Part { family: Family::Firefox, done: 10, total: 100 },
            Part { family: Family::Firefox, done: 0, total: 0 },
        ]);
        assert_eq!(bars, vec![Fetching { family: Family::Firefox, fraction: None }]);
        assert!(combine(&[]).is_empty());
    }

    #[test]
    fn progress_past_the_end_reads_as_done() {
        let bars = combine(&[Part { family: Family::Safari, done: 120, total: 100 }]);
        assert_eq!(bars[0].fraction, Some(1.0));
    }

    #[test]
    fn published_progress_matches_its_partial_file() {
        let partial = Path::new("/u/Downloads/a.zip.download");
        assert!(same_download(partial, partial));
        assert!(same_download(Path::new("/u/Downloads/a.zip"), partial));
        assert!(!same_download(Path::new("/u/Downloads/b.zip"), partial));
    }
}
